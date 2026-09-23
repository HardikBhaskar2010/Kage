//! Telemetry coordinator for KAGE DevTools & Context Engine (Phase 5).
//!
//! Manages tab-scoped bounded ring buffers (`ConsoleRingBuffer`, `NetworkRingBuffer`,
//! `DomTreeStore`), multiplexes incoming CDP domain events across tab sessions,
//! and synthesizes live `BrowserObservation` packs within the 4,000-token budget.
//!
//! Invariants:
//! - Sub-2ms tab context switching (< 2ms benchmark target).
//! - Zero leakage across tabs (strict session-to-tab routing).
//! - Safe secret redaction and untrusted XML framing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tracing::trace;

use kage_context::telemetry::{ConsoleLogLevel, ConsoleRingBuffer, DomTreeStore, NetworkRingBuffer};
use kage_context::{BrowserObservation, ContextPackAssembler, ContextPackError};

use crate::tab::TabId;

/// Tab-scoped telemetry state holding live bounded ring buffers and DOM cache.
pub struct TabTelemetry {
    pub tab_id: TabId,
    pub url: Mutex<String>,
    pub title: Mutex<String>,
    pub origin: Mutex<String>,
    pub console: Mutex<ConsoleRingBuffer>,
    pub network: Mutex<NetworkRingBuffer>,
    pub dom: Mutex<DomTreeStore>,
    pub focus_node_id: Mutex<Option<i64>>,
    pub last_updated_ms: AtomicU64,
}

impl TabTelemetry {
    pub fn new(tab_id: TabId, initial_url: &str) -> Self {
        let origin = Self::extract_origin(initial_url);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        TabTelemetry {
            tab_id,
            url: Mutex::new(initial_url.to_string()),
            title: Mutex::new("New Tab".to_string()),
            origin: Mutex::new(origin),
            console: Mutex::new(ConsoleRingBuffer::default()),
            network: Mutex::new(NetworkRingBuffer::default()),
            dom: Mutex::new(DomTreeStore::new()),
            focus_node_id: Mutex::new(None),
            last_updated_ms: AtomicU64::new(now),
        }
    }

    fn extract_origin(url: &str) -> String {
        if let Some(pos) = url.find("://") {
            let rest = &url[pos + 3..];
            let host = rest.split('/').next().unwrap_or("");
            format!("{}://{}", &url[..pos], host)
        } else {
            url.to_string()
        }
    }

    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.last_updated_ms.store(now, Ordering::Relaxed);
    }
}

/// Central coordinator for browser telemetry and AI observation packs.
#[derive(Clone)]
pub struct TelemetryCoordinator {
    tabs: Arc<RwLock<HashMap<TabId, Arc<TabTelemetry>>>>,
    session_to_tab: Arc<RwLock<HashMap<String, TabId>>>,
    active_tab_id: Arc<RwLock<Option<TabId>>>,
    assembler: Arc<ContextPackAssembler>,
}

impl TelemetryCoordinator {
    pub fn new() -> Self {
        TelemetryCoordinator {
            tabs: Arc::new(RwLock::new(HashMap::new())),
            session_to_tab: Arc::new(RwLock::new(HashMap::new())),
            active_tab_id: Arc::new(RwLock::new(None)),
            assembler: Arc::new(ContextPackAssembler::default_budget()),
        }
    }

    /// Register a new tab in the coordinator.
    pub async fn register_tab(&self, tab_id: TabId, initial_url: &str) -> Arc<TabTelemetry> {
        let tab_telemetry = Arc::new(TabTelemetry::new(tab_id, initial_url));
        let mut tabs = self.tabs.write().await;
        tabs.insert(tab_id, tab_telemetry.clone());

        let mut active = self.active_tab_id.write().await;
        if active.is_none() {
            *active = Some(tab_id);
        }

        tab_telemetry
    }

    /// Unregister a closed tab and cleanup session bindings.
    pub async fn unregister_tab(&self, tab_id: &TabId) {
        let mut tabs = self.tabs.write().await;
        tabs.remove(tab_id);

        let mut sessions = self.session_to_tab.write().await;
        sessions.retain(|_, tid| tid != tab_id);

        let mut active = self.active_tab_id.write().await;
        if *active == Some(*tab_id) {
            *active = tabs.keys().next().copied();
        }
    }

    /// Associate a CDP session ID with a specific TabId.
    pub async fn bind_session(&self, session_id: impl Into<String>, tab_id: TabId) {
        let mut sessions = self.session_to_tab.write().await;
        sessions.insert(session_id.into(), tab_id);
    }

    /// Switch active tab. Operates in < 2ms without holding long locks.
    pub async fn set_active_tab(&self, tab_id: TabId) {
        let mut active = self.active_tab_id.write().await;
        *active = Some(tab_id);
    }

    /// Get current active tab ID.
    pub async fn active_tab(&self) -> Option<TabId> {
        *self.active_tab_id.read().await
    }

