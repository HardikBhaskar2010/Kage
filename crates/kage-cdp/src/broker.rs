//! [`CdpBroker`] — single multiplexed WebSocket connection to the CEF debugging port.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

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

/// Errors from the CDP layer.
#[derive(Debug, Error)]
pub enum CdpError {
    #[error("WebSocket connection failed: {0}")]
    ConnectionFailed(String),

    #[error("session '{0}' not found")]
    SessionNotFound(SessionId),

    #[error("CDP method '{method}' returned error {code}: {message}")]
    ProtocolError {
        method: String,
        code: i64,
        message: String,
    },

    #[error("nonce authentication failed")]
    AuthFailed,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Connection descriptor returned to authorized internal clients (DevTools, Context Engine)
/// for connecting to the Rust-managed CDP loopback broker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdpConnectionDescriptor {
    /// Ephemeral loopback port bound by the broker (never hardcoded).
    pub port: u16,
    /// Ephemeral session nonce required for handshake authentication.
    pub nonce: String,
    /// Full authenticated WebSocket connection URL.
    pub ws_url: String,
}

/// A registered CDP session subscriber receiving raw CDP events.
type EventSink = mpsc::UnboundedSender<serde_json::Value>;

/// Central broker managing the single CDP WebSocket connection and session registry.
///
/// In production this wraps a live `tokio-tungstenite` connection.
/// The stub implementation stores sessions in-memory for scaffolding/tests.
pub struct CdpBroker {
    /// Session nonce issued at startup — must be presented by all WebSocket clients.
    nonce: String,
    /// Ephemeral loopback port dynamically bound at runtime.
    port: u16,
    /// Active session subscriptions.
    sessions: Arc<RwLock<HashMap<SessionId, EventSink>>>,
}

impl CdpBroker {
    /// Create a new broker with an ephemeral nonce and initial port.
    pub fn new() -> Self {
        Self::with_port(0)
    }

    /// Create a new broker bound to a specific or ephemeral loopback port.
    pub fn with_port(port: u16) -> Self {
        let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
        CdpBroker {
            nonce,
            port,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the dynamically bound loopback port once listener socket is established.
    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    /// Return the assigned loopback port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Return the ephemeral nonce — used by the Tauri host to construct the
    /// authenticated WebSocket URL: `ws://127.0.0.1:<port>/cdp?nonce=<nonce>`.
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// Produce a connection descriptor for authorized UI/telemetry consumers.
    pub fn descriptor(&self) -> CdpConnectionDescriptor {
        CdpConnectionDescriptor {
            port: self.port,
            nonce: self.nonce.clone(),
            ws_url: format!("ws://127.0.0.1:{}/cdp?nonce={}", self.port, self.nonce),
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

    /// Broadcast a raw CDP event payload to all open sessions.
    /// (In production, the multiplexer routes by `sessionId` field.)
    pub async fn broadcast(&self, event: serde_json::Value) {
        let sessions = self.sessions.read().await;
        for tx in sessions.values() {
            // Best-effort; a lagging subscriber should not stall others.
            let _ = tx.send(event.clone());
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
