//! Async serial multiplexer — fan-out serial data to multiple clients.

use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast;

use crate::consonance;
use crate::protocol::ConsonanceMessage;

/// Shared state for a serial port multiplexer.
pub struct Multiplexer {
    pub alias: String,
    pub device: Option<String>,
    pub baud: u32,
    /// Broadcast channel for raw serial output.
    output_tx: broadcast::Sender<Vec<u8>>,
    /// Broadcast channel for consonance events.
    consonance_tx: broadcast::Sender<ConsonanceMessage>,
    /// Current consonance heatmap.
    pub heatmap: Arc<std::sync::Mutex<crate::consonance::ConsonanceHeatmap>>,
    /// Connected clients count.
    pub client_count: std::sync::atomic::AtomicUsize,
    /// Most recent frequency for pairwise consonance.
    last_freq: std::sync::Mutex<f64>,
    /// History log lines.
    pub history: std::sync::Mutex<Vec<String>>,
}

impl Multiplexer {
    pub fn new(alias: String, device: Option<String>, baud: u32) -> Self {
        let (output_tx, _) = broadcast::channel(256);
        let (consonance_tx, _) = broadcast::channel(256);
        let heatmap = crate::consonance::ConsonanceHeatmap::new(24, 80.0, 4000.0);

        Self {
            alias,
            device,
            baud,
            output_tx,
            consonance_tx,
            heatmap: Arc::new(std::sync::Mutex::new(heatmap)),
            client_count: std::sync::atomic::AtomicUsize::new(0),
            last_freq: std::sync::Mutex::new(0.0),
            history: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Subscribe to raw serial output.
    pub fn subscribe_output(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    /// Subscribe to consonance events.
    pub fn subscribe_consonance(&self) -> broadcast::Receiver<ConsonanceMessage> {
        self.consonance_tx.subscribe()
    }

    /// Process raw bytes from serial, extract musical data, and fan out.
    pub fn process_serial_data(&self, data: &[u8]) {
        // Fan out raw data
        let _ = self.output_tx.send(data.to_vec());

        // Try to detect frequency data in the stream
        self.try_detect_frequencies(data);

        // Log line-based output
        let text = String::from_utf8_lossy(data);
        let mut history = self.history.lock().unwrap();
        for line in text.lines() {
            if !line.is_empty() {
                let ts = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]").to_string();
                history.push(format!("{} {}", ts, line));
                if history.len() > 5000 {
                    let drain = history.len() - 5000;
                    history.drain(..drain);
                }
            }
        }
    }

    /// Try to extract frequencies from serial data and compute consonance.
    fn try_detect_frequencies(&self, data: &[u8]) {
        // Check for binary Consonance Protocol
        if data.len() >= 5 {
            let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
            if len > 0 && len <= data.len() - 4 && len < 1024 {
                if let Some(msg) = ConsonanceMessage::decode(&data[4..4 + len]) {
                    let _ = self.consonance_tx.send(msg);
                    return;
                }
            }
        }

        // Check for text frequency format
        let text = String::from_utf8_lossy(data);
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("F:") {
                if let Ok(freq) = rest.parse::<f64>() {
                    self.emit_frequency(freq, 0);
                }
            } else if let Some(rest) = trimmed.strip_prefix("NOTE:") {
                let parts: Vec<&str> = rest.split(':').collect();
                if parts.len() >= 2 {
                    if let Ok(freq) = parts[1].parse::<f64>() {
                        self.emit_frequency(freq, 0);
                    }
                }
            }
        }

        // Check for MIDI-like 3-byte messages
        if data.len() >= 3 {
            let status = data[0];
            if status >= 0x80 && status < 0xF0 {
                let channel = (status & 0x0F) as u8;
                let note = data[1];
                let _velocity = data[2];
                let freq = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
                self.emit_frequency(freq, channel);
            }
        }
    }

    /// Emit a consonance event for a detected frequency.
    fn emit_frequency(&self, freq: f64, voice_id: u8) {
        let lattice = consonance::ratio_to_lattice(freq / 440.0);
        let score = {
            let last = self.last_freq.lock().unwrap();
            if *last > 0.0 {
                consonance::consonance_score(freq, *last)
            } else {
                1.0
            }
        };

        // Update heatmap
        {
            let last = self.last_freq.lock().unwrap();
            if *last > 0.0 {
                let mut hm = self.heatmap.lock().unwrap();
                hm.record_pair(*last, freq);
            }
        }

        *self.last_freq.lock().unwrap() = freq;

        let msg = ConsonanceMessage::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            freq,
            lattice,
            score,
            voice_id,
        );

        let _ = self.consonance_tx.send(msg);
    }

    /// Get current history.
    pub fn get_history(&self) -> Vec<String> {
        self.history.lock().unwrap().clone()
    }

