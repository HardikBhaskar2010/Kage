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

use crate::audit::{digest_json, ActorType, AuditSink, AuditStatus, CanonicalAuditRecord};
use crate::policy::{AuditFailurePolicy, PolicyContext, PolicyDecision, PolicyEngine};
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
    audit_sink: Option<Arc<dyn AuditSink>>,
    host_instance_id: String,
}

impl ToolBus {
    /// Create a new, empty [`ToolBus`].
    pub fn new() -> Self {
        ToolBus {
            registry: RwLock::new(HashMap::new()),
            policy: PolicyEngine::new(),
            sanitizer: SecretSanitizer::new(),
            audit_sink: None,
            host_instance_id: format!("inst_{}", uuid::Uuid::new_v4().simple()),
        }
    }

    /// Set an explicit host instance ID (e.g. from desktop host process).
    pub fn with_host_instance_id(mut self, id: impl Into<String>) -> Self {
        self.host_instance_id = id.into();
        self
    }

    /// Set an [`AuditSink`] on builder pattern.
    pub fn with_audit_sink(mut self, sink: Arc<dyn AuditSink>) -> Self {
        self.audit_sink = Some(sink);
        self
    }

    /// Set an [`AuditSink`] dynamically.
    pub fn set_audit_sink(&mut self, sink: Arc<dyn AuditSink>) {
        self.audit_sink = Some(sink);
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
    /// # Invariants Enforced
    /// - INV-02: All browser actions pass through ToolBus.
    /// - INV-04: Privileged mutations require policy approval.
    /// - INV-05: Privileged mutations require successful audit commitment (Fail-Closed).
    /// - INV-06: Secrets sanitized before returning to caller.
    /// - INV-09: STOP cancellation token immediately halts execution.
    pub async fn dispatch(
        &self,
        request: ToolRequest,
        ctx_partial: PartialPolicyContext,
        cancel: CancellationToken,
    ) -> Result<ToolResponse, ToolError> {
        let start = Instant::now();

        // Check cancellation immediately (INV-09)
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled {
                request_id: request.request_id.clone(),
            });
        }

        // 1. Lookup tool.
        let tool = {
            let registry = self.registry.read().await;
            registry
                .get(&request.tool_id)
                .cloned()
                .ok_or_else(|| ToolError::NotFound { id: request.tool_id.clone() })?
        };

        // 2. Schema validation (lightweight structural check via JSON value shape).
        if !request.args.is_object() && !request.args.is_null() {
            return Err(ToolError::SchemaViolation {
                tool_id: request.tool_id.clone(),
                reason: "args must be a JSON object".to_string(),
            });
        }

