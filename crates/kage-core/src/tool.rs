//! Tool vocabulary: [`KageTool`] trait and associated request/response/error types.
//!
//! Every browser action available to the AI subsystem must be implemented as a
//! `KageTool`. The tool is *registered* with the [`ToolBus`] and *dispatched* by it —
//! the AI caller never invokes tool methods directly.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::policy::PermissionTier;

// ---------------------------------------------------------------------------
// Core request / response envelope
// ---------------------------------------------------------------------------

/// Typed payload envelope for every Tool Bus dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Stable tool identifier (e.g. `"dom.read"`, `"network.replay"`).
    pub tool_id: String,
    /// Arbitrary JSON arguments validated against the tool's registered schema.
    pub args: serde_json::Value,
    /// Unique correlation ID for audit linkage and cancellation.
    pub request_id: String,
    /// Human-readable reason provided by the AI subsystem (stored in audit log).
    pub reason: String,
}

/// Successful result from a Tool Bus dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    /// Mirrors [`ToolRequest::request_id`].
    pub request_id: String,
    /// Tool output. Must never contain unredacted secrets.
    pub output: serde_json::Value,
    /// Elapsed wall-clock time for the operation in milliseconds.
    pub elapsed_ms: u64,
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

    #[error("tool '{tool_id}' execution failed: {source}")]
    ExecutionFailed {
        tool_id: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("tool call cancelled by token for request '{request_id}'")]
    Cancelled { request_id: String },

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
    /// Globally unique, dot-namespaced identifier (e.g. `"dom.read_node"`).
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