    /// Increment client count.
    pub fn add_client(&self) {
        self.client_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Decrement client count.
    pub fn remove_client(&self) {
        self.client_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get client count.
    pub fn client_count(&self) -> usize {
        self.client_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Serial reader task — reads from serial port and feeds the multiplexer.
pub async fn serial_reader(mux: Arc<Multiplexer>, device: String, baud: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio_serial::SerialPortBuilderExt;
    let mut serial = tokio_serial::new(&device, baud).open_native_async()?;
    let mut buf = [0u8; 4096];

    tracing::info!("Serial reader started: {} @ {}", device, baud);

    loop {
        match serial.read(&mut buf).await {
            Ok(0) => {
                tracing::warn!("Serial port closed");
                break;
            }
            Ok(n) => {
                mux.process_serial_data(&buf[..n]);
            }
            Err(e) => {
                tracing::error!("Serial read error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consonance::LatticePoint;

    #[test]
    fn test_multiplexer_fan_out() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_output();

        mux.process_serial_data(b"hello\n");

        let received = rx.try_recv().unwrap();
        assert_eq!(received, b"hello\n".to_vec());
    }

    #[test]
    fn test_multiplexer_client_count() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        assert_eq!(mux.client_count(), 0);
        mux.add_client();
        assert_eq!(mux.client_count(), 1);
        mux.add_client();
        assert_eq!(mux.client_count(), 2);
        mux.remove_client();
        assert_eq!(mux.client_count(), 1);
    }

    #[test]
    fn test_frequency_detection_text() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        mux.process_serial_data(b"F:440.0\n");

        let msg = rx.try_recv().unwrap();
        assert!((msg.frequency - 440.0).abs() < 0.1);
        assert_eq!(msg.voice_id, 0);
    }

    #[test]
    fn test_frequency_detection_midi() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        // MIDI note 69 (A4) on channel 0
        mux.process_serial_data(&[0x90, 69, 100]);

        let msg = rx.try_recv().unwrap();
        assert!((msg.frequency - 440.0).abs() < 1.0);
        assert_eq!(msg.voice_id, 0);
    }

    #[test]
    fn test_history_logging() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        mux.process_serial_data(b"line1\nline2\n");

        let history = mux.get_history();
        assert!(history.iter().any(|l| l.contains("line1")));
        assert!(history.iter().any(|l| l.contains("line2")));
    }

    #[test]
    fn test_multiplexer_alias_and_device() {
        let mux = Arc::new(Multiplexer::new(
            "my-synth".to_string(),
            Some("/dev/ttyUSB0".to_string()),
            9600,
        ));
        assert_eq!(mux.alias, "my-synth");
        assert_eq!(mux.device, Some("/dev/ttyUSB0".to_string()));
        assert_eq!(mux.baud, 9600);
    }

    #[test]
    fn test_binary_protocol_detection() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        // Create a valid framed ConsonanceMessage
        let msg = ConsonanceMessage::new(1000, 330.0, LatticePoint::new(0, 0, 0), 0.8, 2);
        let framed = msg.encode_framed();
        mux.process_serial_data(&framed);

        let received = rx.try_recv().unwrap();
        assert!((received.frequency - 330.0).abs() < 0.1);
    }

    #[test]
    fn test_note_format_detection() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        mux.process_serial_data(b"NOTE:C4:261.63\n");

        let msg = rx.try_recv().unwrap();
        assert!((msg.frequency - 261.63).abs() < 0.1);
    }

    #[test]
    fn test_midi_channel_extraction() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        // MIDI note on channel 3 (status 0x93)
        mux.process_serial_data(&[0x93, 60, 80]);

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.voice_id, 3);
    }

    #[test]
    fn test_history_cap() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        // Exceed the 5000 line cap
        for i in 0..5100 {
            let data = format!("line{}\n", i);
            mux.process_serial_data(data.as_bytes());
        }
        let history = mux.get_history();
        assert!(history.len() <= 5000, "history should be capped at 5000, got {}", history.len());
    }

    #[test]
    fn test_heatmap_normalized_after_updates() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        mux.process_serial_data(b"F:200.0\n");
        mux.process_serial_data(b"F:300.0\n");

        let hm = mux.heatmap.lock().unwrap();
        let norm = hm.normalized();
        let max = norm.iter().flat_map(|r| r.iter()).copied().fold(0.0f32, f32::max);
        assert!((max - 1.0).abs() < 0.01, "max should be ~1.0, got {max}");
    }

    #[test]
    fn test_multiple_subscribers() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx1 = mux.subscribe_output();
        let mut rx2 = mux.subscribe_output();

        mux.process_serial_data(b"broadcast\n");

        assert_eq!(rx1.try_recv().unwrap(), b"broadcast\n".to_vec());
        assert_eq!(rx2.try_recv().unwrap(), b"broadcast\n".to_vec());
    }

    #[test]
    fn test_consonance_score_between_two_notes() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        let mut rx = mux.subscribe_consonance();

        // First note — consonance should be 1.0 (no previous)
        mux.process_serial_data(b"F:440.0\n");
        let msg1 = rx.try_recv().unwrap();
        assert!((msg1.consonance - 1.0).abs() < 0.01);

        // Second note — perfect fifth, should be consonant
        mux.process_serial_data(b"F:660.0\n");
        let msg2 = rx.try_recv().unwrap();
        assert!(msg2.consonance > 0.0, "fifth should have positive consonance: {}", msg2.consonance);
    }

    #[test]
    fn test_history_empty_lines_skipped() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        mux.process_serial_data(b"\n\nhello\n\n");
        let history = mux.get_history();
        assert_eq!(history.len(), 1);
        assert!(history[0].contains("hello"));
    }

    #[test]
    fn test_client_count_wraps() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));
        mux.remove_client(); // remove without add — wraps (usize behavior)
        // Just verify it doesn't panic
    }

    #[test]
    fn test_heatmap_updates() {
        let mux = Arc::new(Multiplexer::new("test".to_string(), None, 115200));

        mux.process_serial_data(b"F:440.0\n");
        mux.process_serial_data(b"F:660.0\n");

        let hm = mux.heatmap.lock().unwrap();
        let total: f32 = hm.heatmap.iter().flat_map(|r| r.iter()).sum();
        assert!(total > 0.0);
    }
}
