use crate::events::{CdpEvent, CdpRequest, CdpResponse};
use futures_util::{SinkExt, StreamExt};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;

pub const NONCE_HEADER: &str = "x-kage-session-nonce";

#[derive(thiserror::Error, Debug)]
pub enum BrokerError {
    #[error("SECURITY VIOLATION: CDP Broker must strictly bind to 127.0.0.1 loopback interface. Attempted bind to {0}")]
    NonLoopbackBindingForbidden(IpAddr),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),
}

pub struct CdpBroker {
    bind_addr: SocketAddr,
    session_nonce: String,
    event_tx: broadcast::Sender<CdpEvent>,
    shutdown: Arc<AtomicBool>,
}

impl CdpBroker {
    /// Constructs and starts an authenticated CDP loopback broker on an ephemeral port
    pub async fn bind_ephemeral(session_nonce: &str) -> Result<(Self, SocketAddr), BrokerError> {
        let loopback_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        Self::bind_with_addr(loopback_addr, session_nonce).await
    }

    /// Explicitly binds with an IP address, enforcing loopback-only safety check
    pub async fn bind_with_addr(addr: SocketAddr, session_nonce: &str) -> Result<(Self, SocketAddr), BrokerError> {
        if !addr.ip().is_loopback() {
            return Err(BrokerError::NonLoopbackBindingForbidden(addr.ip()));
        }

        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        let (event_tx, _) = broadcast::channel(128);
        let shutdown = Arc::new(AtomicBool::new(false));

        let broker = Self {
            bind_addr: local_addr,
            session_nonce: session_nonce.to_string(),
            event_tx: event_tx.clone(),
            shutdown: shutdown.clone(),
        };

        let active_nonce = session_nonce.to_string();
        let event_tx_clone = event_tx.clone();
        let shutdown_clone = shutdown.clone();

        // Spawn background connection listener
        tokio::spawn(async move {
            while !shutdown_clone.load(Ordering::Relaxed) {
                if let Ok((stream, peer_addr)) = listener.accept().await {
                    let nonce = active_nonce.clone();
                    let tx = event_tx_clone.clone();

                    tokio::spawn(async move {
                        Self::handle_connection(stream, peer_addr, nonce, tx).await;
                    });
                }
            }
        });

        Ok((broker, local_addr))
    }

    pub fn event_sender(&self) -> broadcast::Sender<CdpEvent> {
        self.event_tx.clone()
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn session_nonce(&self) -> &str {
        &self.session_nonce
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    async fn handle_connection(
        stream: TcpStream,
        _peer_addr: SocketAddr,
        expected_nonce: String,
        event_tx: broadcast::Sender<CdpEvent>,
    ) {
        // Enforce WebSocket upgrade with Session Nonce validation
        let mut auth_error: Option<StatusCode> = None;
        let expected_nonce_clone = expected_nonce.clone();

        let ws_stream_res = tokio_tungstenite::accept_hdr_async(
            stream,
            |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
                let nonce_header = req.headers().get(NONCE_HEADER);
                match nonce_header {
                    None => {
                        auth_error = Some(StatusCode::UNAUTHORIZED);
                        let mut resp = ErrorResponse::new(Some("401 Unauthorized: Missing X-KAGE-Session-Nonce".to_string()));
                        *resp.status_mut() = StatusCode::UNAUTHORIZED;
                        Err(resp)
                    }
                    Some(val) => match val.to_str() {
                        Ok(v) if v == expected_nonce_clone => Ok(res),
                        _ => {
                            auth_error = Some(StatusCode::FORBIDDEN);
                            let mut resp = ErrorResponse::new(Some("403 Forbidden: Invalid Session Nonce".to_string()));
                            *resp.status_mut() = StatusCode::FORBIDDEN;
                            Err(resp)
                        }
                    },
                }
            },
        )
        .await;

        let mut ws_stream = match ws_stream_res {
            Ok(s) => s,
            Err(_) => return, // Rejected unauthenticated handshake
        };

        let mut event_rx = event_tx.subscribe();

        // Loop multiplexing commands and broadcast events
        loop {
            tokio::select! {
                // Incoming commands from client
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(req) = serde_json::from_str::<CdpRequest>(&text) {
                                // Echo success response for domain enable calls
                                let resp = CdpResponse {
                                    id: req.id,
                                    result: Some(serde_json::json!({ "enabled": true })),
                                    error: None,
                                };
                                let resp_text = serde_json::to_string(&resp).unwrap();
                                if ws_stream.send(Message::Text(resp_text)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        _ => {}
                    }
                }
                // Outgoing broadcast events to client
                evt = event_rx.recv() => {
                    if let Ok(event) = evt {
                        let event_text = serde_json::to_string(&event).unwrap();
                        if ws_stream.send(Message::Text(event_text)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}
