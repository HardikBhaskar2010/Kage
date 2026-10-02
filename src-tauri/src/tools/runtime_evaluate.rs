//! Concrete implementation of `devtools.runtime.evaluate` as a governed `KageTool`.
//!
//! # Architecture & Contracts Enforced
//! - `INV-01`: AI/caller never accesses CEF directly.
//! - `INV-02`: Dispatched strictly through `ToolBus`.
//! - `INV-04`: Privileged capability adjudication via `PolicyEngine`.
//! - `INV-05`: Pre-execution audit commitment (Two-stage fail-closed).
//! - `INV-06`: Evaluated output sanitized by `SecretSanitizer` before reaching caller.
//! - `INV-09`: Stops awaiting result immediately upon cancellation token.
//! - `INV-10`/`INV-12`: Resolves and validates explicit browser identity (`TabId` -> `TargetId` -> `SessionId`).

use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use kage_browser::{TabHealth, TabId, TabLifecycle, TabManager};
use kage_core::policy::PermissionTier;
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::cdp_session::CdpSessionManager;

/// Governed Developer Plane tool evaluating JavaScript expressions in Chromium V8 via CDP.
pub struct RuntimeEvaluateTool {
    tab_manager: Arc<TabManager>,
    session_manager: Arc<CdpSessionManager>,
}

impl RuntimeEvaluateTool {
    /// Create a new `RuntimeEvaluateTool` bound to the host `TabManager` and `CdpSessionManager`.
    pub fn new(tab_manager: Arc<TabManager>, session_manager: Arc<CdpSessionManager>) -> Self {
        Self {
            tab_manager,
            session_manager,
        }
    }
}

#[async_trait]
impl KageTool for RuntimeEvaluateTool {
    fn tool_id(&self) -> &'static str {
        "devtools.runtime.evaluate"
    }

    fn tier(&self) -> PermissionTier {
        // High-impact / state-mutating capability. Always authorization-gated and audited (INV-04/05).
        PermissionTier::StateMutating
    }

    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "expression": { "type": "string", "minLength": 1 },
                "return_by_value": { "type": "boolean" },
                "await_promise": { "type": "boolean" }
            },
            "required": ["tab_id", "expression"]
        })
    }

    async fn execute(&self, request: &ToolRequest, cancel: CancellationToken) -> Result<ToolResponse, ToolError> {
        let started = Instant::now();

        // 1. Argument parsing & validation
        let tab_id_str = request.args["tab_id"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'tab_id' string argument".into(),
        })?;

        let tab_uuid = Uuid::parse_str(tab_id_str).map_err(|e| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: format!("'tab_id' is not a valid UUID: {e}"),
        })?;
        let tab_id = TabId(tab_uuid);

        let expression = request.args["expression"].as_str().ok_or_else(|| ToolError::SchemaViolation {
            tool_id: self.tool_id().to_string(),
            reason: "Missing 'expression' string argument".into(),
        })?;

        let return_by_value = request.args.get("return_by_value").and_then(|v| v.as_bool()).unwrap_or(true);
        let await_promise = request.args.get("await_promise").and_then(|v| v.as_bool()).unwrap_or(true);

        // 2. Strict Identity & Lifecycle Resolution (INV-10, INV-12)
        let tab = self.tab_manager.get_tab(tab_id).await.map_err(|_| ToolError::NotFound {
            id: format!("Tab {tab_id} does not exist"),
        })?;

        // 3. Health & Lifecycle Validation (INV-11A)
        let lifecycle = tab.lifecycle.read().await.clone();
        if lifecycle == TabLifecycle::Closing || lifecycle == TabLifecycle::Closed {
            return Err(ToolError::ExecutionFailed {
                tool_id: self.tool_id().to_string(),
                source: format!("Tab {tab_id} is in non-executable state: {lifecycle:?}").into(),
            });
        }

        let health = tab.health.read().await.clone();
        match health {
            TabHealth::Healthy => {}
            TabHealth::Recovering { recovery_epoch } => {
                return Err(ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Tab {tab_id} is recovering (epoch: {recovery_epoch}) - CDP execution unavailable").into(),
                });
            }
            TabHealth::RendererTerminated { status, .. } => {
                return Err(ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Renderer terminated (status: {status:?}) - fail-closed (INV-11A)").into(),
                });
            }
            TabHealth::Unresponsive => {
                return Err(ToolError::ExecutionFailed {
                    tool_id: self.tool_id().to_string(),
                    source: format!("Tab {tab_id} is unresponsive - CDP execution unavailable").into(),
                });
            }
        }

        let cef_browser_id = {
            let ident = tab.identity.read().await;
            ident.cef_browser_id.ok_or_else(|| ToolError::ExecutionFailed {
                tool_id: self.tool_id().to_string(),
                source: format!("Tab {tab_id} has no native CEF browser instance assigned").into(),
            })?
        };

        // 4. Resolve Attached CDP Target ID
        let target_id = {
            let cdp_guard = tab.cdp.read().await;
            cdp_guard.as_ref().map(|c| c.target_id.clone()).ok_or_else(|| ToolError::ExecutionFailed {
                tool_id: self.tool_id().to_string(),
                source: format!("Tab {tab_id} (CEF browser {cef_browser_id}) has no bound CDP target session (INV-12)").into(),
            })?
        };

        // 5. Execute via Persistent CdpSessionManager with Cancellation Protection (INV-09)
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::warn!(request_id = %request.request_id, "RuntimeEvaluateTool execution cancelled by token (INV-09)");
                Err(ToolError::Cancelled {
                    request_id: request.request_id.clone(),
                })
            }
            eval_result = self.session_manager.evaluate(
                tab_id,
                &target_id,
                expression,
                return_by_value,
                await_promise,
            ) => {
                match eval_result {
                    Ok(cdp_output) => {
                        let elapsed_ms = started.elapsed().as_millis() as u64;
                        tracing::debug!(
                            request_id = %request.request_id,
                            tab_id = %tab_id,
                            target_id = %target_id,
                            elapsed_ms,
                            "CDP Runtime.evaluate completed successfully"
                        );

                        Ok(ToolResponse {
                            request_id: request.request_id.clone(),
                            output: cdp_output,
                            elapsed_ms,
                        })
                    }
                    Err(e) => {
                        tracing::error!(
                            request_id = %request.request_id,
                            tab_id = %tab_id,
                            target_id = %target_id,
                            error = %e,
                            "CDP Runtime.evaluate protocol execution failed"
                        );
                        Err(ToolError::ExecutionFailed {
                            tool_id: self.tool_id().to_string(),
                            source: Box::new(e),
                        })
                    }
                }
            }
        }
    }
}