        // 3. Policy adjudication (INV-04).
        let policy_ctx = PolicyContext {
            caller_id: ctx_partial.caller_id.clone(),
            session_id: ctx_partial.session_id.clone(),
            workspace_id: ctx_partial.workspace_id.clone(),
            tool_id: request.tool_id.clone(),
            required_tier: tool.tier(),
            session_granted: ctx_partial.session_granted,
        };
        match self.policy.adjudicate(&policy_ctx) {
            PolicyDecision::Allow => { /* proceed */ }
            PolicyDecision::RequireConfirmation { reason } => {
                let duration_ms = start.elapsed().as_millis() as u64;
                if let Some(ref sink) = self.audit_sink {
                    let _ = sink.append(CanonicalAuditRecord {
                        sequence: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        request_id: request.request_id.clone(),
                        parent_request_id: None,
                        caller: ctx_partial.caller_id.clone(),
                        actor: ctx_partial.actor.unwrap_or(ActorType::Agent),
                        tool_id: request.tool_id.clone(),
                        capability: format!("{:?}", tool.tier()),
                        profile_id: ctx_partial.profile_id.clone(),
                        tab_id: ctx_partial.tab_id.clone(),
                        target_id: ctx_partial.target_id.clone(),
                        session_id: Some(ctx_partial.session_id.clone()),
                        origin: ctx_partial.origin.clone(),
                        tier: tool.tier() as u8,
                        policy_decision: format!("require_confirmation: {reason}"),
                        confirmation_id: None,
                        args_digest: digest_json(&request.args),
                        result_digest: None,
                        status: AuditStatus::Denied,
                        duration_ms,
                        error_code: Some("PERMISSION_CONFIRMATION_REQUIRED".into()),
                        host_instance_id: Some(self.host_instance_id.clone()),
                        prev_hash: None,
                    }).await;
                }
                return Err(ToolError::PermissionDenied {
                    tool_id: request.tool_id.clone(),
                    required: policy_ctx.required_tier,
                    decision: format!("requires_confirmation: {reason}"),
                });
            }
            PolicyDecision::Deny { reason } => {
                let duration_ms = start.elapsed().as_millis() as u64;
                if let Some(ref sink) = self.audit_sink {
                    let _ = sink.append(CanonicalAuditRecord {
                        sequence: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        request_id: request.request_id.clone(),
                        parent_request_id: None,
                        caller: ctx_partial.caller_id.clone(),
                        actor: ctx_partial.actor.unwrap_or(ActorType::Agent),
                        tool_id: request.tool_id.clone(),
                        capability: format!("{:?}", tool.tier()),
                        profile_id: ctx_partial.profile_id.clone(),
                        tab_id: ctx_partial.tab_id.clone(),
                        target_id: ctx_partial.target_id.clone(),
                        session_id: Some(ctx_partial.session_id.clone()),
                        origin: ctx_partial.origin.clone(),
                        tier: tool.tier() as u8,
                        policy_decision: format!("denied: {reason}"),
                        confirmation_id: None,
                        args_digest: digest_json(&request.args),
                        result_digest: None,
                        status: AuditStatus::Denied,
                        duration_ms,
                        error_code: Some("PERMISSION_DENIED".into()),
                        host_instance_id: Some(self.host_instance_id.clone()),
                        prev_hash: None,
                    }).await;
                }
                return Err(ToolError::PermissionDenied {
                    tool_id: request.tool_id.clone(),
                    required: policy_ctx.required_tier,
                    decision: format!("denied: {reason}"),
                });
            }
        }

        // 4. Pre-Execution Audit Commitment for Privileged Mutations (INV-05 Two-Stage Lifecycle)
        // If the action is mutating or privileged, an AuditStatus::Started intent record
        // MUST commit to the audit ledger BEFORE tool execution.
        // If this commit fails: FAIL CLOSED immediately. The tool is NEVER invoked.
        let is_privileged_mutation = tool.tier().audit_failure_policy() == AuditFailurePolicy::FailClosed;
        if is_privileged_mutation {
            if let Some(ref sink) = self.audit_sink {
                let intent_record = CanonicalAuditRecord {
                    sequence: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    request_id: request.request_id.clone(),
                    parent_request_id: None,
                    caller: ctx_partial.caller_id.clone(),
                    actor: ctx_partial.actor.unwrap_or(ActorType::Agent),
                    tool_id: request.tool_id.clone(),
                    capability: format!("{:?}", tool.tier()),
                    profile_id: ctx_partial.profile_id.clone(),
                    tab_id: ctx_partial.tab_id.clone(),
                    target_id: ctx_partial.target_id.clone(),
                    session_id: Some(ctx_partial.session_id.clone()),
                    origin: ctx_partial.origin.clone(),
                    tier: tool.tier() as u8,
                    policy_decision: "allow".into(),
                    confirmation_id: if ctx_partial.session_granted {
                        Some(format!("session:{}", ctx_partial.session_id))
                    } else {
                        None
                    },
                    args_digest: digest_json(&request.args),
                    result_digest: None,
                    status: AuditStatus::Started,
                    duration_ms: 0,
                    error_code: None,
                    host_instance_id: Some(self.host_instance_id.clone()),
                    prev_hash: None,
                };

                if let Err(audit_err) = sink.append(intent_record).await {
                    return Err(ToolError::AuditFailure(format!(
                        "Privileged mutation '{}' failed closed: pre-execution audit intent could not be committed ({audit_err}). Action aborted before execution.",
                        request.tool_id
                    )));
                }
            }
        }

