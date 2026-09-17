//! Central [`ToolBus`] — the single dispatch point for all AI tool calls.
//!
//! # Dispatch lifecycle
//!
//! ```text
//! AI caller
//!   │
//!   ▼ dispatch(ToolRequest)
//! ToolBus
//!   ├─ 1. Lookup tool by tool_id
//!   ├─ 2. Schema validate args
//!   ├─ 3. PolicyEngine::adjudicate(PolicyContext)
//!   │       ├─ Allow          → continue
//!   │       ├─ RequireConf.   → surface modal (caller must retry with grant)
//!   │       └─ Deny           → ToolError::PermissionDenied
//!   ├─ 4. KageTool::execute(request, cancel_token)
//!   ├─ 5. SecretSanitizer::sanitize(response.output)
//!   ├─ 6. Audit::append(record)
//!   └─ 7. Return ToolResponse to caller
//! ```
//!
//! The bus enforces the Prime Architectural Invariant: AI never gets browser
//! authority directly.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::policy::{PolicyContext, PolicyDecision, PolicyEngine};
use crate::sanitizer::SecretSanitizer;
use crate::tool::{KageTool, ToolError, ToolRequest, ToolResponse};

// ---------------------------------------------------------------------------
// ToolBus
// ---------------------------------------------------------------------------

/// Thread-safe central registry and dispatch point for all [`KageTool`] implementations.
pub struct ToolBus {
    registry: RwLock<HashMap<String, Arc<dyn KageTool>>>,
    policy: PolicyEngine,
    sanitizer: SecretSanitizer,
}

impl ToolBus {
    /// Create a new, empty [`ToolBus`].
    pub fn new() -> Self {
        ToolBus {
            registry: RwLock::new(HashMap::new()),
            policy: PolicyEngine::new(),
            sanitizer: SecretSanitizer::new(),
        }
    }

    /// Register a [`KageTool`] implementation.
    ///
    /// Panics if a tool with the same `tool_id` is already registered, preventing
    /// silent shadowing of security-critical tool implementations.
    pub async fn register(&self, tool: impl KageTool) {
        let id = tool.tool_id().to_string();
        let mut registry = self.registry.write().await;
        assert!(
            !registry.contains_key(&id),
            "Duplicate tool registration: '{id}'. Tool IDs must be globally unique."
        );
        registry.insert(id, Arc::new(tool));
    }

    /// Dispatch a [`ToolRequest`] through the full governance pipeline.
    ///
    /// # Arguments
    ///
    /// * `request` — the tool call parameters from the AI subsystem.
    /// * `ctx_partial` — policy context fields supplied by the caller (caller_id,
    ///   session_id, workspace_id, session_granted).  The bus fills in `tool_id`
    ///   and `required_tier` from the tool registry.
    /// * `cancel` — cancellation token; firing it causes in-flight execution to abort.
    pub async fn dispatch(
        &self,
        request: ToolRequest,
        ctx_partial: PartialPolicyContext,
        cancel: CancellationToken,
    ) -> Result<ToolResponse, ToolError> {
        let start = Instant::now();

        // 1. Lookup tool.
        let tool = {
            let registry = self.registry.read().await;
            registry
                .get(&request.tool_id)
                .cloned()
                .ok_or_else(|| ToolError::NotFound { id: request.tool_id.clone() })?
        };

        // 2. Schema validation (lightweight structural check via JSON value shape).
        //    Full JSON Schema validation would add `jsonschema` crate; this stub
        //    ensures the args are at minimum a JSON object.
        if !request.args.is_object() && !request.args.is_null() {
            return Err(ToolError::SchemaViolation {
                tool_id: request.tool_id.clone(),
                reason: "args must be a JSON object".to_string(),
            });
        }

        // 3. Policy adjudication.
        let policy_ctx = PolicyContext {
            caller_id: ctx_partial.caller_id,
            session_id: ctx_partial.session_id,
            workspace_id: ctx_partial.workspace_id,
            tool_id: request.tool_id.clone(),
            required_tier: tool.tier(),
            session_granted: ctx_partial.session_granted,
        };
        match self.policy.adjudicate(&policy_ctx) {
            PolicyDecision::Allow => { /* proceed */ }
            PolicyDecision::RequireConfirmation { reason } => {
                return Err(ToolError::PermissionDenied {
                    tool_id: request.tool_id.clone(),
                    required: policy_ctx.required_tier,
                    decision: format!("requires_confirmation: {reason}"),
                });
            }
            PolicyDecision::Deny { reason } => {
                return Err(ToolError::PermissionDenied {
                    tool_id: request.tool_id.clone(),
                    required: policy_ctx.required_tier,
                    decision: format!("denied: {reason}"),
                });
            }
        }

        // 4. Execute (respects cancellation token).
        let mut response = tool.execute(&request, cancel).await?;

        // 5. Sanitize output before returning to caller.
        response.output = self.sanitizer.sanitize(response.output);

        // 6. TODO: Audit::append(record) — wired in Chunk 7 when kage-storage is linked.
        let _elapsed = start.elapsed();

        Ok(response)
    }
}

