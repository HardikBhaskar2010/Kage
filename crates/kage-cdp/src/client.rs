//! Low-level CDP method call client (request/response over the broker channel).
//!
//! `CdpClient` wraps the [`CdpBroker`] and provides a typed `call()` method that
//! serializes a CDP command, sends it over the WebSocket, and awaits the matching
//! response by `id`.  This is a scaffold stub; the full implementation
//! will be wired to a live tungstenite connection in Chunk 7.

use crate::broker::{CdpBroker, CdpError};
use std::sync::Arc;

/// Typed CDP method call client backed by the [`CdpBroker`].
pub struct CdpClient {
    #[allow(dead_code)]
    broker: Arc<CdpBroker>,
}

impl CdpClient {
    pub fn new(broker: Arc<CdpBroker>) -> Self {
        CdpClient { broker }
    }

    /// Send a CDP command and return the result value.
    ///
    /// Stub: returns a placeholder until the live WebSocket is connected.
    pub async fn call(
        &self,
        method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, CdpError> {
        // TODO(chunk-7): serialize command, send over WS, await matched response ID.
        tracing::debug!("CdpClient::call stub — method={method}");
        Ok(serde_json::json!({ "stub": true, "method": method }))
    }
}
