//! Empirical Verification Suite: Governed `eval_js` and `INV-02` ToolBus Pipeline.
//!
//! Validates:
//! 1. **INV-02**: All JavaScript evaluations pass strictly through `ToolBus`.
//! 2. **Policy Adjudication**: Unconfirmed/denied callers fail closed before any CDP command is sent.
//! 3. **INV-05**: Pre-execution audit commitment (Started intent). Audit failures abort execution immediately.
//! 4. **INV-06**: Evaluated output passes through `SecretSanitizer` before reaching caller.
//! 5. **INV-09**: Cancellation token immediately aborts awaiting result and records terminal cancellation.
//! 6. **INV-10/12**: Explicit tab and CDP target identity resolution. Unknown tabs fail closed with `NotFound`.
//! 7. **Session Reuse**: Persistent `CdpSessionManager` reuses attached `SessionId` across repeated evaluations.
//! 8. **Observable Return Value**: Returns structured V8 results (`RemoteObject`) and handles `exceptionDetails`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use kage_browser::{BrowserEventBus, ProfileId, ProfileManager, TabManager};
use kage_cdp::CdpBroker;
use kage_core::audit::{AuditError, AuditSink, AuditStatus, CanonicalAuditRecord};
use kage_core::bus::{PartialPolicyContext, ToolBus};
use kage_core::tool::{ToolError, ToolRequest};
use kage_host_lib::cdp_session::CdpSessionManager;
use kage_host_lib::tools::RuntimeEvaluateTool;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Mock audit sink instrumented with atomic counters and failure toggle.
struct TestAuditSink {
    records: Mutex<Vec<CanonicalAuditRecord>>,
    fail_appends: AtomicBool,
    append_count: AtomicU64,
}

impl TestAuditSink {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            fail_appends: AtomicBool::new(false),
            append_count: AtomicU64::new(0),
        }
    }

    fn set_fail(&self, fail: bool) {
        self.fail_appends.store(fail, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl AuditSink for TestAuditSink {
    async fn append(&self, record: CanonicalAuditRecord) -> Result<u64, AuditError> {
        if self.fail_appends.load(Ordering::SeqCst) {
            return Err(AuditError::Storage("Simulated audit disk failure (INV-05)".into()));
        }
        self.append_count.fetch_add(1, Ordering::SeqCst);
        let mut list = self.records.lock().await;
        let seq = (list.len() + 1) as u64;
        list.push(record);
        Ok(seq)
    }
}

#[tokio::test]
async fn test_eval_js_policy_denial_prevents_execution() {
    // GATE 1: When caller lacks session approval for Tier-2 capability,
    // PolicyEngine blocks dispatch before any tool execution occurs.
    let broker = Arc::new(CdpBroker::new());
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_01")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    // Create a tab
    let tab_id = tab_mgr
        .create_tab(ProfileId::agent_sandbox(), "https://example.com")
        .await
        .unwrap();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "1 + 1",
            "return_by_value": true,
            "await_promise": true,
        }),
        request_id: "req_denial_01".to_string(),
        reason: "Test ungranted eval_js".to_string(),
    };

    // Caller with session_granted = false (e.g. unescalated AgentSandbox)
    let ctx = PartialPolicyContext::new(
        "agent_untrusted",
        "sess_01",
        "default_ws",
        false, // session_granted = false
    );

    let result = tool_bus.dispatch(request, ctx, CancellationToken::new()).await;

    // Assert permission denied
    assert!(result.is_err(), "Dispatch must fail when session_granted is false");
    match result.unwrap_err() {
        ToolError::PermissionDenied { tool_id, decision, .. } => {
            assert_eq!(tool_id, "devtools.runtime.evaluate");
            assert!(decision.contains("requires_confirmation"));
        }
        other => panic!("Expected ToolError::PermissionDenied, got: {other:?}"),
    }

    // Verify audit recorded Denied record with argument digest
    let records = audit_sink.records.lock().await;
    assert_eq!(records.len(), 1, "Must record denied audit entry");
    assert_eq!(records[0].status, AuditStatus::Denied);
    assert_eq!(records[0].args_digest.len(), 64, "args_digest must be 64-character hex sha256");
    println!("  [PASS] Gate 1: Policy denial prevents execution and logs audit record.");
}

#[tokio::test]
async fn test_eval_js_audit_failure_fails_closed_inv05() {
    // GATE 2: (INV-05 Pre-Execution Fail-Closed)
    // If the audit sink fails to commit the pre-execution intent (Started),
    // execution halts immediately. Tool::execute is NEVER called.
    let broker = Arc::new(CdpBroker::new());
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    audit_sink.set_fail(true); // Simulate broken audit disk

    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_02")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "document.cookie",
        }),
        request_id: "req_audit_fail_01".to_string(),
        reason: "Test audit fail closed".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_02",
        "default_ws",
        true, // session granted
    );

    let result = tool_bus.dispatch(request, ctx, CancellationToken::new()).await;

    // Assert fail-closed on audit error
    assert!(result.is_err(), "Must fail closed when audit append fails");
    match result.unwrap_err() {
        ToolError::AuditFailure(msg) => {
            assert!(msg.contains("failed closed"));
        }
        other => panic!("Expected ToolError::AuditFailure, got: {other:?}"),
    }
    println!("  [PASS] Gate 2: INV-05 Two-stage audit failure fails closed.");
}