        // 5. Execute tool (respects cancellation token). Tool ONLY runs if intent committed.
        let exec_result = tool.execute(&request, cancel).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match exec_result {
            Ok(mut response) => {
                // 6. Sanitize output before returning to caller (INV-06).
                response.output = self.sanitizer.sanitize(response.output);
                response.elapsed_ms = duration_ms;

                // 7. Record completion in audit ledger (INV-05).
                if let Some(ref sink) = self.audit_sink {
                    let record = CanonicalAuditRecord {
                        sequence: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        request_id: request.request_id.clone(),
                        parent_request_id: None,
                        caller: ctx_partial.caller_id.clone(),
                        actor: ctx_partial.actor.unwrap_or(ActorType::Agent),
                        tool_id: request.tool_id.clone(),
                        capability: format!("{:?}", tool.tier()),
                        profile_id: ctx_partial.profile_id.clone(),
                        tab_id: ctx_partial.tab_id.clone(),
                        target_id: ctx_partial.target_id.clone(),
                        session_id: Some(ctx_partial.session_id.clone()),
                        origin: ctx_partial.origin.clone(),
                        tier: tool.tier() as u8,
                        policy_decision: "allow".into(),
                        confirmation_id: if ctx_partial.session_granted {
                            Some(format!("session:{}", ctx_partial.session_id))
                        } else {
                            None
                        },
                        args_digest: digest_json(&request.args),
                        result_digest: Some(digest_json(&response.output)),
                        status: AuditStatus::Success,
                        duration_ms,
                        error_code: None,
                        host_instance_id: Some(self.host_instance_id.clone()),
                        prev_hash: None,
                    };

                    if let Err(audit_err) = sink.append(record).await {
                        // Enforce Invariant 05: Fail-Closed on mutating actions
                        match tool.tier().audit_failure_policy() {
                            AuditFailurePolicy::FailClosed => {
                                return Err(ToolError::AuditFailure(format!(
                                    "Privileged action '{}' failed closed: audit completion commit failed ({audit_err})",
                                    request.tool_id
                                )));
                            }
                            AuditFailurePolicy::DegradeGraceful => {
                                // Non-sensitive telemetry degrades gracefully
                            }
                        }
                    }
                }

                Ok(response)
            }
            Err(err) => {
                if let Some(ref sink) = self.audit_sink {
                    let status = match err {
                        ToolError::Cancelled { .. } => AuditStatus::Cancelled,
                        _ => AuditStatus::Error,
                    };
                    let _ = sink.append(CanonicalAuditRecord {
                        sequence: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        request_id: request.request_id.clone(),
                        parent_request_id: None,
                        caller: ctx_partial.caller_id.clone(),
                        actor: ctx_partial.actor.unwrap_or(ActorType::Agent),
                        tool_id: request.tool_id.clone(),
                        capability: format!("{:?}", tool.tier()),
                        profile_id: ctx_partial.profile_id.clone(),
                        tab_id: ctx_partial.tab_id.clone(),
                        target_id: ctx_partial.target_id.clone(),
                        session_id: Some(ctx_partial.session_id.clone()),
                        origin: ctx_partial.origin.clone(),
                        tier: tool.tier() as u8,
                        policy_decision: "allow".into(),
                        confirmation_id: None,
                        args_digest: digest_json(&request.args),
                        result_digest: None,
                        status,
                        duration_ms,
                        error_code: Some(err.to_string()),
                        host_instance_id: Some(self.host_instance_id.clone()),
                        prev_hash: None,
                    }).await;
                }
                Err(err)
            }
        }
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
    pub actor: Option<ActorType>,
    pub profile_id: Option<String>,
    pub tab_id: Option<String>,
    pub target_id: Option<String>,
    pub origin: Option<String>,
}

