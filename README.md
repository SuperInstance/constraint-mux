# constraint-mux

Serial port multiplexer with real-time consonance analysis. Fan out serial data to multiple WebSocket clients while scoring harmonic consonance on the fly.

Built for collaborative constraint-aware instruments — multiple musicians on separate devices can connect to the same serial port and see live consonance heatmaps of the combined output.

## Install

```bash
cargo install constraint-mux
```

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

## License

MIT OR Apache-2.0
