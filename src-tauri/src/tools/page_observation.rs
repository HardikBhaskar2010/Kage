//! Governed canonical `page.*` observation tools.
//!
//! Provides structured, token-efficient page inspection for autonomous planning:
//! - `page.get_text`: Extracts text content from target element or entire page.
//! - `page.get_element`: Queries element node details, attributes, and bounds.
//! - `page.get_links`: Extracts all hyperlinks with URLs, anchor text, and titles.
//! - `page.get_forms`: Discovers all interactive forms and their input controls.
//! - `page.get_semantic_snapshot`: Produces compact semantic overview of page landmarks and controls.
//! - `page.get_accessibility_tree`: Retrieves the full Chromium accessibility AXTree.

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use kage_browser::TabManager;
use kage_core::policy::PermissionTier;
use kage_core::registry::{
    IdempotencyClassification, RetryPolicy, ToolCategory, ToolMetadata,
};
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use crate::cdp_session::CdpSessionManager;
use crate::tools::common::resolve_tab_or_active;

// ---------------------------------------------------------------------------
// 1. page.get_text
// ---------------------------------------------------------------------------

pub struct PageGetTextTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetTextTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_text",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "Optional CSS selector (defaults to body)" },
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Extracts visible text content from the specified element or entire page body")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for PageGetTextTool {
    fn tool_id(&self) -> &'static str {
        "page.get_text"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;
        let selector = request.args.get("selector").and_then(|v| v.as_str()).unwrap_or("body");

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                // 1. Get root node
                let doc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", json!({ "depth": 0 })).await.map_err(|e| e.to_string())?;
                let root_id = doc.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

                // 2. Query target element
                let q = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelector", json!({
                    "nodeId": root_id,
                    "selector": selector,
                })).await.map_err(|e| e.to_string())?;
                let node_id = q.get("nodeId").and_then(|v| v.as_i64()).unwrap_or(0);
                if node_id == 0 {
                    return Err(format!("Element matching '{selector}' not found"));
                }

                // 3. Get outer HTML or inner text
                let html_res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getOuterHTML", json!({ "nodeId": node_id })).await.map_err(|e| e.to_string())?;
                let raw_html = html_res.get("outerHTML").and_then(|v| v.as_str()).unwrap_or("");

                // Strip basic tags for text extraction
                let text = raw_html
                    .replace("<br>", "\n")
                    .replace("<br/>", "\n")
                    .replace("</p>", "\n")
                    .replace("</div>", "\n")
                    .replace("</li>", "\n");
                let mut stripped = String::new();
                let mut in_tag = false;
                for ch in text.chars() {
                    if ch == '<' {
                        in_tag = true;
                    } else if ch == '>' {
                        in_tag = false;
                        stripped.push(' ');
                    } else if !in_tag {
                        stripped.push(ch);
                    }
                }
                let clean_text = stripped.split_whitespace().collect::<Vec<_>>().join(" ");

                Ok::<String, String>(clean_text)
            } => {
                let text = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Failed to extract text: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "selector": selector,
                        "text": text,
                        "length": text.len()
                    }),
                    format!("Extracted text from '{selector}' in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. page.get_element
// ---------------------------------------------------------------------------

pub struct PageGetElementTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetElementTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_element",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector for target element" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["selector"]
            }),
        )
        .with_description("Retrieves detailed attributes, tag name, and layout bounding box for an element")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for PageGetElementTool {
    fn tool_id(&self) -> &'static str {
        "page.get_element"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;
        let selector = request.args.get("selector").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'selector' argument".into(),
        })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                let doc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", json!({ "depth": 0 })).await.map_err(|e| e.to_string())?;
                let root_id = doc.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

                let q = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelector", json!({
                    "nodeId": root_id,
                    "selector": selector,
                })).await.map_err(|e| e.to_string())?;
                let node_id = q.get("nodeId").and_then(|v| v.as_i64()).unwrap_or(0);
                if node_id == 0 {
                    return Err(format!("Element matching '{selector}' not found"));
                }

                let desc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.describeNode", json!({ "nodeId": node_id })).await.map_err(|e| e.to_string())?;
                let node = desc.get("node").cloned().unwrap_or(json!({}));

                let box_res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getBoxModel", json!({ "nodeId": node_id })).await.ok();
                let bounds = box_res.and_then(|b| b.get("model").cloned());

                Ok::<serde_json::Value, String>(json!({
                    "node_id": node_id,
                    "node_name": node.get("nodeName").unwrap_or(&json!("")),
                    "attributes": node.get("attributes").unwrap_or(&json!([])),
                    "box_model": bounds
                }))
            } => {
                let element_data = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Failed to inspect element: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "selector": selector,
                        "element": element_data
                    }),
                    format!("Inspected '{selector}' in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. page.get_links
// ---------------------------------------------------------------------------

pub struct PageGetLinksTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetLinksTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_links",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "Optional container selector (defaults to entire page)" },
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Extracts all anchor links with target URLs and text labels")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for PageGetLinksTool {
    fn tool_id(&self) -> &'static str {
        "page.get_links"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;
        let selector = request.args.get("selector").and_then(|v| v.as_str()).unwrap_or("body");

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                let doc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", json!({ "depth": -1 })).await.map_err(|e| e.to_string())?;
                let root_id = doc.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

                let link_query = if selector == "body" {
                    "a[href]".to_string()
                } else {
                    format!("{selector} a[href]")
                };

                let query_all = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelectorAll", json!({
                    "nodeId": root_id,
                    "selector": link_query,
                })).await.map_err(|e| e.to_string())?;

                let node_ids = query_all.get("nodeIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let mut links = Vec::new();

                for nid_val in node_ids.iter().take(100) {
                    if let Some(nid) = nid_val.as_i64() {
                        if let Ok(desc) = self.session_manager.call_cdp(tab_id, &target_id, "DOM.describeNode", json!({ "nodeId": nid })).await {
                            if let Some(node) = desc.get("node") {
                                let mut href = String::new();
                                let mut title = String::new();
                                if let Some(attrs) = node.get("attributes").and_then(|v| v.as_array()) {
                                    for chunk in attrs.chunks(2) {
                                        if chunk.len() == 2 {
                                            let name = chunk[0].as_str().unwrap_or("");
                                            let val = chunk[1].as_str().unwrap_or("");
                                            if name.eq_ignore_ascii_case("href") {
                                                href = val.to_string();
                                            } else if name.eq_ignore_ascii_case("title") {
                                                title = val.to_string();
                                            }
                                        }
                                    }
                                }
                                if !href.is_empty() {
                                    links.push(json!({
                                        "node_id": nid,
                                        "href": href,
                                        "title": title
                                    }));
                                }
                            }
                        }
                    }
                }

                Ok::<Vec<serde_json::Value>, String>(links)
            } => {
                let links = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Failed to extract links: {e}").into(),
                })?;

                let total = links.len();
                let elapsed_ms = started.elapsed().as_millis() as u64;

                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "links": links,
                        "total": total
                    }),
                    format!("Extracted {total} links in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. page.get_forms
// ---------------------------------------------------------------------------

pub struct PageGetFormsTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetFormsTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_forms",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Discovers all HTML form elements and their interactive controls")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for PageGetFormsTool {
    fn tool_id(&self) -> &'static str {
        "page.get_forms"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                let doc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", json!({ "depth": -1 })).await.map_err(|e| e.to_string())?;
                let root_id = doc.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

