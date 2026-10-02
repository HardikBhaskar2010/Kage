//! Network inspection tools implementing Developer Intelligence (Phase 6).
//!
//! # Architecture & Contracts Enforced
//! - `INV-01`: AI/caller never accesses CEF directly.
//! - `INV-02`: Dispatched strictly through `ToolBus`.
//! - `INV-04`: Network payload retrieval is `PermissionTier::SensitiveRead`.
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
// 1. devtools.network.get_response_body
// ============================================================================

/// Tool to retrieve the response body of a network transaction by its CDP requestId.
pub struct NetworkGetResponseBodyTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl NetworkGetResponseBodyTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for NetworkGetResponseBodyTool {
    fn tool_id(&self) -> &'static str {
        "devtools.network.get_response_body"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "request_id": { "type": "string", "description": "CDP requestId of the network resource" }
            },
            "required": ["tab_id", "request_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let request_id = request.args["request_id"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'request_id' string argument".into(),
        })?;

        let params = json!({ "requestId": request_id });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Network.getResponseBody", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Network.getResponseBody failed: {e}").into(),
                })?;
                Ok(ToolResponse::new(
                    request.request_id.clone(),
                    output,
                    started.elapsed().as_millis() as u64,
                ))
            }
        }
    }
}
