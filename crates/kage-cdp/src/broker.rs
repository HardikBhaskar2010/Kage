//! [`CdpBroker`] — authenticated loopback WebSocket server and session multiplexer.
//!
//! # Responsibilities & Security Boundary (INV-07)
//!
//! * Strictly binds to `127.0.0.1:0` (ephemeral loopback) or explicit loopback addresses.
//! * Rejects non-loopback bindings (e.g. `0.0.0.0`) to enforce local-only security.
//! * Enforces HTTP handshake authentication requiring the `x-kage-session-nonce` header.
//!   - Missing nonce: HTTP 401 Unauthorized
//!   - Mismatched nonce: HTTP 403 Forbidden
//! * Multiplexes multiple named CDP sessions (`"devtools"`, `"context"`, `"toolbus"`).
//! * Broadcasts typed CDP domain events (`DOM`, `Network`, `Console`, `Target`, `Page`).

use crate::events::{CdpEvent, CdpRequest, CdpResponse, CdpResponseError};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// HTTP header containing the ephemeral authentication nonce.
pub const NONCE_HEADER: &str = "x-kage-session-nonce";

/// Opaque handle identifying a named CDP session (e.g. `"devtools"`, `"context"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new_unique() -> Self {
        SessionId(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        SessionId(s.to_string())
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        SessionId(s)
    }
}

/// Errors produced by the CDP transport and broker layer.
#[derive(Debug, Error)]
pub enum CdpError {
    #[error("SECURITY VIOLATION: CDP Broker must strictly bind to 127.0.0.1 loopback interface. Attempted bind to {0}")]
    NonLoopbackBindingForbidden(IpAddr),

    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Handshake authentication failed: {0}")]
    AuthFailed(String),

    #[error("session '{0}' not found")]
    SessionNotFound(SessionId),

    #[error("CDP method '{method}' returned error {code}: {message}")]
    ProtocolError {
        method: String,
        code: i64,
        message: String,
    },

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Connection descriptor returned to authorized internal clients (DevTools, Context Engine)
/// for connecting to the Rust-managed CDP loopback broker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdpConnectionDescriptor {
    /// Ephemeral loopback port bound by the broker.
    pub port: u16,
    /// Ephemeral session nonce required for handshake authentication.
    pub nonce: String,
    /// Full authenticated WebSocket connection URL.
    pub ws_url: String,
}

/// A registered CDP session subscriber receiving raw CDP events.
type EventSink = mpsc::UnboundedSender<serde_json::Value>;

/// Central broker managing the single authenticated CDP WebSocket listener and session registry.
pub struct CdpBroker {
    bind_addr: SocketAddr,
    session_nonce: String,
    event_tx: broadcast::Sender<CdpEvent>,
    sessions: Arc<RwLock<HashMap<SessionId, EventSink>>>,
    shutdown: Arc<AtomicBool>,
}

impl std::fmt::Debug for CdpBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpBroker")
            .field("bind_addr", &self.bind_addr)
            .field("session_nonce", &self.session_nonce)
            .finish()
    }
}

impl CdpBroker {
    /// Create a new in-memory broker with an ephemeral nonce (for tests/scaffolding).
    pub fn new() -> Self {
        Self::with_port(0)
    }

    /// Create an in-memory broker with a specific loopback port.
    pub fn with_port(port: u16) -> Self {
        let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
        let (event_tx, _) = broadcast::channel(256);
        CdpBroker {
            bind_addr: addr,
            session_nonce: nonce,
            event_tx,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Constructs and starts an authenticated CDP loopback broker on an ephemeral loopback port (`127.0.0.1:0`).
    pub async fn bind_ephemeral(session_nonce: &str) -> Result<(Self, SocketAddr), CdpError> {
        let loopback_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        Self::bind_with_addr(loopback_addr, session_nonce).await
    }

    /// Constructs and starts an authenticated CDP loopback broker with a newly generated random nonce.
    pub async fn bind_ephemeral_random() -> Result<(Self, SocketAddr), CdpError> {
        let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
        Self::bind_ephemeral(&nonce).await
    }

    /// Explicitly binds with a socket address, strictly enforcing loopback-only safety checks.
    pub async fn bind_with_addr(addr: SocketAddr, session_nonce: &str) -> Result<(Self, SocketAddr), CdpError> {
        if !addr.ip().is_loopback() {
            return Err(CdpError::NonLoopbackBindingForbidden(addr.ip()));
        }

        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        let (event_tx, _) = broadcast::channel(256);
        let shutdown = Arc::new(AtomicBool::new(false));
        let sessions = Arc::new(RwLock::new(HashMap::new()));

        let broker = Self {
            bind_addr: local_addr,
            session_nonce: session_nonce.to_string(),
            event_tx: event_tx.clone(),
            sessions: sessions.clone(),
            shutdown: shutdown.clone(),
        };

        let active_nonce = session_nonce.to_string();
        let event_tx_clone = event_tx.clone();
        let shutdown_clone = shutdown.clone();

        // Spawn background connection listener
        tokio::spawn(async move {
            while !shutdown_clone.load(Ordering::Relaxed) {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        let nonce = active_nonce.clone();
                        let tx = event_tx_clone.clone();

                        tokio::spawn(async move {
                            Self::handle_connection(stream, peer_addr, nonce, tx).await;
                        });
                    }
                    Err(_) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
        });

        Ok((broker, local_addr))
    }

    /// Return the assigned loopback port.
    pub fn port(&self) -> u16 {
        self.bind_addr.port()
    }

    /// Return the bound socket address.
    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    /// Return the ephemeral nonce.
    pub fn nonce(&self) -> &str {
        &self.session_nonce
    }

    /// Return the event sender broadcast channel.
    pub fn event_sender(&self) -> broadcast::Sender<CdpEvent> {
        self.event_tx.clone()
    }

    /// Produce a connection descriptor for authorized UI/telemetry consumers.
    pub fn descriptor(&self) -> CdpConnectionDescriptor {
        CdpConnectionDescriptor {
            port: self.bind_addr.port(),
            nonce: self.session_nonce.clone(),
            ws_url: format!("ws://127.0.0.1:{}/cdp?nonce={}", self.bind_addr.port(), self.session_nonce),
        }
    }

    /// Register a new named CDP session and return the receiving end of the event channel.
    pub async fn open_session(&self, id: SessionId) -> mpsc::UnboundedReceiver<serde_json::Value> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.sessions.write().await.insert(id, tx);
        rx
    }

    /// Close an existing session.
    pub async fn close_session(&self, id: &SessionId) -> Result<(), CdpError> {
        self.sessions
            .write()
            .await
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| CdpError::SessionNotFound(id.clone()))
    }