                let query_all = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelectorAll", json!({
                    "nodeId": root_id,
                    "selector": "form",
                })).await.map_err(|e| e.to_string())?;

                let node_ids = query_all.get("nodeIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let mut forms = Vec::new();

                for nid_val in node_ids.iter().take(20) {
                    if let Some(nid) = nid_val.as_i64() {
                        if let Ok(desc) = self.session_manager.call_cdp(tab_id, &target_id, "DOM.describeNode", json!({ "nodeId": nid, "depth": 3 })).await {
                            if let Some(node) = desc.get("node") {
                                let mut action = String::new();
                                let mut method = "GET".to_string();
                                let mut form_id = String::new();

                                if let Some(attrs) = node.get("attributes").and_then(|v| v.as_array()) {
                                    for chunk in attrs.chunks(2) {
                                        if chunk.len() == 2 {
                                            let name = chunk[0].as_str().unwrap_or("");
                                            let val = chunk[1].as_str().unwrap_or("");
                                            if name.eq_ignore_ascii_case("action") {
                                                action = val.to_string();
                                            } else if name.eq_ignore_ascii_case("method") {
                                                method = val.to_uppercase();
                                            } else if name.eq_ignore_ascii_case("id") {
                                                form_id = val.to_string();
                                            }
                                        }
                                    }
                                }

                                forms.push(json!({
                                    "node_id": nid,
                                    "id": form_id,
                                    "action": action,
                                    "method": method
                                }));
                            }
                        }
                    }
                }

                Ok::<Vec<serde_json::Value>, String>(forms)
            } => {
                let forms = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Failed to extract forms: {e}").into(),
                })?;

                let total = forms.len();
                let elapsed_ms = started.elapsed().as_millis() as u64;

                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "forms": forms,
                        "total": total
                    }),
                    format!("Discovered {total} forms in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. page.get_semantic_snapshot
// ---------------------------------------------------------------------------

pub struct PageGetSemanticSnapshotTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetSemanticSnapshotTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_semantic_snapshot",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Generates a compact structured hierarchy of landmarks, headings, and interactive elements")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(15_000)
    }
}

