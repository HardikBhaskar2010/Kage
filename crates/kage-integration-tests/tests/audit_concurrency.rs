//! Concurrent audit append integration test (Phase 1, Item 4).
//!
//! Validates that 50 concurrent Tokio tasks dispatching through ToolBus
//! produce a strictly monotonic sequence (1..=50) with zero hash collisions,
//! zero forked hash chains, and 100% cryptographic integrity.

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use kage_core::audit::ActorType;
use kage_core::bus::{PartialPolicyContext, ToolBus};
use kage_core::policy::PermissionTier;
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use kage_storage::AuditDb;

struct ConcurrentTestTool;

#[async_trait]
impl KageTool for ConcurrentTestTool {
    fn tool_id(&self) -> &'static str {
        "concurrent.action"
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
            output: json!({ "echo": request.args }),
            elapsed_ms: 1,
        })
    }
}

#[tokio::test]
async fn test_concurrent_serialized_audit_appends() {
    let audit_db = Arc::new(AuditDb::open_in_memory().expect("Failed to open in-memory audit DB"));
    let bus = Arc::new(ToolBus::new().with_audit_sink(audit_db.clone()));

    bus.register(ConcurrentTestTool).await;

    const CONCURRENT_TASKS: usize = 50;
    let mut set = JoinSet::new();

    // Spawn 50 concurrent Tokio tasks dispatching simultaneously
    for i in 1..=CONCURRENT_TASKS {
        let bus = bus.clone();
        set.spawn(async move {
            let req = ToolRequest {
                tool_id: "concurrent.action".into(),
                args: json!({ "task_index": i }),
                request_id: format!("req-concurrent-{i:03}"),
                reason: format!("Concurrent task {i}"),
            };
            let ctx = PartialPolicyContext {
                caller_id: format!("agent_worker_{i}"),
                session_id: "sess_parallel".into(),
                workspace_id: "ws_concurrency".into(),
                session_granted: false,
                actor: Some(ActorType::Agent),
                profile_id: Some("prof_sandbox".into()),
                tab_id: Some(format!("tab_{i}")),
                target_id: Some(format!("target_{i}")),
                origin: Some("https://example.com".into()),
            };
            bus.dispatch(req, ctx, CancellationToken::new()).await
        });
    }

    // Wait for all 50 tasks to complete
    let mut success_count = 0;
    while let Some(res) = set.join_next().await {
        let dispatch_res = res.expect("Tokio task join must succeed");
        assert!(dispatch_res.is_ok(), "Dispatch must succeed");
        success_count += 1;
    }
    assert_eq!(success_count, CONCURRENT_TASKS);

    // Verify cryptographic integrity of the resulting serialized chain
    audit_db
        .verify_chain()
        .await
        .expect("Cryptographic hash chain must be 100% intact with zero forking");

    // Fetch all 50 records and verify strictly monotonic sequences
    let records = audit_db.get_recent_records(CONCURRENT_TASKS + 10).await.unwrap();
    assert_eq!(records.len(), CONCURRENT_TASKS);

    // Records are ordered by sequence DESC: [50, 49, ..., 1]
    for (idx, record) in records.iter().enumerate() {
        let expected_seq = (CONCURRENT_TASKS - idx) as u64;
        assert_eq!(
            record.sequence, expected_seq,
            "Sequence must be strictly monotonic without duplicates or gaps"
        );
    }
}
