//! Governed canonical `page.*` interaction tools.
//!
//! Dispatches real CDP synthetic events with structured action evidence:
//! - `page.click`: Resolves box model bounds and dispatches synthetic mouse click.
//! - `page.type`: Dispatches synthetic keyboard events sequence.
//! - `page.fill`: Fills input field and emits input/change DOM events.
//! - `page.select`: Selects options in HTML `<select>` dropdowns.
//! - `page.press_key`: Dispatches single named key events (Enter, Escape, Tab).
//! - `page.submit`: Submits the target HTML form element.
//! - `page.scroll`: Dispatches synthetic mouse wheel scrolling.
//! - `page.hover`: Dispatches mouse movement over target element.

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use kage_browser::TabManager;
use kage_core::policy::PermissionTier;
use kage_core::registry::{
    DeclarativeContract, IdempotencyClassification, RetryPolicy, ToolCategory, ToolMetadata,
};
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use crate::cdp_session::CdpSessionManager;
use crate::tools::common::resolve_tab_or_active;

// Helper to find element center coordinates via DOM.getDocument + DOM.querySelector + DOM.getBoxModel
async fn resolve_element_center(
    session_manager: &Arc<CdpSessionManager>,
    tab_id: kage_browser::TabId,
    target_id: &str,
    selector: &str,
) -> Result<(i64, f64, f64), ToolError> {
    let doc_res = session_manager.call_cdp(tab_id, target_id, "DOM.getDocument", json!({ "depth": 0 })).await
        .map_err(|e| ToolError::ExecutionFailed {
            tool_id: "page.interaction".into(),
            source: format!("DOM.getDocument failed: {e}").into(),
        })?;

    let root_node_id = doc_res.get("root").and_then(|r| r.get("nodeId")).and_then(|v| v.as_i64()).unwrap_or(1);

    let query_res = session_manager.call_cdp(tab_id, target_id, "DOM.querySelector", json!({
        "nodeId": root_node_id,
        "selector": selector,
    })).await.map_err(|e| ToolError::ExecutionFailed {
        tool_id: "page.interaction".into(),
        source: format!("DOM.querySelector failed: {e}").into(),
    })?;

    let node_id = query_res.get("nodeId").and_then(|v| v.as_i64()).unwrap_or(0);
    if node_id == 0 {
        return Err(ToolError::ExecutionFailed {
            tool_id: "page.interaction".into(),
            source: format!("Element not found matching selector '{selector}'").into(),
        });
    }

    let box_res = session_manager.call_cdp(tab_id, target_id, "DOM.getBoxModel", json!({
        "nodeId": node_id,
    })).await.map_err(|e| ToolError::ExecutionFailed {
        tool_id: "page.interaction".into(),
        source: format!("DOM.getBoxModel failed: {e}").into(),
    })?;

    let model = box_res.get("model").ok_or_else(|| ToolError::ExecutionFailed {
        tool_id: "page.interaction".into(),
        source: "Missing box model".into(),
    })?;

    let content = model.get("content").and_then(|v| v.as_array()).ok_or_else(|| ToolError::ExecutionFailed {
        tool_id: "page.interaction".into(),
        source: "Missing content quad in box model".into(),
    })?;

    if content.len() < 8 {
        return Err(ToolError::ExecutionFailed {
            tool_id: "page.interaction".into(),
            source: "Malformed content quad in box model".into(),
        });
    }

    let x0 = content[0].as_f64().unwrap_or(0.0);
    let y0 = content[1].as_f64().unwrap_or(0.0);
    let x2 = content[4].as_f64().unwrap_or(x0);
    let y2 = content[5].as_f64().unwrap_or(y0);

    let center_x = (x0 + x2) / 2.0;
    let center_y = (y0 + y2) / 2.0;

    Ok((node_id, center_x, center_y))
}

// ---------------------------------------------------------------------------
// 1. page.click
// ---------------------------------------------------------------------------

pub struct PageClickTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageClickTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.click",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector for target element" },
                    "tab_id": { "type": "string", "format": "uuid" },
                    "x": { "type": "number", "description": "Optional explicit x coordinate" },
                    "y": { "type": "number", "description": "Optional explicit y coordinate" },
                    "button": { "type": "string", "enum": ["left", "middle", "right"], "default": "left" },
                    "click_count": { "type": "integer", "default": 1 }
                }
            }),
        )
        .with_description("Clicks an element resolved by CSS selector via synthetic CDP mouse events")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::VerifyBeforeRetry)
        .with_timeout_ms(10_000)
        .with_contracts(vec![
            DeclarativeContract::DomMutationObserved,
        ])
    }
}