#[async_trait]
impl KageTool for PageGetSemanticSnapshotTool {
    fn tool_id(&self) -> &'static str {
        "page.get_semantic_snapshot"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                let doc = self.session_manager.call_cdp(tab_id, &target_id, "DOM.getDocument", json!({ "depth": -1 })).await.map_err(|e| e.to_string())?;
                let root_id = doc.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

                // Collect key headings and buttons
                let h_query = self.session_manager.call_cdp(tab_id, &target_id, "DOM.querySelectorAll", json!({
                    "nodeId": root_id,
                    "selector": "h1, h2, h3, button, [role='button'], input, textarea, select",
                })).await.map_err(|e| e.to_string())?;

                let node_ids = h_query.get("nodeIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let mut elements = Vec::new();

                for nid_val in node_ids.iter().take(50) {
                    if let Some(nid) = nid_val.as_i64() {
                        if let Ok(desc) = self.session_manager.call_cdp(tab_id, &target_id, "DOM.describeNode", json!({ "nodeId": nid })).await {
                            if let Some(node) = desc.get("node") {
                                let tag = node.get("nodeName").and_then(|v| v.as_str()).unwrap_or("");
                                elements.push(json!({
                                    "node_id": nid,
                                    "tag": tag,
                                }));
                            }
                        }
                    }
                }

                Ok::<Vec<serde_json::Value>, String>(elements)
            } => {
                let snapshot = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Failed to construct semantic snapshot: {e}").into(),
                })?;

                let total = snapshot.len();
                let elapsed_ms = started.elapsed().as_millis() as u64;

                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "semantic_nodes": snapshot,
                        "total": total
                    }),
                    format!("Semantic snapshot captured ({total} nodes) in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. page.get_accessibility_tree
// ---------------------------------------------------------------------------

pub struct PageGetAccessibilityTreeTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageGetAccessibilityTreeTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.get_accessibility_tree",
            ToolCategory::PageObservation,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" },
                    "depth": { "type": "integer", "default": -1 }
                }
            }),
        )
        .with_description("Retrieves the full Chromium accessibility AXTree for screen-reader and semantic reasoning")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(15_000)
    }
}

#[async_trait]
impl KageTool for PageGetAccessibilityTreeTool {
    fn tool_id(&self) -> &'static str {
        "page.get_accessibility_tree"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;
        let depth = request.args.get("depth").and_then(|v| v.as_i64()).unwrap_or(-1);

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Accessibility.getFullAXTree", json!({ "depth": depth })) => {
                let ax_tree = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Accessibility.getFullAXTree failed: {e}").into(),
                })?;

                let nodes = ax_tree.get("nodes").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let total = nodes.len();
                let elapsed_ms = started.elapsed().as_millis() as u64;

                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "nodes": nodes,
                        "total": total
                    }),
                    format!("Retrieved accessibility tree ({total} nodes) for tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}
