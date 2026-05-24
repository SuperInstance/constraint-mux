//! Dashboard — serves a browser-based HTML dashboard for live consonance visualization.

use std::sync::Arc;
use axum::{
    extract::State,
    response::Html,
    routing::get,
    Router,
    extract::ws::{WebSocket, WebSocketUpgrade, Message},
};
use futures_util::{SinkExt, StreamExt};
use base64::Engine;

use crate::multiplexer::Multiplexer;
use crate::protocol::MuxMessage;

/// The dashboard HTML — a single-page app with live WebSocket consonance visualization.
fn dashboard_html(alias: &str) -> Html<String> {
    Html(format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>constraint-mux: {alias}</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: 'SF Mono', 'Fira Code', monospace; background: #0a0a0f; color: #e0e0e0; }}
  .header {{ padding: 16px 24px; border-bottom: 1px solid #1a1a2e; }}
  .header h1 {{ font-size: 18px; color: #00ff88; }}
  .header .meta {{ font-size: 12px; color: #666; margin-top: 4px; }}
  .container {{ display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 16px 24px; }}
  .panel {{ background: #111118; border: 1px solid #1a1a2e; border-radius: 8px; padding: 16px; }}
  .panel h2 {{ font-size: 14px; color: #888; margin-bottom: 12px; text-transform: uppercase; letter-spacing: 1px; }}
  .freq-display {{ font-size: 48px; color: #00ff88; text-align: center; padding: 20px; }}
  .freq-display .note {{ font-size: 24px; color: #666; }}
  .freq-display .lattice {{ font-size: 14px; color: #444; margin-top: 8px; }}
  .consonance-bar {{ height: 8px; border-radius: 4px; background: #1a1a2e; margin: 8px 0; overflow: hidden; }}
  .consonance-bar .fill {{ height: 100%; border-radius: 4px; transition: width 0.1s, background 0.3s; }}
  .serial-log {{ height: 300px; overflow-y: auto; font-size: 12px; line-height: 1.4; }}
  .serial-log .line {{ padding: 2px 0; border-bottom: 1px solid #0a0a0f; }}
  .serial-log .ts {{ color: #444; }}
  #heatmap {{ width: 100%; aspect-ratio: 1; }}
  .events {{ max-height: 200px; overflow-y: auto; }}
  .events table {{ width: 100%; border-collapse: collapse; font-size: 11px; }}
  .events th {{ text-align: left; color: #555; padding: 4px 8px; border-bottom: 1px solid #1a1a2e; }}
  .events td {{ padding: 3px 8px; }}
  .status {{ display: flex; gap: 24px; padding: 12px 0; }}
  .status .item {{ text-align: center; }}
  .status .item .value {{ font-size: 20px; color: #00ff88; }}
  .status .item .label {{ font-size: 10px; color: #555; text-transform: uppercase; }}
  .voice {{ display: inline-block; padding: 2px 6px; border-radius: 3px; font-size: 10px; margin: 1px; }}
  @keyframes pulse {{ 0%,100% {{ opacity: 1; }} 50% {{ opacity: 0.5; }} }}
  .live {{ animation: pulse 1s infinite; color: #00ff88; }}
</style>
</head>
<body>
<div class="header">
  <h1>♪ constraint-mux: {alias}</h1>
  <div class="meta">
    <span id="transport">connecting...</span> ·
    <span id="client-count">0 clients</span> ·
    <span id="event-count">0 events</span>
  </div>
</div>

<div class="status" id="status-bar">
  <div class="item"><div class="value" id="freq-val">--</div><div class="label">Frequency</div></div>
  <div class="item"><div class="value" id="note-val">--</div><div class="label">Note</div></div>
  <div class="item"><div class="value" id="consonance-val">--</div><div class="label">Consonance</div></div>
  <div class="item"><div class="value" id="lattice-val">--</div><div class="label">Lattice</div></div>
  <div class="item"><div class="value" id="voices-val">--</div><div class="label">Voices</div></div>
</div>

<div class="consonance-bar"><div class="fill" id="consonance-fill" style="width:0%;background:#00ff88"></div></div>

<div class="container">
  <div class="panel">
    <h2>Serial Output</h2>
    <div class="serial-log" id="serial-log"></div>
  </div>
  <div class="panel">
    <h2>Consonance Heatmap</h2>
    <canvas id="heatmap"></canvas>
  </div>
  <div class="panel" style="grid-column: span 2;">
    <h2>Recent Events</h2>
    <div class="events" id="events">
      <table>
        <thead><tr><th>Time</th><th>Freq</th><th>Note</th><th>Lattice</th><th>Consonance</th><th>Voice</th></tr></thead>
        <tbody id="events-body"></tbody>
      </table>
    </div>
  </div>
</div>

<script>
const ws = new WebSocket((location.protocol === 'https:' ? 'wss:' : 'ws:') + '//' + location.host + '/ws');
const noteNames = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];
let eventCount = 0;
const voices = new Map();
const heatmap = new Float32Array({HM_SIZE}*{HM_SIZE});
const HM_SIZE = {HM_SIZE};

function freqToNote(f) {{
  if (f <= 0) return '--';
  const semi = 12 * Math.log2(f / 440);
  const midi = Math.round(semi + 69);
  const idx = ((midi % 12) + 12) % 12;
  const oct = Math.floor(midi / 12) - 1;
  return noteNames[idx] + oct;
}}

function latticeStr(a, b, c) {{
  let parts = [];
  if (a) parts.push('2^' + a);
  if (b) parts.push('3^' + b);
  if (c) parts.push('5^' + c);
  return parts.join(' x ') || 'unison';
}}

function consonanceColor(score) {{
  if (score > 0.8) return '#00ff88';
  if (score > 0.5) return '#88ff00';
  if (score > 0.3) return '#ffaa00';
  return '#ff4444';
}}

function updateHeatmap(data) {{
  for (let i = 0; i < data.length && i < HM_SIZE; i++)
    for (let j = 0; j < data[i].length && j < HM_SIZE; j++)
      heatmap[i * HM_SIZE + j] = data[i][j];
  drawHeatmap();
}}

function drawHeatmap() {{
  const canvas = document.getElementById('heatmap');
  const ctx = canvas.getContext('2d');
  const w = canvas.width = canvas.offsetWidth * devicePixelRatio;
  const h = canvas.height = canvas.offsetHeight * devicePixelRatio;
  ctx.scale(devicePixelRatio, devicePixelRatio);
  const cw = canvas.offsetWidth / HM_SIZE;
  const ch = canvas.offsetHeight / HM_SIZE;
  let mx = 0;
  for (let i = 0; i < HM_SIZE * HM_SIZE; i++) if (heatmap[i] > mx) mx = heatmap[i];
  if (mx === 0) mx = 1;
  for (let i = 0; i < HM_SIZE; i++) {{
    for (let j = 0; j < HM_SIZE; j++) {{
      const v = heatmap[i * HM_SIZE + j] / mx;
      const r = Math.floor(v * 255);
      const g = Math.floor(v * 180);
      ctx.fillStyle = 'rgb(' + r + ',' + g + ',' + Math.floor(v*50) + ')';
      ctx.fillRect(j * cw, i * ch, cw - 0.5, ch - 0.5);
    }}
  }}
}}

function addEvent(evt) {{
  const tbody = document.getElementById('events-body');
  const tr = document.createElement('tr');
  const d = new Date(evt.timestamp_ns / 1e6);
  const ts = d.toLocaleTimeString();
  const note = freqToNote(evt.frequency);
  const hue = (evt.voice_id * 60) % 360;
  tr.innerHTML = '<td>' + ts + '</td><td>' + evt.frequency.toFixed(1) + ' Hz</td><td>' + note + '</td>' +
    '<td>' + latticeStr(evt.lattice_a, evt.lattice_b, evt.lattice_c) + '</td>' +
    '<td style="color:' + consonanceColor(evt.consonance) + '">' + (evt.consonance*100).toFixed(0) + '%</td>' +
    '<td><span class="voice" style="background:hsl(' + hue + ',70%,30%)">V' + evt.voice_id + '</span></td>';
  tbody.insertBefore(tr, tbody.firstChild);
  while (tbody.children.length > 100) tbody.removeChild(tbody.lastChild);
  eventCount++;
  document.getElementById('event-count').textContent = eventCount + ' events';
}}

ws.onmessage = function(e) {{
  const msg = JSON.parse(e.data);
  if (msg.type === 'hello_ack') {{
    document.getElementById('transport').textContent = msg.transport + ' ●';
    document.getElementById('transport').classList.add('live');
  }} else if (msg.type === 'output') {{
    const text = atob(msg.data);
    const log = document.getElementById('serial-log');
    text.split('\\n').forEach(function(line) {{
      if (line) {{
        const div = document.createElement('div');
        div.className = 'line';
        const ts = document.createElement('span');
        ts.className = 'ts';
        ts.textContent = new Date().toLocaleTimeString() + ' ';
        div.appendChild(ts);
        div.appendChild(document.createTextNode(line));
        log.appendChild(div);
      }}
    }});
    while (log.children.length > 500) log.removeChild(log.firstChild);
    log.scrollTop = log.scrollHeight;
  }} else if (msg.type === 'consonance_event') {{
    document.getElementById('freq-val').textContent = msg.frequency.toFixed(1) + ' Hz';
    document.getElementById('note-val').textContent = freqToNote(msg.frequency);
    document.getElementById('consonance-val').textContent = (msg.consonance * 100).toFixed(0) + '%';
    document.getElementById('consonance-val').style.color = consonanceColor(msg.consonance);
    document.getElementById('lattice-val').textContent = latticeStr(msg.lattice_a, msg.lattice_b, msg.lattice_c);
    document.getElementById('consonance-fill').style.width = (msg.consonance * 100) + '%';
    document.getElementById('consonance-fill').style.background = consonanceColor(msg.consonance);
    voices.set(msg.voice_id, msg.frequency);
    document.getElementById('voices-val').textContent = voices.size;
    addEvent(msg);
  }} else if (msg.type === 'heatmap_update') {{
    updateHeatmap(msg.data);
  }}
}};
ws.onclose = function() {{
  document.getElementById('transport').textContent = 'disconnected';
  document.getElementById('transport').classList.remove('live');
}};
</script>
</body>
</html>"##, alias = alias, HM_SIZE = 24))
}

/// Build the axum router for the dashboard + WebSocket.
pub fn build_router(mux: Arc<Multiplexer>, alias: String) -> Router {
    Router::new()
        .route("/", get(dashboard_index))
        .route("/ws", get(ws_handler))
        .with_state((mux, alias))
}

async fn dashboard_index(State((_, alias)): State<(Arc<Multiplexer>, String)>) -> Html<String> {
    dashboard_html(&alias)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State((mux, alias)): State<(Arc<Multiplexer>, String)>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_axum_ws(socket, mux, alias))
}

async fn handle_axum_ws(socket: WebSocket, mux: Arc<Multiplexer>, alias: String) {
    let (mut sender, mut receiver) = socket.split();

    // Send hello
    let hello = MuxMessage::HelloAck {
        alias: alias.clone(),
        device: mux.device.clone(),
        baud: mux.baud,
        transport: "serial".to_string(),
    };
    let hello_json = serde_json::to_string(&hello).unwrap();
    let _ = sender.send(Message::Text(hello_json.into())).await;

    // Send history
    let history = mux.get_history();
    let hist_msg = MuxMessage::History { lines: history };
    let hist_json = serde_json::to_string(&hist_msg).unwrap();
    let _ = sender.send(Message::Text(hist_json.into())).await;

    mux.add_client();

    let mut output_rx = mux.subscribe_output();
    let mut consonance_rx = mux.subscribe_consonance();

    loop {
        tokio::select! {
            result = output_rx.recv() => {
                match result {
                    Ok(data) => {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                        let msg = MuxMessage::Output { data: b64 };
                        let json = serde_json::to_string(&msg).unwrap();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            result = consonance_rx.recv() => {
                match result {
                    Ok(evt) => {
                        let msg = MuxMessage::ConsonanceEvent {
                            timestamp_ns: evt.timestamp_ns,
                            frequency: evt.frequency,
                            lattice_a: evt.lattice_a,
                            lattice_b: evt.lattice_b,
                            lattice_c: evt.lattice_c,
                            consonance: evt.consonance,
                            voice_id: evt.voice_id,
                        };
                        let json = serde_json::to_string(&msg).unwrap();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    mux.remove_client();
    tracing::info!("Dashboard WS client disconnected from {}", alias);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_html_contains_alias() {
        let html = dashboard_html("test-synth");
        let body = html.0;
        assert!(body.contains("test-synth"));
        assert!(body.contains("constraint-mux"));
        assert!(body.contains("heatmap"));
        assert!(body.contains("consonance"));
    }
}