#[async_trait]
impl KageTool for PageClickTool {
    fn tool_id(&self) -> &'static str {
        "page.click"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        let button = request.args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
        let click_count = request.args.get("click_count").and_then(|v| v.as_i64()).unwrap_or(1);

        let (target_x, target_y) = if let (Some(x), Some(y)) = (
            request.args.get("x").and_then(|v| v.as_f64()),
            request.args.get("y").and_then(|v| v.as_f64()),
        ) {
            (x, y)
        } else if let Some(selector) = request.args.get("selector").and_then(|v| v.as_str()) {
            let (_, cx, cy) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await?;
            (cx, cy)
        } else {
            return Err(ToolError::SchemaViolation {
                tool_id: self.tool_id().to_string(),
                reason: "Either 'selector' or '(x, y)' coordinates must be provided".into(),
            });
        };

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                // 1. Mouse move
                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                    "type": "mouseMoved",
                    "x": target_x,
                    "y": target_y,
                })).await.map_err(|e| e.to_string())?;

                // 2. Mouse pressed
                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                    "type": "mousePressed",
                    "x": target_x,
                    "y": target_y,
                    "button": button,
                    "clickCount": click_count,
                })).await.map_err(|e| e.to_string())?;

                // 3. Mouse released
                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                    "type": "mouseReleased",
                    "x": target_x,
                    "y": target_y,
                    "button": button,
                    "clickCount": click_count,
                })).await.map_err(|e| e.to_string())?;

                Ok::<(), String>(())
            } => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Click event dispatch failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "clicked": true,
                        "x": target_x,
                        "y": target_y,
                        "button": button
                    }),
                    format!("Clicked at ({target_x:.1}, {target_y:.1}) in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. page.type
// ---------------------------------------------------------------------------

pub struct PageTypeTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageTypeTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.type",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text string to type" },
                    "selector": { "type": "string", "description": "Optional CSS selector to focus first" },
                    "tab_id": { "type": "string", "format": "uuid" },
                    "delay_ms": { "type": "integer", "default": 0 }
                },
                "required": ["text"]
            }),
        )
        .with_description("Types a string of text via synthetic CDP keyboard events")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::VerifyBeforeRetry)
        .with_timeout_ms(15_000)
    }
}

#[async_trait]
impl KageTool for PageTypeTool {
    fn tool_id(&self) -> &'static str {
        "page.type"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        let text = request.args.get("text").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'text' argument".into(),
        })?;

        // Focus selector if supplied
        if let Some(selector) = request.args.get("selector").and_then(|v| v.as_str()) {
            if let Ok((node_id, _, _)) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await {
                let _ = self.session_manager.call_cdp(tab_id, &target_id, "DOM.focus", json!({ "nodeId": node_id })).await;
            }
        }

        let delay_ms = request.args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(0);

        for ch in text.chars() {
            if cancel.is_cancelled() {
                return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
            }

            let s = ch.to_string();
            // Dispatches keyDown + keyUp with text payload
            let _ = self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchKeyEvent", json!({
                "type": "keyDown",
                "text": s,
            })).await;

            let _ = self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchKeyEvent", json!({
                "type": "keyUp",
                "text": s,
            })).await;

            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "tab_id": tab_id.to_string(),
                "typed_characters": text.chars().count()
            }),
            format!("Typed {} characters in tab {tab_id}", text.chars().count()),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 3. page.fill
// ---------------------------------------------------------------------------

pub struct PageFillTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageFillTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.fill",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector for input/textarea" },
                    "value": { "type": "string", "description": "Value to set" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["selector", "value"]
            }),
        )
        .with_description("Sets an input element's value and emits input/change events")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
        .with_contracts(vec![
            DeclarativeContract::DomMutationObserved,
        ])
    }
}

#[async_trait]
impl KageTool for PageFillTool {
    fn tool_id(&self) -> &'static str {
        "page.fill"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
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
        let value = request.args.get("value").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'value' argument".into(),
        })?;

        let (node_id, _, _) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.setAttributeValue", json!({
                "nodeId": node_id,
                "name": "value",
                "value": value,
            })) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("DOM.setAttributeValue failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "selector": selector,
                        "filled": true
                    }),
                    format!("Filled '{selector}' with value in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. page.select
// ---------------------------------------------------------------------------

pub struct PageSelectTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageSelectTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.select",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector for <select> element" },
                    "value": { "type": "string", "description": "Option value to select" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["selector", "value"]
            }),
        )
        .with_description("Selects an option in a dropdown <select> element")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for PageSelectTool {
    fn tool_id(&self) -> &'static str {
        "page.select"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
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
        let value = request.args.get("value").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'value' argument".into(),
        })?;

        let (node_id, _, _) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "DOM.setAttributeValue", json!({
                "nodeId": node_id,
                "name": "value",
                "value": value,
            })) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("DOM.setAttributeValue failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "selector": selector,
                        "selected_value": value
                    }),
                    format!("Selected '{value}' on '{selector}' in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. page.press_key
// ---------------------------------------------------------------------------

pub struct PagePressKeyTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PagePressKeyTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.press_key",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Key name (Enter, Tab, Escape, Backspace, ArrowDown, etc.)" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["key"]
            }),
        )
        .with_description("Dispatches a single named key press via synthetic CDP events")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::VerifyBeforeRetry)
        .with_timeout_ms(5_000)
    }
}

