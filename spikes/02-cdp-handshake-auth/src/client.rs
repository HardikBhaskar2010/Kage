use crate::broker::NONCE_HEADER;
use crate::events::{CdpEvent, CdpRequest, CdpResponse};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Handshake connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Timeout awaiting response for command {0}")]
    Timeout(u64),

    #[error("Connection closed")]
    ConnectionClosed,
}

pub struct CdpClient {
    next_id: AtomicU64,
    cmd_tx: mpsc::Sender<(CdpRequest, oneshot::Sender<Result<CdpResponse, ClientError>>)>,
    event_tx: broadcast::Sender<CdpEvent>,
}

impl CdpClient {
    pub async fn connect(url: &str, nonce: Option<&str>) -> Result<Self, ClientError> {
        let mut request = url.into_client_request().map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;
        if let Some(token) = nonce {
            request.headers_mut().insert(
                NONCE_HEADER,
                token.parse().map_err(|e: tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue| {
                    ClientError::ConnectionFailed(e.to_string())
                })?,
            );
        }

        let (ws_stream, _response) = connect_async(request)
            .await
            .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<(CdpRequest, oneshot::Sender<Result<CdpResponse, ClientError>>)>(32);
        let (event_tx, _) = broadcast::channel(128);

        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<CdpResponse, ClientError>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending_requests.clone();
        let event_tx_clone = event_tx.clone();

        // Background reader task: demultiplex responses vs incoming events
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Check if it is a command response
                        if let Ok(resp) = serde_json::from_str::<CdpResponse>(&text) {
                            if resp.id > 0 {
                                let mut map = pending_clone.lock().await;
                                if let Some(sender) = map.remove(&resp.id) {
                                    let _ = sender.send(Ok(resp));
                                    continue;
                                }
                            }
                        }

                        // Otherwise check if it is a CDP domain event
                        if let Ok(evt) = serde_json::from_str::<CdpEvent>(&text) {
                            let _ = event_tx_clone.send(evt);
                        }
                    }
                    _ => break,
                }
            }
        });

        // Background writer task
        tokio::spawn(async move {
            while let Some((req, resp_tx)) = cmd_rx.recv().await {
                let id = req.id;
                pending_requests.lock().await.insert(id, resp_tx);

                let req_text = serde_json::to_string(&req).unwrap();
                if write.send(Message::Text(req_text)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            cmd_tx,
            event_tx,
        })
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<CdpEvent> {
        self.event_tx.subscribe()
    }

    pub async fn send_command(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx
            .send((request, resp_tx))
            .await
            .map_err(|_| ClientError::ConnectionClosed)?;

        let resp = resp_rx.await.map_err(|_| ClientError::ConnectionClosed)??;
        if let Some(err) = resp.error {
            return Err(ClientError::CommandFailed(err.to_string()));
        }

        Ok(resp.result.unwrap_or(serde_json::Value::Null))
    }
}