    /// Broadcast a typed CDP event payload to all open sessions.
    pub fn broadcast_event(&self, event: CdpEvent) -> Result<usize, broadcast::error::SendError<CdpEvent>> {
        self.event_tx.send(event)
    }

    /// Broadcast a raw CDP JSON payload to legacy subscribers.
    pub async fn broadcast(&self, event: serde_json::Value) {
        let sessions = self.sessions.read().await;
        for tx in sessions.values() {
            let _ = tx.send(event.clone());
        }
    }

    /// Signal shutdown to the background connection listener.
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
        let expected_nonce_clone = expected_nonce.clone();

        let ws_stream_res = tokio_tungstenite::accept_hdr_async(
            stream,
            |req: &Request, res: Response| -> Result<Response, ErrorResponse> {
                let nonce_header = req.headers().get(NONCE_HEADER);
                match nonce_header {
                    None => {
                        let mut resp = ErrorResponse::new(Some("401 Unauthorized: Missing X-KAGE-Session-Nonce".to_string()));
                        *resp.status_mut() = StatusCode::UNAUTHORIZED;
                        Err(resp)
                    }
                    Some(val) => match val.to_str() {
                        Ok(v) if v == expected_nonce_clone => Ok(res),
                        _ => {
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
                                let resp = Self::handle_request(req);
                                if let Ok(resp_text) = serde_json::to_string(&resp) {
                                    if ws_stream.send(Message::Text(resp_text)).await.is_err() {
                                        break;
                                    }
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
                        if let Ok(event_text) = serde_json::to_string(&event) {
                            if ws_stream.send(Message::Text(event_text)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_request(req: CdpRequest) -> CdpResponse {
        // Special case: simulated errors for testing error handling
        if req.method.starts_with("Error.") {
            return CdpResponse {
                id: req.id,
                result: None,
                error: Some(CdpResponseError {
                    code: -32601,
                    message: format!("Method '{}' not found", req.method),
                    data: None,
                }),
                session_id: req.session_id,
            };
        }

        // Domain enables
        if req.method.ends_with(".enable") {
            return CdpResponse {
                id: req.id,
                result: Some(serde_json::json!({ "enabled": true })),
                error: None,
                session_id: req.session_id,
            };
        }

        // Target attachment simulation
        if req.method == "Target.attachToTarget" {
            let session_id = format!("session_{}", Uuid::new_v4().simple());
            return CdpResponse {
                id: req.id,
                result: Some(serde_json::json!({ "sessionId": session_id })),
                error: None,
                session_id: req.session_id,
            };
        }

        // Default response
        CdpResponse {
            id: req.id,
            result: Some(serde_json::json!({ "acknowledged": true, "method": req.method })),
            error: None,
            session_id: req.session_id,
        }
    }
}

impl Default for CdpBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn nonce_is_nonempty() {
        let broker = CdpBroker::new();
        assert!(!broker.nonce().is_empty(), "Ephemeral nonce must be generated");
    }

    #[tokio::test]
    async fn non_loopback_binding_strictly_rejected() {
        let non_loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9222);
        let res = CdpBroker::bind_with_addr(non_loopback, "test_nonce").await;
        assert!(matches!(res, Err(CdpError::NonLoopbackBindingForbidden(_))));
    }

    #[tokio::test]
    async fn open_and_receive_event() {
        let broker = CdpBroker::new();
        let id = SessionId("devtools".into());
        let mut rx = broker.open_session(id.clone()).await;
        broker.broadcast(json!({ "method": "DOM.documentUpdated" })).await;
        let msg = rx.recv().await.expect("should receive broadcast");
        assert_eq!(msg["method"], "DOM.documentUpdated");
    }

    #[tokio::test]
    async fn close_unknown_session_errors() {
        let broker = CdpBroker::new();
        let id = SessionId("ghost".into());
        let err = broker.close_session(&id).await.unwrap_err();
        assert!(matches!(err, CdpError::SessionNotFound(_)));
    }
}
