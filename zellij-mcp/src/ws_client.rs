//! A minimal client for the Zellij remote-control WebSocket API
//! (`zellij-api`; see `REMOTE_API.md` at the repository root).
//!
//! Opens one connection per call rather than pooling. This is a low-volume,
//! loopback control plane and tool calls from an MCP client can be minutes
//! apart — a fresh connection per call means no reconnect logic, no stale
//! sockets, and nothing that can drift out of sync between calls.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

const CALL_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ApiClient {
    url: String,
}

impl ApiClient {
    pub fn new(ws_url: &str, token: &str) -> Self {
        let separator = if ws_url.contains('?') { "&" } else { "?" };
        Self {
            url: format!("{ws_url}{separator}token={token}"),
        }
    }

    /// Send one command (a JSON object without `id` — one is added here) and
    /// return its `result` on success, or the server's error message.
    pub async fn call(&self, mut command: Value) -> Result<Value, String> {
        let id = uuid::Uuid::new_v4().to_string();
        command["id"] = json!(id);

        let (mut socket, _) =
            tokio::time::timeout(CALL_TIMEOUT, tokio_tungstenite::connect_async(&self.url))
                .await
                .map_err(|_| "timed out connecting to the zellij remote API".to_string())?
                .map_err(|e| format!("could not connect to the zellij remote API: {e}"))?;

        socket
            .send(Message::Text(command.to_string().into()))
            .await
            .map_err(|e| format!("could not send command: {e}"))?;

        let deadline = tokio::time::Instant::now() + CALL_TIMEOUT;
        let result = loop {
            let frame = match tokio::time::timeout_at(deadline, socket.next()).await {
                Ok(Some(Ok(frame))) => frame,
                Ok(Some(Err(e))) => break Err(format!("websocket error: {e}")),
                Ok(None) => break Err("the zellij remote API closed the connection".to_string()),
                Err(_) => break Err(format!("timed out waiting for a reply to {command}")),
            };
            let Message::Text(text) = frame else { continue };
            let value: Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(e) => break Err(format!("the zellij remote API sent invalid JSON: {e}")),
            };
            // A fresh, single-purpose connection should only ever see the one
            // reply it is waiting for — an `event` frame here would mean the
            // server pushed something unsolicited, which is harmless to skip
            // rather than treat as fatal.
            if value.get("event").is_some() {
                continue;
            }
            if value["id"] == json!(id) {
                break if value["ok"] == json!(true) {
                    Ok(value["result"].clone())
                } else {
                    Err(value["error"]
                        .as_str()
                        .unwrap_or("unknown error")
                        .to_string())
                };
            }
        };

        let _ = socket.close(None).await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_appended_as_a_query_parameter() {
        let client = ApiClient::new("ws://127.0.0.1:8787/api", "s3cret");
        assert_eq!(client.url, "ws://127.0.0.1:8787/api?token=s3cret");
    }

    #[test]
    fn an_existing_query_string_is_extended_not_overwritten() {
        let client = ApiClient::new("ws://127.0.0.1:8787/api?debug=1", "s3cret");
        assert_eq!(client.url, "ws://127.0.0.1:8787/api?debug=1&token=s3cret");
    }
}
