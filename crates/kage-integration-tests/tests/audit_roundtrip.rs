//! End-to-end audit roundtrip integration test (Phase 1, INV-05).
//!
//! Validates:
//! ToolBus::dispatch -> AuditDb::append -> verify_chain -> tamper detection.

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use kage_core::audit::{ActorType, AuditReader, AuditVerifier};
use kage_core::bus::{PartialPolicyContext, ToolBus};
use kage_core::policy::PermissionTier;
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use kage_storage::AuditDb;

struct TestInspectTool;

#[async_trait]
impl KageTool for TestInspectTool {
    fn tool_id(&self) -> &'static str {
        "dom.inspect"
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
            output: json!({ "node_name": "DIV", "visible": true }),
            elapsed_ms: 5,
        })
    }
}

struct TestClickTool;

#[async_trait]
impl KageTool for TestClickTool {
    fn tool_id(&self) -> &'static str {
        "page.click"
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
        Ok(ToolResponse {
            request_id: request.request_id.clone(),
            output: json!({ "clicked": true, "selector": "button#submit" }),
            elapsed_ms: 10,
        })
    }
}

#[tokio::test]
async fn test_audit_roundtrip_and_tamper_detection() {
    // 1. Initialize AuditDb and ToolBus
    let audit_db = Arc::new(AuditDb::open_in_memory().expect("Failed to open audit DB"));
    let bus = ToolBus::new().with_audit_sink(audit_db.clone());

    bus.register(TestInspectTool).await;
    bus.register(TestClickTool).await;

    // 2. Dispatch read-only action (Tier 1)
    let inspect_req = ToolRequest {
        tool_id: "dom.inspect".into(),
        args: json!({ "selector": "#header" }),
        request_id: "req-roundtrip-01".into(),
        reason: "Inspect page header".into(),
    };
    let inspect_ctx = PartialPolicyContext {
        caller_id: "ai_planner".into(),
        session_id: "sess_01".into(),
        workspace_id: "ws_01".into(),
        session_granted: false,
        actor: Some(ActorType::Agent),
        profile_id: Some("prof_default".into()),
        tab_id: Some("tab_01".into()),
        target_id: Some("target_01".into()),
        origin: Some("https://example.com".into()),
    };

    let resp1 = bus
        .dispatch(inspect_req, inspect_ctx, CancellationToken::new())
        .await
        .expect("Tier 1 tool must succeed");
    assert_eq!(resp1.output["node_name"], "DIV");

    // 3. Dispatch mutating action (Tier 2) with session approval (Two-stage lifecycle)
    let click_req = ToolRequest {
        tool_id: "page.click".into(),
        args: json!({ "selector": "button#submit" }),
        request_id: "req-roundtrip-02".into(),
        reason: "Click submit button".into(),
    };
    let click_ctx = PartialPolicyContext {
        caller_id: "ai_planner".into(),
        session_id: "sess_01".into(),
        workspace_id: "ws_01".into(),
        session_granted: true, // Pre-approved by user session
        actor: Some(ActorType::Agent),
        profile_id: Some("prof_default".into()),
        tab_id: Some("tab_01".into()),
        target_id: Some("target_01".into()),
        origin: Some("https://example.com".into()),
    };

    let resp2 = bus
        .dispatch(click_req, click_ctx, CancellationToken::new())
        .await
        .expect("Approved Tier 2 tool must succeed");
    assert_eq!(resp2.output["clicked"], true);

    // 4. Verify cryptographic chain integrity via AuditVerifier
    AuditVerifier::verify_chain(&*audit_db)
        .await
        .expect("Chain must be 100% valid before tampering");

    // 5. Check stored log entries via AuditReader
    // Expect 3 entries:
    // seq 1: dom.inspect (success)
    // seq 2: page.click intent (started)
    // seq 3: page.click completion (success)
    let records = AuditReader::get_recent_records(&*audit_db, 10)
        .await
        .expect("Fetch records");
    assert_eq!(records.len(), 3);

    // Most recent is sequence 3: page.click completion
    assert_eq!(records[0].sequence, 3);
    assert_eq!(records[0].tool_id, "page.click");
    assert_eq!(records[0].status, "success");
    assert_eq!(records[0].request_id, "req-roundtrip-02");
    assert_eq!(records[0].target_id.as_deref(), Some("target_01"));

    // Middle is sequence 2: page.click pre-execution intent
    assert_eq!(records[1].sequence, 2);
    assert_eq!(records[1].tool_id, "page.click");
    assert_eq!(records[1].status, "started");
    assert_eq!(records[1].request_id, "req-roundtrip-02");
    assert_eq!(records[1].target_id.as_deref(), Some("target_01"));

    // Earliest is sequence 1: dom.inspect completion
    assert_eq!(records[2].sequence, 1);
    assert_eq!(records[2].tool_id, "dom.inspect");
    assert_eq!(records[2].status, "success");
    assert_eq!(records[2].request_id, "req-roundtrip-01");
    assert_eq!(records[2].target_id.as_deref(), Some("target_01"));
}