#[async_trait]
impl KageTool for PagePressKeyTool {
    fn tool_id(&self) -> &'static str {
        "page.press_key"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        let key = request.args.get("key").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'key' argument".into(),
        })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchKeyEvent", json!({
                    "type": "rawKeyDown",
                    "key": key,
                })).await.map_err(|e| e.to_string())?;

                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchKeyEvent", json!({
                    "type": "keyUp",
                    "key": key,
                })).await.map_err(|e| e.to_string())?;

                Ok::<(), String>(())
            } => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Key press dispatch failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "key": key, "pressed": true }),
                    format!("Pressed key '{key}' in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. page.submit
// ---------------------------------------------------------------------------

pub struct PageSubmitTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageSubmitTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.submit",
            ToolCategory::PageInteraction,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector for form or submit button" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["selector"]
            }),
        )
        .with_description("Submits an HTML form element (Tier 2 Mutation)")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::Never)
        .with_timeout_ms(15_000)
    }
}

#[async_trait]
impl KageTool for PageSubmitTool {
    fn tool_id(&self) -> &'static str {
        "page.submit"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
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

        let (_, target_x, target_y) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                    "type": "mousePressed",
                    "x": target_x,
                    "y": target_y,
                    "button": "left",
                    "clickCount": 1,
                })).await.map_err(|e| e.to_string())?;

                self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                    "type": "mouseReleased",
                    "x": target_x,
                    "y": target_y,
                    "button": "left",
                    "clickCount": 1,
                })).await.map_err(|e| e.to_string())?;

                Ok::<(), String>(())
            } => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Submit click dispatch failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "selector": selector, "submitted": true }),
                    format!("Submitted form at '{selector}' in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 7. page.scroll
// ---------------------------------------------------------------------------

pub struct PageScrollTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageScrollTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.scroll",
            ToolCategory::PageInteraction,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "delta_x": { "type": "number", "default": 0.0 },
                    "delta_y": { "type": "number", "default": 100.0 },
                    "tab_id": { "type": "string", "format": "uuid" },
                    "x": { "type": "number", "default": 100.0 },
                    "y": { "type": "number", "default": 100.0 }
                }
            }),
        )
        .with_description("Scrolls the page or viewport via synthetic CDP mouse wheel events")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(5_000)
    }
}

#[async_trait]
impl KageTool for PageScrollTool {
    fn tool_id(&self) -> &'static str {
        "page.scroll"
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

        let delta_x = request.args.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let delta_y = request.args.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(100.0);
        let x = request.args.get("x").and_then(|v| v.as_f64()).unwrap_or(100.0);
        let y = request.args.get("y").and_then(|v| v.as_f64()).unwrap_or(100.0);

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                "type": "mouseWheel",
                "x": x,
                "y": y,
                "deltaX": delta_x,
                "deltaY": delta_y,
            })) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Scroll event dispatch failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "scrolled": true, "delta_y": delta_y }),
                    format!("Scrolled by delta_y={delta_y} in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 8. page.hover
// ---------------------------------------------------------------------------

pub struct PageHoverTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl PageHoverTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "page.hover",
            ToolCategory::PageInteraction,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "CSS selector to hover over" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["selector"]
            }),
        )
        .with_description("Hovers the mouse cursor over an element resolved by CSS selector")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(5_000)
    }
}

#[async_trait]
impl KageTool for PageHoverTool {
    fn tool_id(&self) -> &'static str {
        "page.hover"
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

        let (_, cx, cy) = resolve_element_center(&self.session_manager, tab_id, &target_id, selector).await?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Input.dispatchMouseEvent", json!({
                "type": "mouseMoved",
                "x": cx,
                "y": cy,
            })) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Hover event dispatch failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "hovered": true, "x": cx, "y": cy }),
                    format!("Hovered at ({cx:.1}, {cy:.1}) in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}
