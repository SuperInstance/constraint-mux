# constraint-mux

Async serial port multiplexer with real-time consonance analysis — fan out serial data to multiple clients with live harmonic lattice scoring and a WebSocket dashboard.

## What This Gives You

<<<<<<< HEAD
## What This Gives You

- **Serial → WebSocket fan-out** — One serial port, unlimited WebSocket clients
- **Live consonance scoring** — 3-limit harmonic lattice (2^a × 3^b × 5^c) pairwise consonance heatmap
- **HTTP dashboard** — Real-time visualization at `localhost:3000`
- **Demo mode** — Test without hardware using simulated musical data
- **Zero-config startup** — Defaults work out of the box

## Install
=======
- **Serial multiplexing** — one serial port, N subscribers (tokio broadcast channels)
- **Real-time consonance scoring** — every data point rated on the Eisenstein harmonic lattice
- **Binary wire protocol** — bincode-encoded `ConsonanceMessage` with length-prefix framing
- **WebSocket dashboard** — live heatmap and frequency display in the browser
- **59 tests** — comprehensive coverage of protocol, multiplexer, and consonance math

## Quick Start

```bash
# Start multiplexer on a serial port
cargo run -- --device /dev/ttyUSB0 --baud 115200 --alias synth-1

# Connect WebSocket clients
# ws://localhost:8080/ws/consonance  — live consonance events
# ws://localhost:8080/ws/raw         — raw serial output
```

### As a Library

```rust
use constraint_mux::{Multiplexer, ConsonanceMessage};

let mux = Multiplexer::new("synth-1".into(), Some("/dev/ttyUSB0".into()), 115200);

// Subscribe to consonance events
let mut rx = mux.subscribe_consonance();
while let Ok(msg) = rx.recv().await {
    println!("Voice {} @ {:.1} Hz, consonance={:.3}, lattice=2^{}×3^{}×5^{}",
             msg.voice_id, msg.frequency, msg.consonance,
             msg.lattice_a, msg.lattice_b, msg.lattice_c);
}
```

## Wire Protocol

Each `ConsonanceMessage` is bincode-serialized with a 4-byte big-endian length prefix:

```
[u32 length][ConsonanceMessage]
  ├── timestamp_ns: u64
  ├── frequency: f64
  ├── lattice_a: i8   (2^a)
  ├── lattice_b: i8   (3^b)
  ├── lattice_c: i8   (5^c)
  ├── consonance: f32
  └── voice_id: u8
```

## API Reference

| Type | Description |
|---|---|
| `Multiplexer` | Async serial fan-out with consonance analysis |
| `ConsonanceMessage` | Wire-level protocol message |
| `ConsonanceHeatmap` | Frequency × time consonance grid |
| `LatticePoint` | Eisenstein lattice coordinate (a, b, c) |

## How It Fits

The **hardware bridge** in the constraint theory ecosystem:

- [constraint-audio](https://github.com/SuperInstance/constraint-audio) — consonance scoring engine used here
- [constraint-synth](https://github.com/SuperInstance/constraint-synth) — synthesizer that could drive the serial data
- [conservation-protocol](https://github.com/SuperInstance/conservation-protocol) — Laplacian messaging for conservation tracking

## Testing

```bash
cargo test  # 59 tests
```

## Installation
>>>>>>> 9de6867 (docs: world-class README audit and rewrite)

```bash
cargo install constraint-mux
```

<<<<<<< HEAD
## Usage

### Connect to a serial port

```bash
constraint-mux start /dev/ttyUSB0 --baud 115200 --alias synth-1
```

This starts:
- A WebSocket server on `127.0.0.1:8080` for real-time data streaming
- A dashboard on `127.0.0.1:3000` with consonance heatmap visualization

### Demo mode (no hardware needed)

```bash
constraint-mux demo
```

Simulates musical data through the consonance pipeline so you can see the heatmap and WebSocket output without a serial device.

### Multiple clients

Every client that connects to `ws://127.0.0.1:8080` gets the same data stream via broadcast. The consonance heatmap reflects all connected inputs.

## How Consonance Works

Frequencies are decomposed into points on the 3-limit harmonic lattice (2^a × 3^b × 5^c):

```rust
use constraint_mux::consonance::LatticePoint;

let fifth = LatticePoint::new(-1, 1, 0);  // 3/2
let third = LatticePoint::new(-2, 0, 1);  // 5/4

// Distance on the lattice — lower = more consonant
let d = fifth.distance(&third);
```

Common just intervals are looked up from a pre-computed table (unison through octave). Pairwise consonance between frequencies fills a heatmap that updates in real time.

## Architecture

```
Serial Port → Multiplexer → broadcast::channel
                              ├→ WebSocket clients (raw data)
                              ├→ Consonance engine (heatmap)
                              └→ Dashboard (HTTP + WS)
```

- **multiplexer.rs** — Async serial read, broadcast fan-out, client tracking
- **consonance.rs** — Lattice decomposition, pairwise scoring, ConsonanceHeatmap
- **protocol.rs** — Message types for consonance events
- **websocket.rs** — WebSocket server for client connections
- **dashboard.rs** — HTTP dashboard with live heatmap

## Configuration

```bash
constraint-mux start DEVICE [OPTIONS]

Options:
  --baud <RATE>           Baud rate (default: 115200)
  --alias <NAME>          Device alias for display
  --ws-addr <ADDR>        WebSocket listen address (default: 127.0.0.1:8080)
  --dashboard-addr <ADDR> Dashboard address (default: 127.0.0.1:3000)
  --foreground            Run in foreground (don't daemonize)
```

## Testing

Unit tests for consonance engine and lattice point arithmetic:

```bash
cargo test
```

## Dependencies

- tokio (async runtime)
- tokio-serial (serial port I/O)
- axum (HTTP + WebSocket server)
- clap (CLI)
- bincode (message serialization)

## Related

- [constraint-audio](https://github.com/SuperInstance/constraint-audio) — Rust audio DSP with lattice oscillators
- [constraint-theory-core](https://github.com/SuperInstance/constraint-theory-core) — The constraint math this is built on
- [flux-tensor-midi](https://github.com/SuperInstance/flux-tensor-midi) — 4D MIDI tensor representation

=======
>>>>>>> 9de6867 (docs: world-class README audit and rewrite)
## License

MIT

## Documentation

📚 [OpenConstruct Docs](https://github.com/SuperInstance/openconstruct-docs)
