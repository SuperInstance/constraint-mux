//! WebSocket bridge — exposes serial data and consonance events via WebSocket.

use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use base64::Engine;

use crate::multiplexer::Multiplexer;
use crate::protocol::MuxMessage;

/// Handle a single WebSocket client connection.
pub async fn handle_ws_client(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mux: Arc<Multiplexer>,
    alias: String,
) {
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send hello
    let hello = MuxMessage::HelloAck {
        alias: alias.clone(),
        device: mux.device.clone(),
        baud: mux.baud,
        transport: "serial".to_string(),
    };
    let hello_json = serde_json::to_string(&hello).unwrap();
    let _ = ws_sender.send(Message::Text(hello_json)).await;

    // Send history
    let history = mux.get_history();
    let hist_msg = MuxMessage::History { lines: history };
    let hist_json = serde_json::to_string(&hist_msg).unwrap();
    let _ = ws_sender.send(Message::Text(hist_json)).await;

    mux.add_client();

    // Subscribe to data streams
    let mut output_rx = mux.subscribe_output();
    let mut consonance_rx = mux.subscribe_consonance();

    // Main loop: forward data to WebSocket client
    loop {
        tokio::select! {
            result = output_rx.recv() => {
                match result {
                    Ok(data) => {
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                        let msg = MuxMessage::Output { data: b64 };
                        let json = serde_json::to_string(&msg).unwrap();
                        if ws_sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WS client lagged {} messages", n);
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
                        if ws_sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WS consonance client lagged {} messages", n);
                    }
                    Err(_) => break,
                }
            }

            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(MuxMessage::Input { data: _ }) = serde_json::from_str(&text) {
                            tracing::debug!("WS client sent data");
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    mux.remove_client();
    tracing::info!("WebSocket client disconnected from {}", alias);
}

/// Start the WebSocket server.
pub async fn serve(mux: Arc<Multiplexer>, addr: std::net::SocketAddr, alias: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("WebSocket server listening on ws://{}", addr);

    while let Ok((stream, peer)) = listener.accept().await {
        let mux = mux.clone();
        let alias = alias.clone();
        tracing::info!("WebSocket connection from {}", peer);

        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    handle_ws_client(ws_stream, mux, alias).await;
                }
                Err(e) => {
                    tracing::error!("WebSocket handshake failed: {}", e);
                }
            }
        });
    }

    Ok(())
}
