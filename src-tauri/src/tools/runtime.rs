//! Extended Runtime inspection tools implementing Developer Intelligence (Phase 6).
//!
//! # Architecture & Contracts Enforced
//! - `INV-01`: AI/caller never accesses CEF directly.
//! - `INV-02`: Dispatched strictly through `ToolBus`.
//! - `INV-04`: Call function is `PermissionTier::StateMutating`; property reads are `ReadTelemetry`.
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
// 1. devtools.runtime.get_properties
// ============================================================================

/// Tool to inspect properties of a remote V8 object.
pub struct RuntimeGetPropertiesTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl RuntimeGetPropertiesTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for RuntimeGetPropertiesTool {
    fn tool_id(&self) -> &'static str {
        "devtools.runtime.get_properties"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "object_id": { "type": "string", "description": "Identifier of the remote object" },
                "own_properties": { "type": "boolean", "description": "Return own properties only" }
            },
            "required": ["tab_id", "object_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let object_id = request.args["object_id"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'object_id' string argument".into(),
        })?;

        let own_properties = request.args.get("own_properties").and_then(|v| v.as_bool()).unwrap_or(true);

        let params = json!({
            "objectId": object_id,
            "ownProperties": own_properties,
            "generatePreview": true,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Runtime.getProperties", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Runtime.getProperties failed: {e}").into(),
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
// 2. devtools.runtime.call_function
// ============================================================================

/// Tool to invoke a JavaScript function declaration on a remote object or global scope.
pub struct RuntimeCallFunctionTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl RuntimeCallFunctionTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for RuntimeCallFunctionTool {
    fn tool_id(&self) -> &'static str {
        "devtools.runtime.call_function"
    }

    fn tier(&self) -> PermissionTier {
        // High-impact / state-mutating capability
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "function_declaration": { "type": "string", "description": "Declaration of function to call" },
                "object_id": { "type": "string", "description": "Target remote object ID (optional)" },
                "arguments": { "type": "array", "description": "Call arguments (optional)" },
                "return_by_value": { "type": "boolean" },
                "await_promise": { "type": "boolean" }
            },
            "required": ["tab_id", "function_declaration"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let function_declaration = request.args["function_declaration"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'function_declaration' string argument".into(),
        })?;

        let object_id = request.args.get("object_id").and_then(|v| v.as_str());
        let arguments = request.args.get("arguments").cloned().unwrap_or_else(|| json!([]));
        let return_by_value = request.args.get("return_by_value").and_then(|v| v.as_bool()).unwrap_or(true);
        let await_promise = request.args.get("await_promise").and_then(|v| v.as_bool()).unwrap_or(true);

        let mut params = json!({
            "functionDeclaration": function_declaration,
            "arguments": arguments,
            "returnByValue": return_by_value,
            "awaitPromise": await_promise,
            "generatePreview": true,
        });

        if let Some(obj_id) = object_id {
            params["objectId"] = json!(obj_id);
        }

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Runtime.callFunctionOn", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Runtime.callFunctionOn failed: {e}").into(),
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
// 3. devtools.runtime.await_promise
// ============================================================================

/// Tool to explicitly await a remote JavaScript promise object.
pub struct RuntimeAwaitPromiseTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl RuntimeAwaitPromiseTool {
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self { tab_manager, session_manager }
    }
}

#[async_trait]
impl KageTool for RuntimeAwaitPromiseTool {
    fn tool_id(&self) -> &'static str {
        "devtools.runtime.await_promise"
    }

    fn tier(&self) -> PermissionTier {
        PermissionTier::ReadOnly
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "promise_object_id": { "type": "string", "description": "Remote object ID of the promise" },
                "return_by_value": { "type": "boolean" }
            },
            "required": ["tab_id", "promise_object_id"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();
        let (tab_id, _tab, target_id) = resolve_active_tab(&self.tab_manager, self.tool_id(), &request.args).await?;

        let promise_object_id = request.args["promise_object_id"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'promise_object_id' string argument".into(),
        })?;

        let return_by_value = request.args.get("return_by_value").and_then(|v| v.as_bool()).unwrap_or(true);

        let params = json!({
            "promiseObjectId": promise_object_id,
            "returnByValue": return_by_value,
            "generatePreview": true,
        });

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                Err(ToolError::Cancelled { request_id: request.request_id.clone() })
            }
            res = self.session_manager.call_cdp(tab_id, &target_id, "Runtime.awaitPromise", params) => {
                let output = res.map_err(|e| ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("CDP Runtime.awaitPromise failed: {e}").into(),
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
