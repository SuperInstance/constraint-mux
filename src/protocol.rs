//! Binary consonance protocol for musical instruments over serial.
//!
//! Wire format (bincode):
//! - timestamp_ns: u64 (nanoseconds since epoch)
//! - frequency: f64 (Hz)
//! - lattice: (i8, i8, i8) = 2^a × 3^b × 5^c
//! - consonance: f32 (pre-computed score)
//! - voice_id: u8

use crate::consonance::{ConsonanceEvent, LatticePoint};
use serde::{Deserialize, Serialize};

/// Wire-level message for the consonance protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsonanceMessage {
    pub timestamp_ns: u64,
    pub frequency: f64,
    pub lattice_a: i8,
    pub lattice_b: i8,
    pub lattice_c: i8,
    pub consonance: f32,
    pub voice_id: u8,
}

impl ConsonanceMessage {
    pub fn new(timestamp_ns: u64, frequency: f64, lattice: LatticePoint, consonance: f32, voice_id: u8) -> Self {
        Self {
            timestamp_ns,
            frequency,
            lattice_a: lattice.a,
            lattice_b: lattice.b,
            lattice_c: lattice.c,
            consonance,
            voice_id,
        }
    }

    /// Encode to binary using bincode.
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).expect("bincode serialize")
    }

    /// Decode from binary.
    pub fn decode(data: &[u8]) -> Option<Self> {
        bincode::deserialize(data).ok()
    }

    /// Encode with a 4-byte length prefix (for framing).
    pub fn encode_framed(&self) -> Vec<u8> {
        let payload = self.encode();
        let len = payload.len() as u32;
        let mut buf = len.to_be_bytes().to_vec();
        buf.extend_from_slice(&payload);
        buf
    }

    /// Convert to a ConsonanceEvent.
    pub fn to_event(&self) -> ConsonanceEvent {
        ConsonanceEvent {
            timestamp_ns: self.timestamp_ns,
            frequency: self.frequency,
            lattice: LatticePoint::new(self.lattice_a, self.lattice_b, self.lattice_c),
            consonance: self.consonance,
            voice_id: self.voice_id,
        }
    }
}

/// Client ↔ Daemon protocol messages (JSON over WebSocket or Unix socket).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MuxMessage {
    // Client -> Daemon
    #[serde(rename = "hello")]
    Hello,
    #[serde(rename = "input")]
    Input { data: String }, // base64 encoded
    #[serde(rename = "consonance_subscribe")]
    ConsonanceSubscribe { min_score: Option<f32> },

    // Daemon -> Client
    #[serde(rename = "hello_ack")]
    HelloAck {
        alias: String,
        device: Option<String>,
        baud: u32,
        transport: String,
    },
    #[serde(rename = "output")]
    Output { data: String }, // base64 encoded
    #[serde(rename = "consonance_event")]
    ConsonanceEvent {
        timestamp_ns: u64,
        frequency: f64,
        lattice_a: i8,
        lattice_b: i8,
        lattice_c: i8,
        consonance: f32,
        voice_id: u8,
    },
    #[serde(rename = "heatmap_update")]
    HeatmapUpdate { data: Vec<Vec<f32>> },
    #[serde(rename = "history")]
    History { lines: Vec<String> },
    #[serde(rename = "error")]
    Error { message: String },
}

impl MuxMessage {
    /// Encode to JSON bytes with 4-byte BE length prefix.
    pub fn encode_framed(&self) -> Vec<u8> {
        let payload = serde_json::to_vec(self).expect("json serialize");
        let len = payload.len() as u32;
        let mut buf = len.to_be_bytes().to_vec();
        buf.extend_from_slice(&payload);
        buf
    }

    /// Decode JSON from bytes.
    pub fn decode(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_consonance_message_roundtrip() {
        let msg = ConsonanceMessage::new(
            1234567890,
            440.0,
            LatticePoint::new(0, 0, 0),
            1.0,
            1,
        );
        let encoded = msg.encode();
        let decoded = ConsonanceMessage::decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_consonance_message_framed() {
        let msg = ConsonanceMessage::new(
            999,
            261.63,
            LatticePoint::new(-2, 0, 1),
            0.85,
            2,
        );
        let framed = msg.encode_framed();
        // First 4 bytes are length
        let len = u32::from_be_bytes([framed[0], framed[1], framed[2], framed[3]]) as usize;
        assert_eq!(len, framed.len() - 4);
        let decoded = ConsonanceMessage::decode(&framed[4..]).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_mux_message_json_roundtrip() {
        let msgs = vec![
            MuxMessage::Hello,
            MuxMessage::Input { data: base64::engine::general_purpose::STANDARD.encode(b"hello") },
            MuxMessage::HelloAck {
                alias: "synth".to_string(),
                device: Some("/dev/ttyUSB0".to_string()),
                baud: 115200,
                transport: "serial".to_string(),
            },
            MuxMessage::Output { data: "AQID".to_string() },
            MuxMessage::Error { message: "test".to_string() },
        ];
        for msg in msgs {
            let json = serde_json::to_vec(&msg).unwrap();
            let decoded: MuxMessage = serde_json::from_slice(&json).unwrap();
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn test_mux_message_framed() {
        let msg = MuxMessage::ConsonanceSubscribe { min_score: Some(0.5) };
        let framed = msg.encode_framed();
        let len = u32::from_be_bytes([framed[0], framed[1], framed[2], framed[3]]) as usize;
        let decoded = MuxMessage::decode(&framed[4..]).unwrap();
        assert_eq!(msg, decoded);
    }
}
