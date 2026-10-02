//! Governed canonical `tab.*` tools.
//!
//! Replaces ad-hoc window operations with typed, profile-bound tab lifecycle capabilities:
//! - `tab.create`: Spawns a new tab bound to an explicit ProfileId (INV-10).
//! - `tab.close`: Closes the target tab and destroys its session.
//! - `tab.switch`: Switches active focus to target tab.
//! - `tab.focus`: Brings target tab and its surface to the foreground.
//! - `tab.list`: Lists all open tabs with their ProfileId and lifecycle status.
//! - `tab.get_state`: Queries full state of a single tab.

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use kage_browser::{ProfileId, TabId, TabManager};
use kage_core::policy::PermissionTier;
use kage_core::registry::{
    DeclarativeContract, IdempotencyClassification, RetryPolicy, ToolCategory, ToolMetadata,
};
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use crate::cdp_session::CdpSessionManager;

// ---------------------------------------------------------------------------
// 1. tab.create
// ---------------------------------------------------------------------------

pub struct TabCreateTool {
    tab_manager: Arc<TabManager>,
}

impl TabCreateTool {
    pub fn new(tab_manager: Arc<TabManager>) -> Self {
        Self { tab_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.create",
            ToolCategory::Tab,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "default": "about:blank" },
                    "profile_id": { "type": "string", "description": "Target profile partition (INV-10)" }
                }
            }),
        )
        .with_description("Creates a new browser tab with explicit ProfileId binding")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::Never)
        .with_contracts(vec![
            DeclarativeContract::TabCreated,
            DeclarativeContract::ProfileBound,
        ])
    }
}

#[async_trait]
impl KageTool for TabCreateTool {
    fn tool_id(&self) -> &'static str {
        "tab.create"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let url = request.args.get("url").and_then(|v| v.as_str()).unwrap_or("about:blank");
        let profile_id_str = request.args.get("profile_id").and_then(|v| v.as_str()).unwrap_or("personal");
        let profile_id = ProfileId(profile_id_str.to_string());

