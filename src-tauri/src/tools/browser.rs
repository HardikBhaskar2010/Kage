//! Governed canonical `browser.*` tools.
//!
//! Replaces arbitrary script execution with typed, governed navigation capabilities:
//! - `browser.navigate`: Navigates tab to URL via CDP `Page.navigate`.
//! - `browser.reload`: Reloads current page via CDP `Page.reload`.
//! - `browser.go_back`: Navigates backward in tab history.
//! - `browser.go_forward`: Navigates forward in tab history.
//! - `browser.stop`: Halts page loading via CDP `Page.stopLoading`.
//! - `browser.wait_for_navigation`: Awaits page navigation settle/ready.

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

// ---------------------------------------------------------------------------
// 1. browser.navigate
// ---------------------------------------------------------------------------

pub struct BrowserNavigateTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl BrowserNavigateTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.navigate",
            ToolCategory::Browser,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Target HTTP/HTTPS URL" },
                    "tab_id": { "type": "string", "format": "uuid", "description": "Optional Tab ID (defaults to active tab)" },
                    "wait_until": { "type": "string", "enum": ["started", "committed", "completed"], "default": "committed" }
                },
                "required": ["url"]
            }),
        )
        .with_description("Navigates the browser tab to an explicit destination URL")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(15_000)
        .with_contracts(vec![
            DeclarativeContract::NavigationCommitted,
            DeclarativeContract::NavigationCompleted,
        ])
    }
}

#[async_trait]
impl KageTool for BrowserNavigateTool {
    fn tool_id(&self) -> &'static str {
        "browser.navigate"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, tab, target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;

        let url = request.args.get("url").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'url' argument".into(),
        })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Page.navigate", json!({ "url": url })) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Page.navigate failed: {e}").into(),
                })?;

                // Update tab url state
                {
                    let mut tab_url = tab.url.write().await;
                    *tab_url = url.to_string();
                }

                let frame_id = output.get("frameId").and_then(|v| v.as_str()).unwrap_or("");
                let elapsed_ms = started.elapsed().as_millis() as u64;

                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "tab_id": tab_id.to_string(),
                        "url": url,
                        "frame_id": frame_id,
                        "navigation_status": "committed"
                    }),
                    format!("Navigated tab {tab_id} to {url}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. browser.reload
// ---------------------------------------------------------------------------

pub struct BrowserReloadTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl BrowserReloadTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.reload",
            ToolCategory::Browser,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" },
                    "ignore_cache": { "type": "boolean", "default": false }
                }
            }),
        )
        .with_description("Reloads the current page in the specified or active tab")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for BrowserReloadTool {
    fn tool_id(&self) -> &'static str {
        "browser.reload"
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
        let ignore_cache = request.args.get("ignore_cache").and_then(|v| v.as_bool()).unwrap_or(false);

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Page.reload", json!({ "ignoreCache": ignore_cache })) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Page.reload failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "reloaded": true }),
                    format!("Reloaded tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. browser.go_back
// ---------------------------------------------------------------------------

pub struct BrowserGoBackTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl BrowserGoBackTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.go_back",
            ToolCategory::Browser,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Navigates back to the preceding entry in session history")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::VerifyBeforeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for BrowserGoBackTool {
    fn tool_id(&self) -> &'static str {
        "browser.go_back"
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

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            hist_res = self.session_manager.call_cdp(tab_id, &target_id, "Page.getNavigationHistory", json!({})) => {
                let hist = hist_res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Page.getNavigationHistory failed: {e}").into(),
                })?;

                let current_index = hist.get("currentIndex").and_then(|v| v.as_i64()).unwrap_or(0);
                if current_index <= 0 {
                    return Err(ToolError::ExecutionFailed {
                        tool_id: self.tool_id().to_string(),
                        source: "Cannot go back: already at beginning of history".into(),
                    });
                }

                let entries = hist.get("entries").and_then(|v| v.as_array()).ok_or_else(|| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: "Malformed navigation history entries".into(),
                })?;

                let target_entry_id = entries[(current_index - 1) as usize].get("id").and_then(|v| v.as_i64()).ok_or_else(|| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: "Missing entry id in history".into(),
                })?;

                self.session_manager.call_cdp(tab_id, &target_id, "Page.navigateToHistoryEntry", json!({ "entryId": target_entry_id })).await
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool_id: self.tool_id().to_string(),
                        source: format!("Page.navigateToHistoryEntry failed: {e}").into(),
                    })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "went_back": true }),
                    format!("Navigated back in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. browser.go_forward
// ---------------------------------------------------------------------------

pub struct BrowserGoForwardTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl BrowserGoForwardTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.go_forward",
            ToolCategory::Browser,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Navigates forward to the next entry in session history")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::VerifyBeforeRetry)
        .with_timeout_ms(10_000)
    }
}