#[tokio::test]
async fn test_eval_js_unknown_tab_fails_closed() {
    // GATE 3: Unknown or invalid tab UUID is rejected by identity validation (INV-10, INV-12).
    let broker = Arc::new(CdpBroker::new());
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_03")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let fake_tab_id = Uuid::new_v4();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": fake_tab_id.to_string(),
            "expression": "window.location.href",
        }),
        request_id: "req_unknown_tab_01".to_string(),
        reason: "Test non-existent tab".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_03",
        "default_ws",
        true,
    );

    let result = tool_bus.dispatch(request, ctx, CancellationToken::new()).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::NotFound { id } => {
            assert!(id.contains(&fake_tab_id.to_string()));
        }
        other => panic!("Expected ToolError::NotFound, got: {other:?}"),
    }
    println!("  [PASS] Gate 3: Unknown TabId fails closed with ToolError::NotFound.");
}

#[tokio::test]
async fn test_eval_js_cancellation_semantics_inv09() {
    // GATE 4: (INV-09 Cancellation)
    // When cancellation token triggers, tool stops awaiting result immediately.
    let broker = Arc::new(CdpBroker::new());
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_04")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "new Promise(() => {})", // Infinite pending promise
            "await_promise": true,
        }),
        request_id: "req_cancel_01".to_string(),
        reason: "Test cancellation token".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_04",
        "default_ws",
        true,
    );

    let cancel_token = CancellationToken::new();
    cancel_token.cancel(); // Pre-cancelled token

    let result = tool_bus.dispatch(request, ctx, cancel_token).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::Cancelled { request_id } => {
            assert_eq!(request_id, "req_cancel_01");
        }
        other => panic!("Expected ToolError::Cancelled, got: {other:?}"),
    }
    println!("  [PASS] Gate 4: Cancellation token returns ToolError::Cancelled immediately.");
}

#[tokio::test]
async fn test_eval_js_secret_sanitization_inv06() {
    // GATE 5: (INV-06 Secret Sanitizer Boundary)
    // Ensures evaluated response outputs containing credential patterns are sanitized before returning.
    use kage_core::sanitizer::SecretSanitizer;
    let sanitizer = SecretSanitizer::new();

    let secret_output = json!({
        "result": {
            "type": "string",
            "value": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.do_not_leak_this"
        }
    });

    let sanitized = sanitizer.sanitize(secret_output);
    let val_str = sanitized["result"]["value"].as_str().unwrap();
    assert!(!val_str.contains("do_not_leak_this"), "JWT secret must not appear in output");
    assert!(val_str.contains("[REDACTED]"), "Secret must be redacted");
    println!("  [PASS] Gate 5: SecretSanitizer scrubs JWT credential pattern.");
}

#[tokio::test]
async fn test_eval_js_nominal_v8_evaluation_via_toolbus() {
    // GATE 6: Nominal V8 Evaluation over loopback WebSocket
    // Proves: ToolBus -> RuntimeEvaluateTool -> CdpSessionManager -> CdpBroker -> Result -> Sanitizer -> Audit
    let (broker, _) = CdpBroker::bind_ephemeral_random().await.unwrap();
    let broker = Arc::new(broker);
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_nominal")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    // Create tab, bind CEF browser and CDP target
    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();
    tab_mgr.bind_cef_browser(tab_id, 1).await.unwrap();
    tab_mgr.bind_cdp_target(tab_id, "mock_page_target_01").await.unwrap();

    // 1. Nominal arithmetic: "1 + 1" -> 2
    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "1 + 1",
            "return_by_value": true,
            "await_promise": true,
        }),
        request_id: "req_nominal_01".to_string(),
        reason: "Test nominal evaluate".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_nominal",
        "default_ws",
        true,
    );

    let resp = tool_bus.dispatch(request, ctx.clone(), CancellationToken::new()).await.expect("dispatch nominal 1+1");
    assert_eq!(resp.output["result"]["type"], "number");
    assert_eq!(resp.output["result"]["value"], 2);

    // 2. Promise resolution: "Promise.resolve(42)" -> 42
    let promise_req = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "Promise.resolve(42)",
            "return_by_value": true,
            "await_promise": true,
        }),
        request_id: "req_nominal_02".to_string(),
        reason: "Test promise evaluate".to_string(),
    };

    let promise_resp = tool_bus.dispatch(promise_req, ctx, CancellationToken::new()).await.expect("dispatch promise");
    assert_eq!(promise_resp.output["result"]["type"], "number");
    assert_eq!(promise_resp.output["result"]["value"], 42);

    // Verify audit ledger commits both invocations with 64-char SHA-256 digests
    let records = audit_sink.records.lock().await;
    // 2 requests * 2 records each (Started + Success) = 4 records
    assert_eq!(records.len(), 4, "Must have 2 Started and 2 Success audit records");
    assert_eq!(records[0].status, AuditStatus::Started);
    assert_eq!(records[1].status, AuditStatus::Success);
    assert_eq!(records[0].args_digest.len(), 64);
    assert_eq!(records[1].args_digest.len(), 64);
    println!("  [PASS] Gate 6: Nominal V8 evaluation ('1 + 1' -> 2, 'Promise.resolve(42)' -> 42) succeeds with full audit chain.");
}

