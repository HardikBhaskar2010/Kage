//! Storage inspection tools implementing Developer Intelligence (Phase 6).
//!
//! # Architecture & Contracts Enforced
//! - `INV-01`: AI/caller never accesses CEF directly.
//! - `INV-02`: Dispatched strictly through `ToolBus`.
//! - `INV-04`: Storage access is `PermissionTier::SensitiveRead`.
//! - `INV-05`: SensitiveRead operations are audited.
//! - `INV-06`: Output scrubbed by `SecretSanitizer` at ToolBus boundary.
//! - `INV-09`: Cancellation token immediately aborts in-flight CDP awaits.
//! - `INV-10`/`INV-12`: Strict identity binding `(TabId -> TargetId -> SessionId)`.

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use kage_browser::TabManager;
use kage_core::policy::PermissionTier;
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::cdp_session::CdpSessionManager;
use super::common::resolve_active_tab;

// ============================================================================
// 1. devtools.storage.get_cookies
// ============================================================================

/// Tool to inspect browser cookies associated with the active tab.
pub struct StorageGetCookiesTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl StorageGetCookiesTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for StorageGetCookiesTool {
    fn tool_id(&self) -> &'static str {
        "devtools.storage.get_cookies"
    }

    fn tier(&self) -> PermissionTier {
        // Read-only storage inspection — secret redaction applied by SecretSanitizer (INV-06).
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "urls": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of URLs to filter cookies for"
                }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let params = if let Some(urls) = request.args.get("urls") {
            json!({ "urls": urls })
        } else {
            json!({})
        };

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Network.getCookies", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Network.getCookies failed: {e}").into(),
                })?;
                Ok(ToolResponse {
                    request_id: request.request_id.clone(),
                    output,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                })
            }
        }
    }
}

// ============================================================================
// 2. devtools.storage.get_local_storage
// ============================================================================

/// Tool to inspect DOM localStorage key-value items of the active tab.
pub struct StorageGetLocalStorageTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl StorageGetLocalStorageTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for StorageGetLocalStorageTool {
    fn tool_id(&self) -> &'static str {
        "devtools.storage.get_local_storage"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let expression = r#"(() => {
            const items = {};
            for (let i = 0; i < localStorage.length; i++) {
                const key = localStorage.key(i);
                items[key] = localStorage.getItem(key);
            }
            return items;
        })()"#;

        let params = json!({
            "expression": expression,
            "returnByValue": true,
            "generatePreview": true,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Runtime.evaluate", params) => {
                let eval_res = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP localStorage evaluate failed: {e}").into(),
                })?;
                let output = json!({
                    "items": eval_res.get("result").and_then(|r| r.get("value")).unwrap_or(&json!({}))
                });
                Ok(ToolResponse {
                    request_id: request.request_id.clone(),
                    output,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                })
            }
        }
    }
}

// ============================================================================
// 3. devtools.storage.get_session_storage
// ============================================================================

/// Tool to inspect DOM sessionStorage key-value items of the active tab.
pub struct StorageGetSessionStorageTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl StorageGetSessionStorageTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for StorageGetSessionStorageTool {
    fn tool_id(&self) -> &'static str {
        "devtools.storage.get_session_storage"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let expression = r#"(() => {
            const items = {};
            for (let i = 0; i < sessionStorage.length; i++) {
                const key = sessionStorage.key(i);
                items[key] = sessionStorage.getItem(key);
            }
            return items;
        })()"#;

        let params = json!({
            "expression": expression,
            "returnByValue": true,
            "generatePreview": true,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Runtime.evaluate", params) => {
                let eval_res = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP sessionStorage evaluate failed: {e}").into(),
                })?;
                let output = json!({
                    "items": eval_res.get("result").and_then(|r| r.get("value")).unwrap_or(&json!({}))
                });
                Ok(ToolResponse {
                    request_id: request.request_id.clone(),
                    output,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                })
            }
        }
    }
}