impl PartialPolicyContext {
    pub fn new(
        caller_id: impl Into<String>,
        session_id: impl Into<String>,
        workspace_id: impl Into<String>,
        session_granted: bool,
    ) -> Self {
        Self {
            caller_id: caller_id.into(),
            session_id: session_id.into(),
            workspace_id: workspace_id.into(),
            session_granted,
            actor: None,
            profile_id: None,
            tab_id: None,
            target_id: None,
            origin: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PermissionTier;
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

    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Mutex;
    use crate::audit::AuditError as CoreAuditError;

    struct MutatingTool {
        executed: Arc<AtomicBool>,
    }

    impl MutatingTool {
        fn new(executed: Arc<AtomicBool>) -> Self {
            Self { executed }
        }
    }

    #[async_trait]
    impl KageTool for MutatingTool {
        fn tool_id(&self) -> &'static str {
            "test.mutate"
        }
        fn tier(&self) -> PermissionTier {
            PermissionTier::StateMutating
        }
        fn schema(&self) -> serde_json::Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            request: &ToolRequest,
            _cancel: CancellationToken,
        ) -> Result<ToolResponse, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolResponse {
                request_id: request.request_id.clone(),
                output: json!({ "status": "mutated" }),
                elapsed_ms: 0,
            })
        }
    }

    struct MockAuditSink {
        records: Mutex<Vec<CanonicalAuditRecord>>,
        should_fail: AtomicBool,
    }

    impl MockAuditSink {
        fn new() -> Self {
            Self {
                records: Mutex::new(Vec::new()),
                should_fail: AtomicBool::new(false),
            }
        }

        fn with_failure() -> Self {
            Self {
                records: Mutex::new(Vec::new()),
                should_fail: AtomicBool::new(true),
            }
        }
    }

    #[async_trait]
    impl AuditSink for MockAuditSink {
        async fn append(&self, record: CanonicalAuditRecord) -> Result<u64, CoreAuditError> {
            if self.should_fail.load(Ordering::SeqCst) {
                return Err(CoreAuditError::Storage("Simulated audit disk failure".into()));
            }
            let mut list = self.records.lock().await;
            let seq = (list.len() + 1) as u64;
            list.push(record);
            Ok(seq)
        }
    }

    fn partial_ctx() -> PartialPolicyContext {
        PartialPolicyContext::new("ai_subsystem", "sess-test", "ws-default", false)
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
    async fn dispatch_records_in_audit_sink() {
        let sink = Arc::new(MockAuditSink::new());
        let bus = ToolBus::new().with_audit_sink(sink.clone());
        bus.register(EchoTool).await;

        let req = ToolRequest {
            tool_id: "test.echo".into(),
            args: json!({ "msg": "hello" }),
            request_id: "req-audit-001".into(),
            reason: "audit test".into(),
        };
        let resp = bus.dispatch(req, partial_ctx(), CancellationToken::new()).await.unwrap();
        assert_eq!(resp.output["msg"], "hello");

        let entries = sink.records.lock().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_id, "test.echo");
        assert_eq!(entries[0].status, AuditStatus::Success);
    }

    #[tokio::test]
    async fn mutating_action_fails_closed_on_audit_error() {
        // INVARIANT 05: Privileged mutations require successful audit commitment.
        // Two-Stage Lifecycle: An AuditStatus::Started intent record MUST commit BEFORE execution.
        let sink = Arc::new(MockAuditSink::with_failure());
        let bus = ToolBus::new().with_audit_sink(sink);
        let executed = Arc::new(AtomicBool::new(false));
        bus.register(MutatingTool::new(executed.clone())).await;

        let req = ToolRequest {
            tool_id: "test.mutate".into(),
            args: json!({ "action": "click" }),
            request_id: "req-mutate-001".into(),
            reason: "mutation test".into(),
        };
        // Session granted = true so policy passes
        let mut ctx = partial_ctx();
        ctx.session_granted = true;

        let result = bus.dispatch(req, ctx, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ToolError::AuditFailure(_))),
            "Mutating action MUST fail closed when audit write fails (INV-05)"
        );
        assert_eq!(
            executed.load(Ordering::SeqCst),
            false,
            "CRITICAL SECURITY CONTRACT (INV-05): Mutating tool execute() MUST NEVER be called if audit intent fails!"
        );
    }

    #[tokio::test]
    async fn stop_halts_dispatch_immediately() {
        // INVARIANT 09: STOP prevents subsequent actions.
        let bus = ToolBus::new();
        bus.register(EchoTool).await;

        let cancel = CancellationToken::new();
        cancel.cancel(); // Pre-cancelled token

        let req = ToolRequest {
            tool_id: "test.echo".into(),
            args: json!({}),
            request_id: "req-cancel-001".into(),
            reason: "cancellation test".into(),
        };
        let result = bus.dispatch(req, partial_ctx(), cancel).await;
        assert!(
            matches!(result, Err(ToolError::Cancelled { .. })),
            "Pre-cancelled dispatch must immediately abort with ToolError::Cancelled"
        );
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
            reason: "unit test".into(),
        };
        let resp = bus.dispatch(req, partial_ctx(), CancellationToken::new()).await.unwrap();
        assert_eq!(resp.output["password"], "[REDACTED]");
        assert_eq!(resp.output["url"], "https://example.com");
    }
}