    /// Look up telemetry buffers for a specific TabId.
    pub async fn get_tab_telemetry(&self, tab_id: &TabId) -> Option<Arc<TabTelemetry>> {
        let tabs = self.tabs.read().await;
        tabs.get(tab_id).cloned()
    }

    /// Retrieve telemetry buffers for the currently active tab.
    pub async fn get_active_telemetry(&self) -> Option<Arc<TabTelemetry>> {
        let active_id = (*self.active_tab_id.read().await)?;
        let tabs = self.tabs.read().await;
        tabs.get(&active_id).cloned()
    }

    /// Ingest a CDP domain event routed by its CDP `sessionId`.
    pub async fn ingest_cdp_event(&self, session_id: &str, method: &str, params: &serde_json::Value) {
        let tab_id = {
            let sessions = self.session_to_tab.read().await;
            sessions.get(session_id).copied()
        };

        let tab_id = match tab_id {
            Some(id) => id,
            None => {
                trace!("No tab registered for CDP session '{}'", session_id);
                return;
            }
        };

        let telemetry = match self.get_tab_telemetry(&tab_id).await {
            Some(t) => t,
            None => return,
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        telemetry.touch();

        match method {
            // Console / Runtime
            "Runtime.consoleAPICalled" => {
                let console_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                let level = ConsoleLogLevel::parse(console_type);
                let args_text = if let Some(args) = params.get("args").and_then(|v| v.as_array()) {
                    args.iter()
                        .map(|a| {
                            a.get("value")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| a.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                };

                let mut console = telemetry.console.lock().await;
                console.push(level, "v8", &args_text, None, now);
            }

            "Log.entryAdded" => {
                if let Some(entry) = params.get("entry") {
                    let level_str = entry.get("level").and_then(|v| v.as_str()).unwrap_or("info");
                    let level = ConsoleLogLevel::parse(level_str);
                    let text = entry.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let source = entry.get("source").and_then(|v| v.as_str()).unwrap_or("log");

                    let mut console = telemetry.console.lock().await;
                    console.push(level, source, text, None, now);
                }
            }

            // Network
            "Network.requestWillBeSent" => {
                if let (Some(req_id), Some(req_obj)) = (
                    params.get("requestId").and_then(|v| v.as_str()),
                    params.get("request"),
                ) {
                    let url = req_obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let method = req_obj.get("method").and_then(|v| v.as_str()).unwrap_or("GET");

                    let mut network = telemetry.network.lock().await;
                    network.on_request_will_be_sent(req_id, url, method, now);
                }
            }

            "Network.responseReceived" => {
                if let (Some(req_id), Some(resp_obj)) = (
                    params.get("requestId").and_then(|v| v.as_str()),
                    params.get("response"),
                ) {
                    let status = resp_obj.get("status").and_then(|v| v.as_i64()).unwrap_or(200) as i32;
                    let mime = resp_obj.get("mimeType").and_then(|v| v.as_str());

                    let mut network = telemetry.network.lock().await;
                    network.on_response_received(req_id, status, mime, now);
                }
            }

            "Network.loadingFinished" => {
                if let Some(req_id) = params.get("requestId").and_then(|v| v.as_str()) {
                    let mut network = telemetry.network.lock().await;
                    network.on_loading_finished(req_id, now);
                }
            }

            "Network.loadingFailed" => {
                if let (Some(req_id), Some(err_text)) = (
                    params.get("requestId").and_then(|v| v.as_str()),
                    params.get("errorText").and_then(|v| v.as_str()),
                ) {
                    let mut network = telemetry.network.lock().await;
                    network.on_loading_failed(req_id, err_text, now);
                }
            }

            // DOM Mutations
            "DOM.setChildNodes" => {
                let mut dom = telemetry.dom.lock().await;
                if let (Some(parent_id), Some(nodes)) = (
                    params.get("parentId").and_then(|v| v.as_i64()),
                    params.get("nodes").and_then(|v| v.as_array()),
                ) {
                    for node_val in nodes {
                        dom.child_node_inserted(parent_id, 0, node_val);
                    }
                }
            }

            "DOM.childNodeInserted" => {
                let mut dom = telemetry.dom.lock().await;
                if let (Some(parent_id), Some(prev_id), Some(node_val)) = (
                    params.get("parentNodeId").and_then(|v| v.as_i64()),
                    params.get("previousNodeId").and_then(|v| v.as_i64()),
                    params.get("node"),
                ) {
                    dom.child_node_inserted(parent_id, prev_id, node_val);
                }
            }

            "DOM.childNodeRemoved" => {
                let mut dom = telemetry.dom.lock().await;
                if let (Some(parent_id), Some(node_id)) = (
                    params.get("parentNodeId").and_then(|v| v.as_i64()),
                    params.get("nodeId").and_then(|v| v.as_i64()),
                ) {
                    dom.child_node_removed(parent_id, node_id);
                }
            }

            "DOM.attributeModified" => {
                let mut dom = telemetry.dom.lock().await;
                if let (Some(node_id), Some(name), Some(value)) = (
                    params.get("nodeId").and_then(|v| v.as_i64()),
                    params.get("name").and_then(|v| v.as_str()),
                    params.get("value").and_then(|v| v.as_str()),
                ) {
                    dom.attribute_modified(node_id, name, value);
                }
            }

            "DOM.attributeRemoved" => {
                let mut dom = telemetry.dom.lock().await;
                if let (Some(node_id), Some(name)) = (
                    params.get("nodeId").and_then(|v| v.as_i64()),
                    params.get("name").and_then(|v| v.as_str()),
                ) {
                    dom.attribute_removed(node_id, name);
                }
            }

            // Page navigation update
            "Page.frameNavigated" => {
                if let Some(frame) = params.get("frame") {
                    let parent_id = frame.get("parentId");
                    // Main frame navigation only (INV-12)
                    if parent_id.is_none() || parent_id.and_then(|v| v.as_str()).map_or(false, |s| s.is_empty()) {
                        if let Some(new_url) = frame.get("url").and_then(|v| v.as_str()) {
                            let mut url_lock = telemetry.url.lock().await;
                            *url_lock = new_url.to_string();
                            let mut origin_lock = telemetry.origin.lock().await;
                            *origin_lock = TabTelemetry::extract_origin(new_url);
                        }
                    }
                }
            }

            _ => {
                trace!("Ignored CDP event in telemetry coordinator: {}", method);
            }
        }
    }

    /// Assembles a high-level `BrowserObservation` for the active tab.
    pub async fn build_active_observation(&self) -> Result<Option<BrowserObservation>, ContextPackError> {
        let telemetry = match self.get_active_telemetry().await {
            Some(t) => t,
            None => return Ok(None),
        };

        let origin = telemetry.origin.lock().await.clone();
        let url = telemetry.url.lock().await.clone();
        let title = telemetry.title.lock().await.clone();
        let focus_node = *telemetry.focus_node_id.lock().await;
        let dom = telemetry.dom.lock().await;
        let console = telemetry.console.lock().await;
        let network = telemetry.network.lock().await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let obs = self.assembler.assemble_from_telemetry(
            &origin,
            &url,
            &title,
            now,
            &*dom,
            focus_node,
            &*console,
            &*network,
        )?;

        Ok(Some(obs))
    }
}

impl Default for TelemetryCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    #[tokio::test]
    async fn test_sub_2ms_tab_switching() {
        let coord = TelemetryCoordinator::new();
        let tab1 = TabId::new();
        let tab2 = TabId::new();

        coord.register_tab(tab1, "https://example.com/tab1").await;
        coord.register_tab(tab2, "https://example.com/tab2").await;

        // Switch to tab 2
        let start = Instant::now();
        coord.set_active_tab(tab2).await;
        let active_tel = coord.get_active_telemetry().await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(active_tel.tab_id, tab2);
        assert!(
            elapsed.as_millis() < 2,
            "Tab switching took {:?}, exceeding 2ms benchmark",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_cdp_event_routing_to_tab_telemetry() {
        let coord = TelemetryCoordinator::new();
        let tab1 = TabId::new();
        let tab2 = TabId::new();

        coord.register_tab(tab1, "https://example.com/app1").await;
        coord.register_tab(tab2, "https://example.com/app2").await;

        coord.bind_session("session-1", tab1).await;
        coord.bind_session("session-2", tab2).await;

        // Ingest console event into session 1
        coord
            .ingest_cdp_event(
                "session-1",
                "Runtime.consoleAPICalled",
                &json!({
                    "type": "error",
                    "args": [{"value": "Session 1 Error"}]
                }),
            )
            .await;

        // Ingest network event into session 2
        coord
            .ingest_cdp_event(
                "session-2",
                "Network.requestWillBeSent",
                &json!({
                    "requestId": "req-99",
                    "request": {
                        "url": "https://api.tab2.com/data",
                        "method": "POST"
                    }
                }),
            )
            .await;

        let tel1 = coord.get_tab_telemetry(&tab1).await.unwrap();
        let tel2 = coord.get_tab_telemetry(&tab2).await.unwrap();

        // Verify isolation
        let console1 = tel1.console.lock().await;
        assert_eq!(console1.len(), 1);
        assert_eq!(console1.entries()[0].text, "Session 1 Error");

        let console2 = tel2.console.lock().await;
        assert_eq!(console2.len(), 0);

        let net1 = tel1.network.lock().await;
        assert_eq!(net1.len(), 0);

        let net2 = tel2.network.lock().await;
        assert_eq!(net2.len(), 1);
        assert_eq!(net2.entries()[0].url, "https://api.tab2.com/data");
    }
}
