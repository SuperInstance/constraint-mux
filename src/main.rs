//! constraint-mux — Serial port multiplexer with real-time consonance analysis.

mod consonance;
mod dashboard;
mod multiplexer;
mod protocol;
mod websocket;

use std::net::SocketAddr;
use std::sync::Arc;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "constraint-mux", version, about = "Serial multiplexer with real-time consonance analysis")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a daemon for a serial port
    Start {
        /// Serial device path (e.g., /dev/ttyUSB0)
        device: Option<String>,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
        /// Alias name
        #[arg(short, long)]
        alias: Option<String>,
        /// Run in foreground
        #[arg(short, long)]
        foreground: bool,
        /// WebSocket listen address
        #[arg(long, default_value = "127.0.0.1:8080")]
        ws_addr: String,
        /// Dashboard listen address
        #[arg(long, default_value = "127.0.0.1:3000")]
        dashboard_addr: String,
    },
    /// Demo mode: simulate musical data without hardware
    Demo {
        /// Alias name
        #[arg(short, long, default_value = "demo")]
        alias: String,
        /// WebSocket listen address
        #[arg(long, default_value = "127.0.0.1:8080")]
        ws_addr: String,
        /// Dashboard listen address
        #[arg(long, default_value = "127.0.0.1:3000")]
        dashboard_addr: String,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "constraint_mux=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            device,
            baud,
            alias,
            foreground: _,
            ws_addr,
            dashboard_addr,
        } => {
            let alias = alias.or_else(|| {
                device.as_ref().map(|d| {
                    std::path::Path::new(d)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "serial".to_string())
                })
            }).unwrap_or_else(|| "serial".to_string());

            run_daemon(device, baud, alias, ws_addr, dashboard_addr, false).await;
        }
        Commands::Demo {
            alias,
            ws_addr,
            dashboard_addr,
        } => {
            run_daemon(None, 115200, alias, ws_addr, dashboard_addr, true).await;
        }
    }
}

async fn run_daemon(
    device: Option<String>,
    baud: u32,
    alias: String,
    ws_addr: String,
    dashboard_addr: String,
    demo_mode: bool,
) {
    let mux = Arc::new(multiplexer::Multiplexer::new(
        alias.clone(),
        device.clone(),
        baud,
    ));

    tracing::info!("constraint-mux starting: alias={}, device={:?}, baud={}", alias, device, baud);

    // Start serial reader if device specified
    if let Some(ref dev) = device {
        let mux_clone = mux.clone();
        let dev = dev.clone();
        tokio::spawn(async move {
            if let Err(e) = multiplexer::serial_reader(mux_clone, dev, baud).await {
                tracing::error!("Serial reader error: {}", e);
            }
        });
    }

    // Start demo mode data generator
    if demo_mode {
        let mux_clone = mux.clone();
        tokio::spawn(async move {
            demo_data_generator(mux_clone).await;
        });
    }

    // Start WebSocket server
    let ws_addr_parsed: SocketAddr = ws_addr.parse().expect("Invalid WS address");
    let mux_ws = mux.clone();
    let alias_ws = alias.clone();
    let ws_handle = tokio::spawn(async move {
        if let Err(e) = websocket::serve(mux_ws, ws_addr_parsed, alias_ws).await {
            tracing::error!("WebSocket server error: {}", e);
        }
    });

    // Start dashboard HTTP server
    let dash_addr_parsed: SocketAddr = dashboard_addr.parse().expect("Invalid dashboard address");
    let mux_dash = mux.clone();
    let alias_dash = alias.clone();
    let dash_handle = tokio::spawn(async move {
        let app = dashboard::build_router(mux_dash, alias_dash);
        let listener = match tokio::net::TcpListener::bind(dash_addr_parsed).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Dashboard bind failed: {}", e);
                return;
            }
        };
        tracing::info!("Dashboard listening on http://{}", dash_addr_parsed);
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("Dashboard server error: {}", e);
        }
    });

    tracing::info!("♪ constraint-mux ready: {} — ws://{} | http://{}", alias, ws_addr, dashboard_addr);

    // Periodically log heatmap state
    let mux_hm = mux.clone();
    let heatmap_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let hm = mux_hm.heatmap.lock().unwrap();
            let total: f32 = hm.heatmap.iter().flat_map(|r| r.iter()).sum();
            tracing::debug!("Heatmap total energy: {:.2}", total);
        }
    });

    // Wait for servers
    tokio::select! {
        _ = ws_handle => tracing::warn!("WebSocket server exited"),
        _ = dash_handle => tracing::warn!("Dashboard server exited"),
        _ = heatmap_handle => tracing::warn!("Heatmap updater exited"),
    }
}

/// Generate demo musical data for testing without hardware.
async fn demo_data_generator(mux: Arc<multiplexer::Multiplexer>) {
    use consonance::{frequency_to_note, ratio_to_lattice, consonance_score};

    tracing::info!("Demo mode: generating simulated musical data");

    let notes = [
        261.63, // C4
        293.66, // D4
        329.63, // E4
        349.23, // F4
        392.00, // G4
        440.00, // A4
        493.88, // B4
        523.25, // C5
    ];

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
    let mut note_idx = 0u64;
    let mut last_freq = 0.0f64;

    loop {
        interval.tick().await;

        let base_freq = notes[(note_idx % notes.len() as u64) as usize];
        let freq = base_freq * (1.0 + (note_idx as f64 * 0.01).sin() * 0.02);

        let lattice = ratio_to_lattice(freq / 440.0);
        let consonance = if last_freq > 0.0 {
            consonance_score(freq, last_freq)
        } else {
            1.0
        };

        let data = format!("F:{:.2}\n", freq);
        mux.process_serial_data(data.as_bytes());

        let (note_name, octave) = frequency_to_note(freq);
        tracing::debug!(
            "Demo: {} {}{:.0} ({:.1} Hz) lattice=({},{},{}) consonance={:.2}",
            if note_idx % 8 == 0 { "♪" } else { "♩" },
            note_name, octave, freq,
            lattice.a, lattice.b, lattice.c,
            consonance
        );

        last_freq = freq;
        note_idx += 1;

        // Occasionally play a chord
        if note_idx % 7 == 0 {
            let chord_freq = base_freq * 1.5;
            let data2 = format!("F:{:.2}\n", chord_freq);
            mux.process_serial_data(data2.as_bytes());
        }
    }
}