        let tab_id = self.tab_manager.create_tab(profile_id.clone(), url).await.map_err(|e| ToolError::ExecutionFailed {
            tool_id: self.tool_id().to_string(),
            source: format!("TabManager::create_tab failed: {e}").into(),
        })?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "tab_id": tab_id.to_string(),
                "profile_id": profile_id.to_string(),
                "url": url
            }),
            format!("Created tab {tab_id} in profile {profile_id}"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 2. tab.close
// ---------------------------------------------------------------------------

pub struct TabCloseTool {
    tab_manager: Arc<TabManager>,
}

impl TabCloseTool {
    pub fn new(tab_manager: Arc<TabManager>) -> Self {
        Self { tab_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.close",
            ToolCategory::Tab,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["tab_id"]
            }),
        )
        .with_description("Closes the specified tab and destroys its sessions")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for TabCloseTool {
    fn tool_id(&self) -> &'static str {
        "tab.close"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let tab_id_str = request.args.get("tab_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'tab_id' argument".into(),
        })?;

        let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: format!("'tab_id' is not a valid UUID: {e}"),
        })?;
        let tab_id = TabId(tab_uuid);

        self.tab_manager.close_tab(tab_id).await.map_err(|e| ToolError::ExecutionFailed {
            tool_id: self.tool_id().to_string(),
            source: format!("TabManager::close_tab failed: {e}").into(),
        })?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({ "tab_id": tab_id.to_string(), "closed": true }),
            format!("Closed tab {tab_id}"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 3. tab.switch
// ---------------------------------------------------------------------------

pub struct TabSwitchTool {
    tab_manager: Arc<TabManager>,
}

impl TabSwitchTool {
    pub fn new(tab_manager: Arc<TabManager>) -> Self {
        Self { tab_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.switch",
            ToolCategory::Tab,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["tab_id"]
            }),
        )
        .with_description("Switches the active tab in the browser control plane")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for TabSwitchTool {
    fn tool_id(&self) -> &'static str {
        "tab.switch"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let tab_id_str = request.args.get("tab_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'tab_id' argument".into(),
        })?;

        let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: format!("'tab_id' is not a valid UUID: {e}"),
        })?;
        let tab_id = TabId(tab_uuid);

        self.tab_manager.switch_tab(tab_id).await.map_err(|e| ToolError::ExecutionFailed {
            tool_id: self.tool_id().to_string(),
            source: format!("TabManager::switch_tab failed: {e}").into(),
        })?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({ "tab_id": tab_id.to_string(), "switched": true }),
            format!("Switched to tab {tab_id}"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 4. tab.focus
// ---------------------------------------------------------------------------

pub struct TabFocusTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl TabFocusTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.focus",
            ToolCategory::Tab,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["tab_id"]
            }),
        )
        .with_description("Brings the target tab to front and focuses its window surface")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for TabFocusTool {
    fn tool_id(&self) -> &'static str {
        "tab.focus"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let tab_id_str = request.args.get("tab_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'tab_id' argument".into(),
        })?;

        let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: format!("'tab_id' is not a valid UUID: {e}"),
        })?;
        let tab_id = TabId(tab_uuid);

        self.tab_manager.switch_tab(tab_id).await.map_err(|e| ToolError::ExecutionFailed {
            tool_id: self.tool_id().to_string(),
            source: format!("TabManager::switch_tab failed: {e}").into(),
        })?;

        let tab = self.tab_manager.get_tab(tab_id).await.map_err(|_| ToolError::NotFound {
            id: format!("Tab {tab_id} does not exist"),
        })?;

        let target_id = {
            let guard = tab.cdp.read().await;
            guard.as_ref().map(|c| c.target_id.clone())
        };

        if let Some(ref target) = target_id {
            let _ = self.session_manager.call_cdp(tab_id, target, "Page.bringToFront", json!({})).await;
        }

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({ "tab_id": tab_id.to_string(), "focused": true }),
            format!("Focused tab {tab_id}"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 5. tab.list
// ---------------------------------------------------------------------------

pub struct TabListTool {
    tab_manager: Arc<TabManager>,
}

impl TabListTool {
    pub fn new(tab_manager: Arc<TabManager>) -> Self {
        Self { tab_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.list",
            ToolCategory::Tab,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {}
            }),
        )
        .with_description("Lists all open browser tabs with their explicit ProfileId binding")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for TabListTool {
    fn tool_id(&self) -> &'static str {
        "tab.list"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let tabs = self.tab_manager.list_tabs().await;
        let active_tab = self.tab_manager.get_active_tab().await;

        let tab_summaries: Vec<serde_json::Value> = tabs
            .into_iter()
            .map(|t| {
                json!({
                    "tab_id": t.id().to_string(),
                    "profile_id": t.profile_id().to_string(),
                    "url": t.url,
                    "title": t.title,
                    "lifecycle": format!("{:?}", t.lifecycle),
                    "health": format!("{:?}", t.health),
                    "is_active": Some(t.id()) == active_tab
                })
            })
            .collect();

        let total = tab_summaries.len();
        let elapsed_ms = started.elapsed().as_millis() as u64;

        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({ "tabs": tab_summaries, "total": total }),
            format!("Retrieved {total} open tabs"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 6. tab.get_state
// ---------------------------------------------------------------------------

pub struct TabGetStateTool {
    tab_manager: Arc<TabManager>,
}

impl TabGetStateTool {
    pub fn new(tab_manager: Arc<TabManager>) -> Self {
        Self { tab_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "tab.get_state",
            ToolCategory::Tab,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["tab_id"]
            }),
        )
        .with_description("Queries detailed state of a single tab")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for TabGetStateTool {
    fn tool_id(&self) -> &'static str {
        "tab.get_state"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        Self::metadata().schema
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled { request_id: request.request_id.clone() });
        }

        let tab_id_str = request.args.get("tab_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'tab_id' argument".into(),
        })?;

        let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: format!("'tab_id' is not a valid UUID: {e}"),
        })?;
        let tab_id = TabId(tab_uuid);

        let tab = self.tab_manager.get_tab(tab_id).await.map_err(|_| ToolError::NotFound {
            id: format!("Tab {tab_id} does not exist"),
        })?;

        let url = tab.url.read().await.clone();
        let title = tab.title.read().await.clone();
        let lifecycle = tab.lifecycle.read().await.clone();
        let health = tab.health.read().await.clone();
        let can_go_back = tab.can_go_back.load(std::sync::atomic::Ordering::Relaxed);
        let can_go_forward = tab.can_go_forward.load(std::sync::atomic::Ordering::Relaxed);

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "tab_id": tab_id.to_string(),
                "profile_id": tab.profile_id.to_string(),
                "url": url,
                "title": title,
                "lifecycle": format!("{lifecycle:?}"),
                "health": format!("{health:?}"),
                "can_go_back": can_go_back,
                "can_go_forward": can_go_forward
            }),
            format!("Retrieved state for tab {tab_id}"),
            elapsed_ms,
        ))
    }
}