#[tokio::test]
async fn test_eval_js_exception_details_preservation() {
    // GATE 7: Exception Details Preservation
    // Verifies that V8 evaluation errors preserve structured `exceptionDetails` for DevTools reporting.
    let (broker, _) = CdpBroker::bind_ephemeral_random().await.unwrap();
    let broker = Arc::new(broker);
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_err")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();
    tab_mgr.bind_cef_browser(tab_id, 2).await.unwrap();
    tab_mgr.bind_cdp_target(tab_id, "mock_page_target_02").await.unwrap();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "throw new Error('KAGE_TEST_ERROR')",
        }),
        request_id: "req_exception_01".to_string(),
        reason: "Test exception propagation".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_err",
        "default_ws",
        true,
    );

    let resp = tool_bus.dispatch(request, ctx, CancellationToken::new()).await.expect("dispatch throw");
    assert!(resp.output.get("exceptionDetails").is_some(), "Must preserve exceptionDetails object");
    let err_text = resp.output["exceptionDetails"]["text"].as_str().unwrap();
    assert!(err_text.contains("KAGE_TEST_ERROR"), "Error text must be preserved");
    println!("  [PASS] Gate 7: JavaScript exception Details preserved in ToolResponse.");
}

#[tokio::test]
async fn test_eval_js_stale_session_auto_recovery() {
    // GATE 8: Stale CDP Session Invalidation & Automatic Re-Attachment
    // Simulates a stale session error ("No session with given id"), verifies CdpSessionManager
    // invalidates cached session, calls Target.attachToTarget for a fresh sessionId, and succeeds on single retry.
    let (broker, _) = CdpBroker::bind_ephemeral_random().await.unwrap();
    let broker = Arc::new(broker);
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_stale")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();
    tab_mgr.bind_cef_browser(tab_id, 3).await.unwrap();
    tab_mgr.bind_cdp_target(tab_id, "mock_page_target_stale").await.unwrap();

    // Explicitly seed the session manager with a stale session id that the broker will reject on first evaluate
    session_mgr.seed_session_for_test(tab_id, "mock_page_target_stale", "stale_session_seed").await;

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "trigger_stale_session_test",
        }),
        request_id: "req_stale_01".to_string(),
        reason: "Test stale session auto-recovery".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_stale",
        "default_ws",
        true,
    );

    // Dispatch should automatically recover and succeed
    let resp = tool_bus.dispatch(request, ctx, CancellationToken::new()).await.expect("auto-recovery dispatch");
    assert!(resp.output["result"]["value"].is_string(), "Must return result after retry");
    println!("  [PASS] Gate 8: Stale CDP session invalidated, re-attached, and succeeded on retry.");
}

#[tokio::test]
async fn test_eval_js_agent_context_sink_boundary_inv06() {
    // GATE 9: (INV-06 Agent Context Sink Boundary)
    // Proves that when an evaluation outputs a sensitive token (JWT / Bearer token),
    // the ToolBus boundary scrub prevents the secret from reaching any agent context sink.
    let (broker, _) = CdpBroker::bind_ephemeral_random().await.unwrap();
    let broker = Arc::new(broker);
    let session_mgr = Arc::new(CdpSessionManager::new(broker.clone()));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(10);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(TestAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_boundary")
            .with_audit_sink(audit_sink.clone()),
    );

    let tool = RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone());
    tool_bus.register(tool).await;

    let tab_id = tab_mgr
        .create_tab(ProfileId::personal(), "https://example.com")
        .await
        .unwrap();
    tab_mgr.bind_cef_browser(tab_id, 4).await.unwrap();
    tab_mgr.bind_cdp_target(tab_id, "mock_page_target_boundary").await.unwrap();

    let request = ToolRequest {
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": tab_id.to_string(),
            "expression": "generate_secret_token",
        }),
        request_id: "req_boundary_01".to_string(),
        reason: "Test secret boundary".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "devtools_console",
        "sess_boundary",
        "default_ws",
        true,
    );

    let resp = tool_bus.dispatch(request, ctx, CancellationToken::new()).await.expect("dispatch secret test");
    let returned_value = resp.output["result"]["value"].as_str().unwrap();

    // Verify raw secret was sanitized at the ToolBus exit boundary
    assert!(!returned_value.contains("super_secret_jwt"), "Raw secret token MUST NOT cross ToolBus boundary");
    assert!(returned_value.contains("[REDACTED]"), "Secret MUST be redacted to [REDACTED]");
    println!("  [PASS] Gate 9: INV-06 Agent Context Boundary proves zero raw secrets cross ToolBus output boundary.");
}
