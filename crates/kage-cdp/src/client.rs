//! Low-level CDP method call client with monotonic sequencing and async request/response demuxing.
//!
//! # Zero Stub Mandate
//!
//! Connects over live WebSocket (with ephemeral session nonce), maintains atomic monotonic
//! message IDs, routes responses to matching `oneshot` caller channels, and broadcasts
//! asynchronous domain events to multiple consumers.

use crate::broker::{CdpError, NONCE_HEADER};
use crate::events::{CdpEvent, CdpRequest, CdpResponse};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<CdpResponse, CdpError>>>>>;

/// Typed CDP method call client backed by an authenticated WebSocket connection.
pub struct CdpClient {
    next_id: AtomicU64,
    cmd_tx: mpsc::Sender<(CdpRequest, oneshot::Sender<Result<CdpResponse, CdpError>>)>,
    event_tx: broadcast::Sender<CdpEvent>,
}

impl std::fmt::Debug for CdpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpClient")
            .field("next_id", &self.next_id.load(Ordering::Relaxed))
            .finish()
    }
}

impl CdpClient {
    /// Connects to a CDP WebSocket endpoint, presenting the optional session nonce in HTTP headers.
    pub async fn connect(url: &str, nonce: Option<&str>) -> Result<Self, CdpError> {
        let mut request = url
            .into_client_request()
            .map_err(|e| CdpError::ConnectionFailed(e.to_string()))?;

        if let Some(token) = nonce {
            request.headers_mut().insert(
                NONCE_HEADER,
                token
                    .parse()
                    .map_err(|e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| {
                        CdpError::ConnectionFailed(e.to_string())
                    })?,
            );
        }

        let (ws_stream, _response) = connect_async(request).await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") {
                CdpError::AuthFailed("401 Unauthorized: Missing X-KAGE-Session-Nonce".into())
            } else if err_str.contains("403") {
                CdpError::AuthFailed("403 Forbidden: Invalid Session Nonce".into())
            } else {
                CdpError::ConnectionFailed(err_str)
            }
        })?;

        let (mut write, mut read) = ws_stream.split();
        let (cmd_tx, mut cmd_rx) =
            mpsc::channel::<(CdpRequest, oneshot::Sender<Result<CdpResponse, CdpError>>)>(64);
        let (event_tx, _) = broadcast::channel::<CdpEvent>(256);

        let pending_requests: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = pending_requests.clone();
        let event_tx_clone = event_tx.clone();

        // Background reader task: demultiplex responses vs incoming domain events
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Check if it is a command response
                        if let Ok(resp) = serde_json::from_str::<CdpResponse>(&text) {
                            if resp.id > 0 {
                                let mut map = pending_reader.lock().await;
                                if let Some(sender) = map.remove(&resp.id) {
                                    let _ = sender.send(Ok(resp));
                                    continue;
                                }
                            }
                        }

                        // Otherwise check if it is a typed CDP domain event
                        if let Ok(evt) = serde_json::from_str::<CdpEvent>(&text) {
                            let _ = event_tx_clone.send(evt);
                        }
                    }
                    _ => break,
                }
            }

            // Drain pending callers on connection termination
            let mut map = pending_reader.lock().await;
            for (_, sender) in map.drain() {
                let _ = sender.send(Err(CdpError::ConnectionClosed));
            }
        });

        // Background writer task
        let pending_writer = pending_requests.clone();
        tokio::spawn(async move {
            while let Some((req, resp_tx)) = cmd_rx.recv().await {
                let id = req.id;
                pending_writer.lock().await.insert(id, resp_tx);

                if let Ok(req_text) = serde_json::to_string(&req) {
                    if write.send(Message::Text(req_text)).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            cmd_tx,
            event_tx,
        })
    }

    /// Subscribe to typed asynchronous CDP domain events broadcast over the WebSocket.
    pub fn subscribe_events(&self) -> broadcast::Receiver<CdpEvent> {
        self.event_tx.subscribe()
    }

    /// Send a CDP command and await the correlated response value.
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CdpError> {
        self.call_session(None, method, params).await
    }

    /// Send a CDP command scoped to a specific protocol session (e.g. attached target)
    /// and await the correlated response value.
    pub async fn call_session(
        &self,
        session_id: Option<&str>,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
            session_id: session_id.map(ToString::to_string),
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx
            .send((request, resp_tx))
            .await
            .map_err(|_| CdpError::ConnectionClosed)?;

        let resp = resp_rx.await.map_err(|_| CdpError::ConnectionClosed)??;
        if let Some(err) = resp.error {
            return Err(CdpError::ProtocolError {
                method: method.to_string(),
                code: err.code,
                message: err.message,
            });
        }

        Ok(resp.result.unwrap_or(serde_json::Value::Null))
    }
}
