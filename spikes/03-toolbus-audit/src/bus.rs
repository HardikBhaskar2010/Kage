use crate::audit::AuditLogger;
use crate::policy::{CallerContext, PermissionDecision, PolicyEngine};
use crate::sanitizer::SecretSanitizer;
use crate::tool::{validate_schema, KageTool, ToolError, ToolRegistry};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub struct ToolBus {
    registry: ToolRegistry,
    policy: PolicyEngine,
    audit: Arc<AuditLogger>,
}

impl ToolBus {
    pub fn new(audit: Arc<AuditLogger>) -> Self {
        Self {
            registry: ToolRegistry::new(),
            policy: PolicyEngine::new(),
            audit,
        }
    }

    pub fn register_tool(&mut self, tool: Arc<dyn KageTool>) {
        self.registry.register(tool);
    }

    pub fn policy_engine(&self) -> &PolicyEngine {
        &self.policy
    }

    pub fn audit_logger(&self) -> Arc<AuditLogger> {
        self.audit.clone()
    }

    pub async fn dispatch(
        &self,
        tool_name: &str,
        raw_args: Value,
        caller: &CallerContext,
        approval_token: Option<&str>,
        cancel: CancellationToken,
    ) -> Result<Value, ToolError> {
        let start_time = Instant::now();

        // 1. Locate Tool
        let tool = self.registry.get(tool_name).ok_or_else(|| {
            let err = ToolError::NotFound(tool_name.to_string());
            let _ = self.audit.record(
                tool_name,
                0,
                &caller.caller_id,
                "{}",
                "NOT_FOUND",
                start_time.elapsed().as_millis() as i64,
            );
            err
        })?;

        // 2. Schema Validation (prior to execution or sensitive processing)
        if let Err(reason) = validate_schema(&tool.schema(), &raw_args) {
            let sanitized_args = SecretSanitizer::sanitize_value(&raw_args);
            let sanitized_str = serde_json::to_string(&sanitized_args).unwrap_or_default();
            let _ = self.audit.record(
                tool_name,
                tool.tier() as i32,
                &caller.caller_id,
                &sanitized_str,
                "VALIDATION_FAILED",
                start_time.elapsed().as_millis() as i64,
            );
            return Err(ToolError::ValidationFailed {
                tool: tool_name.to_string(),
                reason,
            });
        }

        // 3. Deep Recursive Secret Redaction on arguments before auditing & dispatching
        let sanitized_args = SecretSanitizer::sanitize_value(&raw_args);
        let sanitized_args_str = serde_json::to_string(&sanitized_args).unwrap_or_default();

        // 4. Policy Engine Evaluation (Context-Aware)
        let decision = self.policy.evaluate(tool.as_ref(), caller, approval_token)?;
        match decision {
            PermissionDecision::Allow | PermissionDecision::AllowSessionGranted => {
                // Allowed to proceed
            }
            PermissionDecision::RequiresUserPrompt { prompt_message } => {
                let _ = self.audit.record(
                    tool_name,
                    tool.tier() as i32,
                    &caller.caller_id,
                    &sanitized_args_str,
                    "PROMPT_REQUIRED",
                    start_time.elapsed().as_millis() as i64,
                );
                return Err(ToolError::PermissionDenied {
                    tool: tool_name.to_string(),
                    tier: tool.tier(),
                    reason: format!("User prompt required: {}", prompt_message),
                });
            }
            PermissionDecision::Denied { reason } => {
                let _ = self.audit.record(
                    tool_name,
                    tool.tier() as i32,
                    &caller.caller_id,
                    &sanitized_args_str,
                    "DENIED",
                    start_time.elapsed().as_millis() as i64,
                );
                return Err(ToolError::PermissionDenied {
                    tool: tool_name.to_string(),
                    tier: tool.tier(),
                    reason,
                });
            }
        }

        // 5. Check bounded cancellation checkpoint before execution
        if cancel.is_cancelled() {
            let _ = self.audit.record(
                tool_name,
                tool.tier() as i32,
                &caller.caller_id,
                &sanitized_args_str,
                "CANCELLED",
                start_time.elapsed().as_millis() as i64,
            );
            return Err(ToolError::Cancelled);
        }

        // 6. Execute tool with bounded cancellation token
        let exec_result = tool.execute(sanitized_args.clone(), cancel).await;
        let duration_ms = start_time.elapsed().as_millis() as i64;

        match exec_result {
            Ok(output) => {
                let sanitized_output = SecretSanitizer::sanitize_value(&output);
                let _ = self.audit.record(
                    tool_name,
                    tool.tier() as i32,
                    &caller.caller_id,
                    &sanitized_args_str,
                    "EXECUTED",
                    duration_ms,
                );
                Ok(sanitized_output)
            }
            Err(ToolError::Cancelled) => {
                let _ = self.audit.record(
                    tool_name,
                    tool.tier() as i32,
                    &caller.caller_id,
                    &sanitized_args_str,
                    "CANCELLED",
                    duration_ms,
                );
                Err(ToolError::Cancelled)
            }
            Err(e) => {
                let _ = self.audit.record(
                    tool_name,
                    tool.tier() as i32,
                    &caller.caller_id,
                    &sanitized_args_str,
                    "EXECUTION_FAILED",
                    duration_ms,
                );
                Err(e)
            }
        }
    }
}
