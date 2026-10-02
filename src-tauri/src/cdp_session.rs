//! Host-side CDP session lifecycle manager (INV-10, INV-12).
//!
//! Maintains a persistent loopback CDP client connection and caches active protocol
//! sessions per `(TabId, TargetId)` to avoid creating throwaway WebSocket connections
//! or spawning redundant `Target.attachToTarget` protocol sessions on every evaluation.

use std::collections::HashMap;
use std::sync::Arc;
use kage_browser::TabId;
use kage_cdp::{CdpBroker, CdpClient, CdpError};
use serde_json::json;
use tokio::sync::RwLock;

/// Manages persistent CDP client connectivity and attached protocol session lifetimes.
pub struct CdpSessionManager {
    broker: Arc<CdpBroker>,
    client: RwLock<Option<Arc<CdpClient>>>,
    tab_sessions: RwLock<HashMap<TabId, (String, String)>>, // TabId -> (TargetId, SessionId)
}

impl CdpSessionManager {
    /// Create a new `CdpSessionManager` anchored to the local authenticated `CdpBroker`.
    pub fn new(broker: Arc<CdpBroker>) -> Self {
        Self {
            broker,
            client: RwLock::new(None),
            tab_sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Retrieve or establish the persistent loopback `CdpClient` connection.
    pub async fn get_client(&self) -> Result<Arc<CdpClient>, CdpError> {
        {
            let guard = self.client.read().await;
            if let Some(ref c) = *guard {
                return Ok(c.clone());
            }
        }

        let mut guard = self.client.write().await;
        if let Some(ref c) = *guard {
            return Ok(c.clone());
        }

        let desc = self.broker.descriptor();
        tracing::debug!(
            port = desc.port,
            "Establishing persistent host CDP loopback client connection"
        );

        let client = CdpClient::connect(&desc.ws_url, Some(&desc.nonce)).await?;
        let client_arc = Arc::new(client);
        *guard = Some(client_arc.clone());
        Ok(client_arc)
    }

    /// Invalidate the active client connection on fatal transport failure.
    pub async fn invalidate_client(&self) {
        let mut guard = self.client.write().await;
        *guard = None;
        let mut sessions = self.tab_sessions.write().await;
        sessions.clear();
    }

    /// Resolve an existing live `SessionId` for a `(TabId, TargetId)`, or attach a new session.
    pub async fn get_or_attach_session(&self, tab_id: TabId, target_id: &str) -> Result<String, CdpError> {
        {
            let sessions = self.tab_sessions.read().await;
            if let Some((cached_target, session_id)) = sessions.get(&tab_id) {
                if cached_target == target_id {
                    return Ok(session_id.clone());
                }
            }
        }

        let client = self.get_client().await?;
        tracing::info!(
            tab_id = %tab_id,
            target_id = target_id,
            "Attaching persistent CDP session for target"
        );

        let attach_res = client
            .call(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
            )
            .await?;

        let session_id = attach_res["sessionId"]
            .as_str()
            .ok_or_else(|| {
                CdpError::ConnectionFailed("No sessionId returned by Target.attachToTarget".into())
            })?
            .to_string();

        let mut sessions = self.tab_sessions.write().await;
        sessions.insert(tab_id, (target_id.to_string(), session_id.clone()));
        Ok(session_id)
    }

    /// Call any CDP method on the specified tab target with automatic session multiplexing,
    /// stale-session reattachment, and single-retry error recovery.
    pub async fn call_cdp(
        &self,
        tab_id: TabId,
        target_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CdpError> {
        let client = self.get_client().await?;
        let session_id = self.get_or_attach_session(tab_id, target_id).await?;

        match client.call_session(Some(&session_id), method, params.clone()).await {
            Ok(result) => Ok(result),
            Err(CdpError::ProtocolError { ref message, .. })
                if message.contains("No session with given id") || message.contains("Target closed") =>
            {
                tracing::warn!(
                    tab_id = %tab_id,
                    target_id = target_id,
                    method = method,
                    "CDP session was stale ({message}) — re-attaching target and retrying {method}"
                );
                // Invalidate cached session
                self.invalidate_tab(tab_id).await;

                // Re-attach and retry once
                let new_session_id = self.get_or_attach_session(tab_id, target_id).await?;
                client.call_session(Some(&new_session_id), method, params).await
            }
            Err(CdpError::ConnectionClosed) => {
                tracing::warn!("CDP loopback connection was closed — reconnecting and retrying {method}");
                self.invalidate_client().await;
                let new_client = self.get_client().await?;
                let new_session_id = self.get_or_attach_session(tab_id, target_id).await?;
                new_client.call_session(Some(&new_session_id), method, params).await
            }
            Err(e) => Err(e),
        }
    }

    /// Evaluate a JavaScript expression in the context of the specified tab target via CDP `Runtime.evaluate`.
    ///
    /// If the existing session has become stale (e.g. target navigation or renderer restart),
    /// automatically detaches, re-attaches, and retries the evaluation once.
    pub async fn evaluate(
        &self,
        tab_id: TabId,
        target_id: &str,
        expression: &str,
        return_by_value: bool,
        await_promise: bool,
    ) -> Result<serde_json::Value, CdpError> {
        self.call_cdp(
            tab_id,
            target_id,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": return_by_value,
                "awaitPromise": await_promise,
                "generatePreview": true,
            }),
        )
        .await
    }

    /// Invalidate cached session for a closed or navigated tab.
    pub async fn invalidate_tab(&self, tab_id: TabId) {
        let mut sessions = self.tab_sessions.write().await;
        sessions.remove(&tab_id);
    }

    /// Explicitly seed a session binding for deterministic stale-session testing.
    pub async fn seed_session_for_test(&self, tab_id: TabId, target_id: &str, session_id: &str) {
        let mut sessions = self.tab_sessions.write().await;
        sessions.insert(tab_id, (target_id.to_string(), session_id.to_string()));
    }
}
