//! Tool vocabulary: [`KageTool`] trait and associated request/response/error types.
//!
//! Every browser action available to the AI subsystem must be implemented as a
//! `KageTool`. The tool is *registered* with the [`ToolBus`] and *dispatched* by it —
//! the AI caller never invokes tool methods directly.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::lineage::ExecutionStatus;
use crate::policy::PermissionTier;

// ---------------------------------------------------------------------------
// Core request / response envelope
// ---------------------------------------------------------------------------

/// Typed payload envelope for every Tool Bus dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Stable tool identifier (e.g. `"dom.read"`, `"browser.navigate"`).
    pub tool_id: String,
    /// Arbitrary JSON arguments validated against the tool's registered schema.
    pub args: serde_json::Value,
    /// Unique correlation ID for audit linkage and cancellation.
    pub request_id: String,
    /// Human-readable reason provided by the AI subsystem (stored in audit log).
    pub reason: String,

    /// Optional Agent Task Scope ID (Phase 8 Lineage).
    #[serde(default)]
    pub task_id: Option<String>,
    /// Optional Plan Step Scope ID (Phase 8 Lineage).
    #[serde(default)]
    pub step_id: Option<String>,
    /// Optional assigned or inherited execution instance ID.
    #[serde(default)]
    pub execution_id: Option<String>,
}

impl ToolRequest {
    /// Construct a basic tool request.
    pub fn new(
        tool_id: impl Into<String>,
        args: serde_json::Value,
        request_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            tool_id: tool_id.into(),
            args,
            request_id: request_id.into(),
            reason: reason.into(),
            task_id: None,
            step_id: None,
            execution_id: None,
        }
    }

    /// Attach agent execution lineage to the request.
    pub fn with_lineage(
        mut self,
        task_id: impl Into<Option<String>>,
        step_id: impl Into<Option<String>>,
    ) -> Self {
        self.task_id = task_id.into();
        self.step_id = step_id.into();
        self
    }
}

/// Rich, layered result model from a Tool Bus dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    /// Mirrors [`ToolRequest::request_id`].
    pub request_id: String,
    /// Tool output. Must never contain unredacted secrets.
    pub output: serde_json::Value,
    /// Elapsed wall-clock time for the operation in milliseconds.
    pub elapsed_ms: u64,

    /// Execution instance UUID.
    #[serde(default = "generate_execution_id")]
    pub execution_id: String,
    /// Correlated agent task ID.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Correlated plan step ID.
    #[serde(default)]
    pub step_id: Option<String>,
    /// Final execution status.
    #[serde(default)]
    pub status: ExecutionStatus,
    /// Timestamp when tool execution commenced.
    #[serde(default)]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp when tool execution finished.
    #[serde(default)]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// High-level summary of action effect (e.g. "Navigated to https://example.com").
    #[serde(default)]
    pub effect_summary: Option<String>,
    /// Error description if execution failed.
    #[serde(default)]
    pub failure: Option<String>,
    /// Associated tab or provenance identity.
    #[serde(default)]
    pub identity: Option<serde_json::Value>,
}

fn generate_execution_id() -> String {
    Uuid::new_v4().to_string()
}

impl ToolResponse {
    /// Construct a basic successful tool response (backward-compatible).
    pub fn new(request_id: impl Into<String>, output: serde_json::Value, elapsed_ms: u64) -> Self {
        let now = chrono::Utc::now();
        Self {
            request_id: request_id.into(),
            output,
            elapsed_ms,
            execution_id: generate_execution_id(),
            task_id: None,
            step_id: None,
            status: ExecutionStatus::Success,
            started_at: Some(now),
            completed_at: Some(now),
            effect_summary: None,
            failure: None,
            identity: None,
        }
    }

    /// Construct a successful response with explicit effect summary.
    pub fn success(
        request_id: impl Into<String>,
        output: serde_json::Value,
        effect: impl Into<String>,
        elapsed_ms: u64,
    ) -> Self {
        let mut resp = Self::new(request_id, output, elapsed_ms);
        resp.effect_summary = Some(effect.into());
        resp
    }

    /// Construct a failure response.
    pub fn failure(
        request_id: impl Into<String>,
        error_msg: impl Into<String>,
        elapsed_ms: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        let err = error_msg.into();
        Self {
            request_id: request_id.into(),
            output: serde_json::json!({ "error": err }),
            elapsed_ms,
            execution_id: generate_execution_id(),
            task_id: None,
            step_id: None,
            status: ExecutionStatus::Failed,
            started_at: Some(now),
            completed_at: Some(now),
            effect_summary: None,
            failure: Some(err),
            identity: None,
        }
    }

    /// Populate lineage from request.
    pub fn populate_lineage_from_request(&mut self, request: &ToolRequest) {
        if self.task_id.is_none() {
            self.task_id = request.task_id.clone();
        }
        if self.step_id.is_none() {
            self.step_id = request.step_id.clone();
        }
        if let Some(ref exec_id) = request.execution_id {
            self.execution_id = exec_id.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Error hierarchy
// ---------------------------------------------------------------------------

/// All errors that can originate from tool execution or bus governance.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool '{id}' not registered with the ToolBus")]
    NotFound { id: String },

    #[error("schema validation failed for '{tool_id}': {reason}")]
    SchemaViolation { tool_id: String, reason: String },

    #[error("permission denied — tier {required:?} not granted for '{tool_id}' (decision: {decision})")]
    PermissionDenied {
        tool_id: String,
        required: PermissionTier,
        decision: String,
    },

    #[error("developer tool '{tool_id}' is prohibited from generic agent execution (eval_js prohibition)")]
    DeveloperToolProhibited { tool_id: String },

    #[error("tool '{tool_id}' execution failed: {source}")]
    ExecutionFailed {
        tool_id: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("tool call cancelled by token for request '{request_id}'")]
    Cancelled { request_id: String },

    #[error("tool execution timed out after {timeout_ms}ms")]
    Timeout { tool_id: String, timeout_ms: u64 },

    #[error("audit write failed: {0}")]
    AuditFailure(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// KageTool trait
// ---------------------------------------------------------------------------

/// The trait every browser action must implement to be registered with the [`ToolBus`].
///
/// # Implementation contract
///
/// * `tool_id()` — must be unique across the registry and stable across versions.
/// * `tier()` — must reflect the *minimum* permission tier required to execute.
/// * `schema()` — must be a valid JSON Schema draft-7 object used to validate args.
/// * `execute()` — must **never** call CEF, CDP, or filesystem APIs directly.
///   All platform I/O must be injected via the `ctx` argument.
#[async_trait]
pub trait KageTool: Send + Sync + 'static {
    /// Globally unique, dot-namespaced identifier (e.g. `"dom.read_node"`, `"browser.navigate"`).
    fn tool_id(&self) -> &'static str;

    /// Minimum permission tier that must be granted before execution proceeds.
    fn tier(&self) -> PermissionTier;

    /// JSON Schema (draft-7) describing the valid shape of [`ToolRequest::args`].
    fn schema(&self) -> serde_json::Value;

    /// Execute the tool given the validated, policy-cleared request.
    ///
    /// The implementation must honour the `cancel` token and return
    /// [`ToolError::Cancelled`] promptly when it fires.
    async fn execute(
        &self,
        request: &ToolRequest,
        cancel: CancellationToken,
    ) -> Result<ToolResponse, ToolError>;
}
