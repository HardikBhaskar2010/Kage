//! Governed DOM inspection tools implementing Developer Intelligence (Phase 6).
//!
//! # Architecture & Contracts Enforced
//! - `INV-01`: AI/caller never accesses CEF directly.
//! - `INV-02`: Dispatched strictly through `ToolBus`.
//! - `INV-04`: Capability tier is `PermissionTier::ReadTelemetry`.
//! - `INV-09`: Cancellation token immediately aborts in-flight CDP awaits.
//! - `INV-10`/`INV-12`: Strict identity binding `(TabId -> TargetId -> SessionId)`.
//! - `INV-11A`: Health validation rejects closing/terminated tabs.

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
// 1. devtools.dom.get_document
// ============================================================================

/// Tool to retrieve the DOM document root or subtree.
pub struct DomGetDocumentTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomGetDocumentTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomGetDocumentTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.get_document"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "depth": { "type": "integer", "description": "Subtree depth (-1 for full tree)" },
                "pierce": { "type": "boolean", "description": "Traverse through iframes and shadow roots" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let depth = request.args.get("depth").and_then(|v| v.as_i64()).unwrap_or(-1);
        let pierce = request.args.get("pierce").and_then(|v| v.as_bool()).unwrap_or(true);

        let params = json!({
            "depth": depth,
            "pierce": pierce,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.getDocument failed: {e}").into(),
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

// ============================================================================
// 2. devtools.dom.query_selector
// ============================================================================

/// Tool to query a single DOM element by CSS selector within a node context.
pub struct DomQuerySelectorTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomQuerySelectorTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomQuerySelectorTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.query_selector"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "node_id": { "type": "integer", "description": "Context NodeId to query within" },
                "selector": { "type": "string", "description": "CSS selector query" }
            },
            "required": ["tab_id", "node_id", "selector"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let node_id = request.args["node_id"].as_i64().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'node_id' integer argument".into(),
        })?;

        let selector = request.args["selector"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'selector' string argument".into(),
        })?;

        let params = json!({
            "nodeId": node_id,
            "selector": selector,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelector", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.querySelector failed: {e}").into(),
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

// ============================================================================
// 3. devtools.dom.query_selector_all
// ============================================================================

/// Tool to query all matching DOM elements by CSS selector within a node context.
pub struct DomQuerySelectorAllTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomQuerySelectorAllTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomQuerySelectorAllTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.query_selector_all"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "node_id": { "type": "integer", "description": "Context NodeId to query within" },
                "selector": { "type": "string", "description": "CSS selector query" }
            },
            "required": ["tab_id", "node_id", "selector"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let node_id = request.args["node_id"].as_i64().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'node_id' integer argument".into(),
        })?;

        let selector = request.args["selector"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'selector' string argument".into(),
        })?;

        let params = json!({
            "nodeId": node_id,
            "selector": selector,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelectorAll", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.querySelectorAll failed: {e}").into(),
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

// ============================================================================
// 4. devtools.dom.get_outer_html
// ============================================================================

/// Tool to retrieve the outer HTML markup of a specific DOM node.
pub struct DomGetOuterHtmlTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomGetOuterHtmlTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomGetOuterHtmlTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.get_outer_html"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "node_id": { "type": "integer", "description": "NodeId whose outer HTML to retrieve" }
            },
            "required": ["tab_id", "node_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let node_id = request.args["node_id"].as_i64().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'node_id' integer argument".into(),
        })?;

        let params = json!({ "nodeId": node_id });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getOuterHTML", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.getOuterHTML failed: {e}").into(),
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

// ============================================================================
// 5. devtools.dom.get_attributes
// ============================================================================

/// Tool to retrieve all name/value attributes of a specific DOM node.
pub struct DomGetAttributesTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomGetAttributesTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomGetAttributesTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.get_attributes"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "node_id": { "type": "integer", "description": "NodeId whose attributes to retrieve" }
            },
            "required": ["tab_id", "node_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let node_id = request.args["node_id"].as_i64().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'node_id' integer argument".into(),
        })?;

        let params = json!({ "nodeId": node_id });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getAttributes", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.getAttributes failed: {e}").into(),
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

// ============================================================================
// 6. devtools.dom.get_bounds
// ============================================================================

/// Tool to retrieve the CSS box model coordinates and bounding dimensions of a DOM node.
pub struct DomGetBoundsTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DomGetBoundsTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for DomGetBoundsTool {
    fn tool_id(&self) -> &'static str {
        "devtools.dom.get_bounds"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "node_id": { "type": "integer", "description": "NodeId whose box model bounds to retrieve" }
            },
            "required": ["tab_id", "node_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let node_id = request.args["node_id"].as_i64().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'node_id' integer argument".into(),
        })?;

        let params = json!({ "nodeId": node_id });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getBoxModel", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP DOM.getBoxModel failed: {e}").into(),
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
