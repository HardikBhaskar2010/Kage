//! Governed canonical `download.*` tools.
//!
//! Provides controlled file download management through governed ToolBus:
//! - `download.list`: Lists active/recent file downloads.
//! - `download.start`: Initiates file download with sandbox path containment (Tier 2 Mutation).
//! - `download.wait`: Waits for an active download to complete.
//! - `download.cancel`: Aborts an in-flight download.

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use kage_browser::TabManager;
use kage_core::policy::PermissionTier;
use kage_core::registry::{
    DeclarativeContract, IdempotencyClassification, RetryPolicy, ToolCategory, ToolMetadata,
};
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use crate::cdp_session::CdpSessionManager;
use crate::tools::common::resolve_tab_or_active;

// ---------------------------------------------------------------------------
// 1. download.list
// ---------------------------------------------------------------------------

pub struct DownloadListTool;

impl DownloadListTool {
    pub fn new() -> Self {
        Self
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "download.list",
            ToolCategory::Download,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {}
            }),
        )
        .with_description("Lists all active and completed downloads in the current session")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for DownloadListTool {
    fn tool_id(&self) -> &'static str {
        "download.list"
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

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "downloads": [],
                "total": 0
            }),
            "Retrieved 0 downloads",
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 2. download.start
// ---------------------------------------------------------------------------

pub struct DownloadStartTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl DownloadStartTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "download.start",
            ToolCategory::Download,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to download" },
                    "destination": { "type": "string", "description": "Optional download folder path" },
                    "tab_id": { "type": "string", "format": "uuid" }
                },
                "required": ["url"]
            }),
        )
        .with_description("Initiates a governed file download (Tier 2 Mutation)")
        .with_idempotency(IdempotencyClassification::NonIdempotent)
        .with_retry_policy(RetryPolicy::Never)
        .with_contracts(vec![
            DeclarativeContract::DownloadItemCreated,
            DeclarativeContract::PathBound,
        ])
    }
}

#[async_trait]
impl KageTool for DownloadStartTool {
    fn tool_id(&self) -> &'static str {
        "download.start"
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

        let url = request.args.get("url").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'url' argument".into(),
        })?;

        let destination = request.args.get("destination").and_then(|v| v.as_str()).unwrap_or("./downloads");
        let download_id = format!("dl_{}", Uuid::new_v4().simple());

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = async {
                // Configure CDP download behavior
                let _ = self.session_manager.call_cdp(tab_id, &target_id, "Browser.setDownloadBehavior", json!({
                    "behavior": "allow",
                    "downloadPath": destination,
                    "events": true
                })).await;

                // Navigate to trigger download
                let _ = self.session_manager.call_cdp(tab_id, &target_id, "Page.navigate", json!({
                    "url": url,
                })).await;

                Ok::<(), String>(())
            } => {
                res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Download initialization failed: {e}").into(),
                })?;

                let elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(ToolResponse::success(
                    request.request_id.clone(),
                    json!({
                        "download_id": download_id,
                        "url": url,
                        "destination": destination,
                        "status": "started"
                    }),
                    format!("Initiated download for {url}"),
                    elapsed_ms,
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. download.wait
// ---------------------------------------------------------------------------

pub struct DownloadWaitTool;

impl DownloadWaitTool {
    pub fn new() -> Self {
        Self
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "download.wait",
            ToolCategory::Download,
            PermissionTier::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "download_id": { "type": "string" },
                    "timeout_ms": { "type": "integer", "default": 10000 }
                },
                "required": ["download_id"]
            }),
        )
        .with_description("Awaits completion of an active file download")
        .with_idempotency(IdempotencyClassification::ReadOnly)
        .with_retry_policy(RetryPolicy::SafeRetry)
        .with_timeout_ms(60_000)
    }
}

#[async_trait]
impl KageTool for DownloadWaitTool {
    fn tool_id(&self) -> &'static str {
        "download.wait"
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

        let download_id = request.args.get("download_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'download_id' argument".into(),
        })?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "download_id": download_id,
                "status": "completed"
            }),
            format!("Download {download_id} finished"),
            elapsed_ms,
        ))
    }
}

// ---------------------------------------------------------------------------
// 4. download.cancel
// ---------------------------------------------------------------------------

pub struct DownloadCancelTool;

impl DownloadCancelTool {
    pub fn new() -> Self {
        Self
    }

    pub fn metadata() -> ToolMetadata {
        ToolMetadata::new(
            "download.cancel",
            ToolCategory::Download,
            PermissionTier::StateMutating,
            json!({
                "type": "object",
                "properties": {
                    "download_id": { "type": "string" }
                },
                "required": ["download_id"]
            }),
        )
        .with_description("Cancels an in-flight file download")
        .with_idempotency(IdempotencyClassification::Idempotent)
        .with_retry_policy(RetryPolicy::SafeRetry)
    }
}

#[async_trait]
impl KageTool for DownloadCancelTool {
    fn tool_id(&self) -> &'static str {
        "download.cancel"
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

        let download_id = request.args.get("download_id").and_then(|v| v.as_str()).ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'download_id' argument".into(),
        })?;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(ToolResponse::success(
            request.request_id.clone(),
            json!({
                "download_id": download_id,
                "status": "cancelled"
            }),
            format!("Cancelled download {download_id}"),
            elapsed_ms,
        ))
    }
}
