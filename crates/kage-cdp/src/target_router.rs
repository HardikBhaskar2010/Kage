//! Target and session routing for CDP multiplexing (INV-10, INV-12).
//!
//! # The Identity Separation Mandate
//!
//! In KAGE, CDP identity NEVER replaces or contaminates authoritative browser identity:
//! ```text
//! Authoritative Browser Identity: TabId + ProfileId + CefBrowserId
//! CDP Target Association:        TargetId (CdpBinding)
//! Multiplexed Protocol Session:  SessionId (CdpSession)
//! ```
//!
//! `TargetRouter` maintains the bidirectional association between KAGE `TabId`s
//! and CDP `TargetId`s, and routes protocol sessions (`SessionId`) to their attached targets.

use crate::broker::SessionId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Details about an active multiplexed CDP protocol session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSessionInfo {
    pub session_id: SessionId,
    pub target_id: String,
    pub tab_id: Option<Uuid>,
    pub client_name: String,
    pub attached_at_ms: u64,
}

/// Target and Session Router managing CDP target-to-tab mappings and multi-session channels.
#[derive(Clone)]
pub struct TargetRouter {
    tab_to_target: Arc<RwLock<HashMap<Uuid, String>>>,
    target_to_tab: Arc<RwLock<HashMap<String, Uuid>>>,
    sessions: Arc<RwLock<HashMap<SessionId, TargetSessionInfo>>>,
}

impl TargetRouter {
    /// Create a new, empty target router.
    pub fn new() -> Self {
        Self {
            tab_to_target: Arc::new(RwLock::new(HashMap::new())),
            target_to_tab: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Associates a CDP `TargetId` with an authoritative KAGE `TabId`.
    ///
    /// This is an association relationship only; it preserves the strict identity
    /// separation contract (`TabId + ProfileId + CefBrowserId` = browser identity).
    pub async fn bind_target(&self, tab_id: Uuid, target_id: impl Into<String>) {
        let tid = target_id.into();
        self.tab_to_target.write().await.insert(tab_id, tid.clone());
        self.target_to_tab.write().await.insert(tid, tab_id);
    }

    /// Unbinds a CDP `TargetId` and detaches all sessions associated with it.
    pub async fn unbind_target(&self, target_id: &str) -> Option<Uuid> {
        let tab_id = self.target_to_tab.write().await.remove(target_id)?;
        self.tab_to_target.write().await.remove(&tab_id);

        // Detach all sessions targeting this target
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, info| info.target_id != target_id);

        Some(tab_id)
    }

    /// Look up the `TargetId` currently bound to a given `TabId`.
    pub async fn get_target_for_tab(&self, tab_id: &Uuid) -> Option<String> {
        self.tab_to_target.read().await.get(tab_id).cloned()
    }

    /// Look up the `TabId` currently bound to a given `TargetId`.
    pub async fn get_tab_for_target(&self, target_id: &str) -> Option<Uuid> {
        self.target_to_tab.read().await.get(target_id).copied()
    }

    /// Attach a new named client session (`"devtools"`, `"context"`, `"toolbus"`)
    /// to a specific `TargetId`.
    pub async fn attach_session(
        &self,
        target_id: impl Into<String>,
        client_name: impl Into<String>,
    ) -> SessionId {
        let tid = target_id.into();
        let cname = client_name.into();
        let sid = SessionId(format!("kage_sess_{}_{}", cname, Uuid::new_v4().simple()));
        let tab_id = self.target_to_tab.read().await.get(&tid).copied();

        let info = TargetSessionInfo {
            session_id: sid.clone(),
            target_id: tid,
            tab_id,
            client_name: cname,
            attached_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        self.sessions.write().await.insert(sid.clone(), info);
        sid
    }

    /// Detach an existing session.
    pub async fn detach_session(&self, session_id: &SessionId) -> Option<TargetSessionInfo> {
        self.sessions.write().await.remove(session_id)
    }

    /// Retrieve session metadata for an active session.
    pub async fn get_session_info(&self, session_id: &SessionId) -> Option<TargetSessionInfo> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all active session IDs attached to a given `TargetId`.
    pub async fn list_sessions_for_target(&self, target_id: &str) -> Vec<SessionId> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|info| info.target_id == target_id)
            .map(|info| info.session_id.clone())
            .collect()
    }

    /// Return count of actively mapped targets.
    pub async fn target_count(&self) -> usize {
        self.target_to_tab.read().await.len()
    }

    /// Return count of actively attached sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for TargetRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_target_tab_binding_lifecycle() {
        let router = TargetRouter::new();
        let tab_id = Uuid::new_v4();
        let target_id = "target_cef_page_1001";

        // Bind
        router.bind_target(tab_id, target_id).await;
        assert_eq!(router.get_target_for_tab(&tab_id).await, Some(target_id.to_string()));
        assert_eq!(router.get_tab_for_target(target_id).await, Some(tab_id));
        assert_eq!(router.target_count().await, 1);

        // Attach sessions
        let devtools_session = router.attach_session(target_id, "devtools").await;
        let context_session = router.attach_session(target_id, "context").await;
        assert_eq!(router.session_count().await, 2);

        let sessions = router.list_sessions_for_target(target_id).await;
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&devtools_session));
        assert!(sessions.contains(&context_session));

        // Unbind target automatically clears target sessions
        let unbound_tab = router.unbind_target(target_id).await;
        assert_eq!(unbound_tab, Some(tab_id));
        assert_eq!(router.target_count().await, 0);
        assert_eq!(router.session_count().await, 0);
    }
}