impl Default for ToolBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Caller-supplied portion of [`PolicyContext`]; the bus fills in tool metadata.
#[derive(Debug, Clone)]
pub struct PartialPolicyContext {
    pub caller_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub session_granted: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolError;
    use async_trait::async_trait;
    use serde_json::json;

    struct EchoTool;

    #[async_trait]
    impl KageTool for EchoTool {
        fn tool_id(&self) -> &'static str {
            "test.echo"
        }
        fn tier(&self) -> PermissionTier {
            PermissionTier::ReadOnly
        }
        fn schema(&self) -> serde_json::Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            request: &ToolRequest,
            _cancel: CancellationToken,
        ) -> Result<ToolResponse, ToolError> {
            Ok(ToolResponse {
                request_id: request.request_id.clone(),
                output: request.args.clone(),
                elapsed_ms: 0,
            })
        }
    }

    fn partial_ctx() -> PartialPolicyContext {
        PartialPolicyContext {
            caller_id: "ai_subsystem".into(),
            session_id: "sess-test".into(),
            workspace_id: "ws-default".into(),
            session_granted: false,
        }
    }

    #[tokio::test]
    async fn dispatch_tier1_succeeds() {
        let bus = ToolBus::new();
        bus.register(EchoTool).await;

        let req = ToolRequest {
            tool_id: "test.echo".into(),
            args: json!({ "msg": "hello" }),
            request_id: "req-001".into(),
            reason: "unit test".into(),
        };
        let resp = bus.dispatch(req, partial_ctx(), CancellationToken::new()).await.unwrap();
        assert_eq!(resp.output["msg"], "hello");
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_errors() {
        let bus = ToolBus::new();
        let req = ToolRequest {
            tool_id: "nonexistent.tool".into(),
            args: json!({}),
            request_id: "req-002".into(),
            reason: "unit test".into(),
        };
        let err = bus.dispatch(req, partial_ctx(), CancellationToken::new()).await.unwrap_err();
        assert!(matches!(err, ToolError::NotFound { .. }));
    }

    #[tokio::test]
    async fn dispatch_sanitizes_secret_in_output() {
        let bus = ToolBus::new();
        bus.register(EchoTool).await;

        let req = ToolRequest {
            tool_id: "test.echo".into(),
            args: json!({ "password": "supersecret", "url": "https://example.com" }),
            request_id: "req-003".into(),
            reason: "unit test",
        };
        let resp = bus.dispatch(req, partial_ctx(), CancellationToken::new()).await.unwrap();
        assert_eq!(resp.output["password"], "[REDACTED]");
        assert_eq!(resp.output["url"], "https://example.com");
    }
}