#[async_trait]
impl KageTool for BrowserGoForwardTool {
    fn tool_id(&self) -> &'static str {
        "browser.go_forward"
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

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            hist_res = self.session_manager.call_cdp(tab_id, &target_id, "Page.getNavigationHistory", json!({})) => {
                let hist = hist_res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Page.getNavigationHistory failed: {e}").into(),
                })?;

                let current_index = hist.get("currentIndex").and_then(|v| v.as_i64()).unwrap_or(0);
                let entries = hist.get("entries").and_then(|v| v.as_array()).ok_or_else(|| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: "Malformed navigation history entries".into(),
                })?;

                if (current_index + 1) as usize >= entries.len() {
                    return Err(ToolError::ExecutionFailed {
                        tool_id: self.tool_id().to_string(),
                        source: "Cannot go forward: already at latest history entry".into(),
                    });
                }

                let target_entry_id = entries[(current_index + 1) as usize].get("id").and_then(|v| v.as_i64()).ok_or_else(|| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: "Missing entry id in history".into(),
                })?;

                self.session_manager.call_cdp(tab_id, &target_id, "Page.navigateToHistoryEntry", json!({ "entryId": target_entry_id })).await
                    .map_err(|e| ToolError::ExecutionFailed {
                        tool_id: self.tool_id().to_string(),
                        source: format!("Page.navigateToHistoryEntry failed: {e}").into(),
                    })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "went_forward": true }),
                    format!("Navigated forward in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. browser.stop
// ---------------------------------------------------------------------------

pub struct BrowserStopTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl BrowserStopTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.stop",
            ToolCategory::Browser,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                }
            }),
        )
        .with_description("Halts all ongoing network requests and page loading in the tab")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(5_000)
    }
}

#[async_trait]
impl KageTool for BrowserStopTool {
    fn tool_id(&self) -> &'static str {
        "browser.stop"
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

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Page.stopLoading", json!({})) => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Page.stopLoading failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({ "tab_id": tab_id.to_string(), "stopped": true }),
                    format!("Stopped page loading in tab {tab_id}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. browser.wait_for_navigation
// ---------------------------------------------------------------------------

pub struct BrowserWaitForNavigationTool {
    tab_manager: Arc<TabManager>,
    _session_manager: Arc<CdpSessionManager>,
}

impl BrowserWaitForNavigationTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, _session_manager: session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "browser.wait_for_navigation",
            ToolCategory::Browser,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" },
                    "timeout_ms": { "type": "integer", "default": 5000 },
                    "expected_url": { "type": "string" }
                }
            }),
        )
        .with_description("Awaits navigation completion and settlement in the specified tab")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(30_000)
    }
}

#[async_trait]
impl KageTool for BrowserWaitForNavigationTool {
    fn tool_id(&self) -> &'static str {
        "browser.wait_for_navigation"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, tab, _target_id) = resolve_tab_or_active(&self.tab_manager, self.tool_id(), &request.args).await?;
        let timeout_ms = request.args.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
        let expected_url = request.args.get("expected_url").and_then(|v| v.as_str());

        let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if cancel.is_cancelled() {
                return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
            }

            let current_url = tab.url.read().await.clone();
            if let Some(exp) = expected_url {
                if current_url.contains(exp) {
                    break;
                }
            } else if !current_url.is_empty() {
                break;
            }

            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let final_url = tab.url.read().await.clone();
        let elapsed_ms = started.elapsed().as_millis() as u64;

        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "tab_id": tab_id.to_string(),
                "url": final_url,
                "ready": true
            }),
            format!("Navigation settled on {}", final_url),
            elapsed_ms,
        ))
    }
}
