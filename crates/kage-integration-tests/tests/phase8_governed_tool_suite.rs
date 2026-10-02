//! Phase 8 Governed KAGE Tool Suite & Capability Registry Integration Test Suite.
//!
//! Validates:
//! - **GATE-08-A**: First-class action & execution lineage (AgentTaskId -> PlanStepId -> ToolRequestId -> ToolExecutionId -> AuditRecordId).
//! - **GATE-08-B**: Layered action-result model (status, timestamps, latency_ms, effect_summary, failure).
//! - **GATE-08-C**: Canonical browser capability suite (browser.navigate, reload, go_back, go_forward, stop, wait_for_navigation).
//! - **GATE-08-D**: Canonical tab capability suite with INV-10 explicit (TabId, ProfileId) binding.
//! - **GATE-08-E**: Canonical page interaction suite (synthetic CDP mouse/keyboard/DOM input).
//! - **GATE-08-F**: Canonical page observation suite (structured DOM/links/forms/AX tree without arbitrary JS execution).
//! - **GATE-08-G**: Dynamic capability registry & metadata indexing (idempotency, retry policies, contracts).
//! - **GATE-08-H**: Strict prohibition of generic eval_js backdoor for autonomous agents (INV-01, fail-closed).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use kage_browser::{BrowserEventBus, ProfileId, ProfileManager, TabManager};
use kage_cdp::CdpBroker;
use kage_core::audit::{AuditError, AuditSink, AuditStatus, CanonicalAuditRecord};
use kage_core::bus::{PartialPolicyContext, ToolBus};
use kage_core::lineage::{AgentTaskId, ExecutionStatus, PlanStepId, ToolExecutionId};
use kage_core::policy::PermissionTier;
use kage_core::registry::{CapabilityRegistry, DeclarativeContract, IdempotencyClassification, RetryPolicy, ToolCategory, ToolMetadata};
use kage_core::tool::{KageTool, ToolError, ToolRequest, ToolResponse};
use kage_host_lib::cdp_session::CdpSessionManager;
use kage_host_lib::tools::*;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// In-memory mock audit sink recording all canonical audit log entries.
struct InMemoryAuditSink {
    records: Mutex<Vec<CanonicalAuditRecord>>,
    append_count: AtomicU64,
    fail_appends: AtomicBool,
}

impl InMemoryAuditSink {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            append_count: AtomicU64::new(0),
            fail_appends: AtomicBool::new(false),
        }
    }

    async fn get_records(&self) -> Vec<CanonicalAuditRecord> {
        self.records.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl AuditSink for InMemoryAuditSink {
    async fn append(&self, record: CanonicalAuditRecord) -> Result<u64, AuditError> {
        if self.fail_appends.load(Ordering::SeqCst) {
            return Err(AuditError::Storage("Simulated audit disk failure".into()));
        }
        self.append_count.fetch_add(1, Ordering::SeqCst);
        let mut list = self.records.lock().await;
        let seq = (list.len() + 1) as u64;
        list.push(record);
        Ok(seq)
    }
}

/// Helper to construct standard test harness
async fn setup_test_harness() -> (
    Arc<ToolBus>,
    Arc<InMemoryAuditSink>,
    Arc<TabManager>,
    Arc<CdpSessionManager>,
) {
    let broker = Arc::new(CdpBroker::new());
    let session_mgr = Arc::new(CdpSessionManager::new(broker));
    let profile_mgr = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(100);
    let tab_mgr = Arc::new(TabManager::new(profile_mgr, event_bus));

    let audit_sink = Arc::new(InMemoryAuditSink::new());
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id("test_host_p8")
            .with_audit_sink(audit_sink.clone()),
    );

    (tool_bus, audit_sink, tab_mgr, session_mgr)
}

// ─── GATE-08-A: First-Class Action & Execution Lineage ───────────────────────

#[tokio::test]
async fn test_gate_08_a_first_class_action_lineage() {
    let (tool_bus, audit_sink, tab_mgr, session_mgr) = setup_test_harness().await;

    // Register tab.create with metadata
    tool_bus.register_with_metadata(
        TabCreateTool::new(tab_mgr.clone()),
        TabCreateTool::metadata(),
    ).await;

    let task_id = AgentTaskId::new();
    let step_id = PlanStepId::new();
    let execution_id = ToolExecutionId::new();
    let request_id = format!("req_{}", Uuid::new_v4());

    let request = ToolRequest {
        task_id: Some(task_id.to_string()),
        step_id: Some(step_id.to_string()),
        execution_id: Some(execution_id.to_string()),
        tool_id: "tab.create".to_string(),
        args: json!({
            "url": "https://example.com/checkout",
            "profile_id": "personal"
        }),
        request_id: request_id.clone(),
        reason: "Autonomous test purchase step".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "agent_planner",
        "session_lineage_01",
        "workspace_lineage",
        true,
    );

    let response = tool_bus
        .dispatch(request, ctx, CancellationToken::new())
        .await
        .expect("tab.create execution should succeed");

    // 1. Verify Lineage IDs in ToolResponse
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.task_id, Some(task_id.to_string()));
    assert_eq!(response.step_id, Some(step_id.to_string()));
    assert_eq!(response.execution_id, execution_id.to_string());
    assert_eq!(response.status, ExecutionStatus::Success);

    // 2. Verify Lineage IDs committed to Audit Ledger
    let records = audit_sink.get_records().await;
    assert_eq!(records.len(), 2, "Must produce Started intent and Success record");

    let started_record = &records[0];
    assert_eq!(started_record.status, AuditStatus::Started);
    assert_eq!(started_record.task_id, Some(task_id.to_string()));
    assert_eq!(started_record.step_id, Some(step_id.to_string()));
    assert_eq!(started_record.execution_id, Some(execution_id.to_string()));

    let success_record = &records[1];
    assert_eq!(success_record.status, AuditStatus::Success);
    assert_eq!(success_record.task_id, Some(task_id.to_string()));
    assert_eq!(success_record.step_id, Some(step_id.to_string()));
    assert_eq!(success_record.execution_id, Some(execution_id.to_string()));
}

// ─── GATE-08-B: Layered Action-Result Model ──────────────────────────────────

#[tokio::test]
async fn test_gate_08_b_layered_action_result_model() {
    let (tool_bus, _, tab_mgr, _) = setup_test_harness().await;

    tool_bus.register_with_metadata(
        TabListTool::new(tab_mgr.clone()),
        TabListTool::metadata(),
    ).await;

    let request_id = format!("req_{}", Uuid::new_v4());
    let request = ToolRequest {
        task_id: Some(AgentTaskId::new().to_string()),
        step_id: Some(PlanStepId::new().to_string()),
        execution_id: Some(ToolExecutionId::new().to_string()),
        tool_id: "tab.list".to_string(),
        args: json!({}),
        request_id: request_id.clone(),
        reason: "Observe active tabs".to_string(),
    };

    let ctx = PartialPolicyContext::new(
        "agent_planner",
        "session_01",
        "workspace_01",
        true,
    );

    let response = tool_bus
        .dispatch(request, ctx, CancellationToken::new())
        .await
        .expect("tab.list should succeed");

    // Verify layered action result fields
    assert_eq!(response.status, ExecutionStatus::Success);
    assert!(response.started_at.is_some(), "Must have started_at timestamp");
    assert!(response.completed_at.is_some(), "Must have completed_at timestamp");
    assert!(response.effect_summary.is_some(), "Must have high-level effect summary");
    assert!(response.failure.is_none(), "Successful action must not report failure");

    let summary = response.effect_summary.as_ref().unwrap();
    assert!(summary.contains("Retrieved"), "Effect summary must describe action outcome");
}

// ─── GATE-08-C: Canonical Browser Capability Suite ───────────────────────────

#[tokio::test]
async fn test_gate_08_c_browser_capability_suite() {
    let (tool_bus, _, tab_mgr, session_mgr) = setup_test_harness().await;

    // Register all browser tools with canonical metadata
    tool_bus.register_with_metadata(
        BrowserNavigateTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserNavigateTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserReloadTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserReloadTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserGoBackTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserGoBackTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserGoForwardTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserGoForwardTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserStopTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserStopTool::metadata(),
    ).await;
    tool_bus.register_with_metadata(
        BrowserWaitForNavigationTool::new(tab_mgr.clone(), session_mgr.clone()),
        BrowserWaitForNavigationTool::metadata(),
    ).await;

    // Verify metadata
    let nav_meta = BrowserNavigateTool::metadata();
    assert_eq!(nav_meta.tool_id, "browser.navigate");
    assert_eq!(nav_meta.category, ToolCategory::Browser);
    assert_eq!(nav_meta.idempotency, IdempotencyClassification::Idempotent);
    assert_eq!(nav_meta.retry_policy, RetryPolicy::SafeRetry);
    assert!(nav_meta.declarative_contracts.contains(&DeclarativeContract::NavigationCommitted));

    let wait_meta = BrowserWaitForNavigationTool::metadata();
    assert_eq!(wait_meta.tool_id, "browser.wait_for_navigation");
    assert_eq!(wait_meta.category, ToolCategory::Browser);
    assert_eq!(wait_meta.idempotency, IdempotencyClassification::ReadOnly);
    assert_eq!(wait_meta.tier, PermissionTier::ReadOnly);

    // Dispatching browser.navigate without any active tabs fails closed cleanly with SchemaViolation or NotFound
    let req = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "browser.navigate".to_string(),
        args: json!({ "url": "https://example.com" }),
        request_id: "req_nav_empty".to_string(),
        reason: "Test empty navigation".to_string(),
    };
    let ctx = PartialPolicyContext::new("agent", "sess", "ws", true);
    let result = tool_bus.dispatch(req, ctx, CancellationToken::new()).await;
    assert!(result.is_err(), "Navigation with no active tabs must fail gracefully");
}

// ─── GATE-08-D: Canonical Tab Capability Suite (INV-10 Explicit Binding) ─────

#[tokio::test]
async fn test_gate_08_d_tab_capability_suite_inv10() {
    let (tool_bus, _, tab_mgr, session_mgr) = setup_test_harness().await;

    tool_bus.register_with_metadata(TabCreateTool::new(tab_mgr.clone()), TabCreateTool::metadata()).await;
    tool_bus.register_with_metadata(TabListTool::new(tab_mgr.clone()), TabListTool::metadata()).await;
    tool_bus.register_with_metadata(TabGetStateTool::new(tab_mgr.clone()), TabGetStateTool::metadata()).await;
    tool_bus.register_with_metadata(TabSwitchTool::new(tab_mgr.clone()), TabSwitchTool::metadata()).await;
    tool_bus.register_with_metadata(TabCloseTool::new(tab_mgr.clone()), TabCloseTool::metadata()).await;

    let ctx = PartialPolicyContext::new("agent", "sess", "ws", true);

    // 1. Create personal tab
    let req_create = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.create".to_string(),
        args: json!({ "url": "https://example.com", "profile_id": "personal" }),
        request_id: "req_t1".to_string(),
        reason: "Create personal tab".to_string(),
    };
    let resp1 = tool_bus.dispatch(req_create, ctx.clone(), CancellationToken::new()).await.unwrap();
    let tab1_id = resp1.output["tab_id"].as_str().unwrap().to_string();
    assert_eq!(resp1.output["profile_id"], "personal", "INV-10: Must preserve ProfileId");

    // 2. Create agent sandbox tab
    let req_create_sandbox = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.create".to_string(),
        args: json!({ "url": "about:blank", "profile_id": "agent_sandbox" }),
        request_id: "req_t2".to_string(),
        reason: "Create sandbox tab".to_string(),
    };
    let resp2 = tool_bus.dispatch(req_create_sandbox, ctx.clone(), CancellationToken::new()).await.unwrap();
    let tab2_id = resp2.output["tab_id"].as_str().unwrap().to_string();
    assert_eq!(resp2.output["profile_id"], "agent_sandbox", "INV-10: Must preserve ProfileId");

    // 3. Tab list reflects both tabs and their partition profiles
    let req_list = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.list".to_string(),
        args: json!({}),
        request_id: "req_t3".to_string(),
        reason: "List tabs".to_string(),
    };
    let resp3 = tool_bus.dispatch(req_list, ctx.clone(), CancellationToken::new()).await.unwrap();
    assert_eq!(resp3.output["total"], 2);

    // 4. Tab state reflects explicit tab details
    let req_state = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.get_state".to_string(),
        args: json!({ "tab_id": tab1_id }),
        request_id: "req_t4".to_string(),
        reason: "Query tab state".to_string(),
    };
    let resp4 = tool_bus.dispatch(req_state, ctx.clone(), CancellationToken::new()).await.unwrap();
    assert_eq!(resp4.output["tab_id"], tab1_id);
    assert_eq!(resp4.output["profile_id"], "personal");

    // 5. Switch active tab
    let req_switch = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.switch".to_string(),
        args: json!({ "tab_id": tab2_id }),
        request_id: "req_t5".to_string(),
        reason: "Switch active tab".to_string(),
    };
    let resp5 = tool_bus.dispatch(req_switch, ctx.clone(), CancellationToken::new()).await.unwrap();
    assert_eq!(resp5.output["switched"], true);

    // 6. Close tab
    let req_close = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "tab.close".to_string(),
        args: json!({ "tab_id": tab1_id }),
        request_id: "req_t6".to_string(),
        reason: "Close tab 1".to_string(),
    };
    let resp6 = tool_bus.dispatch(req_close, ctx.clone(), CancellationToken::new()).await.unwrap();
    assert_eq!(resp6.output["closed"], true);
}

// ─── GATE-08-E: Canonical Page Interaction Suite ─────────────────────────────

#[tokio::test]
async fn test_gate_08_e_page_interaction_suite() {
    let (tool_bus, _, tab_mgr, session_mgr) = setup_test_harness().await;

    tool_bus.register_with_metadata(PageClickTool::new(tab_mgr.clone(), session_mgr.clone()), PageClickTool::metadata()).await;
    tool_bus.register_with_metadata(PageTypeTool::new(tab_mgr.clone(), session_mgr.clone()), PageTypeTool::metadata()).await;
    tool_bus.register_with_metadata(PageFillTool::new(tab_mgr.clone(), session_mgr.clone()), PageFillTool::metadata()).await;
    tool_bus.register_with_metadata(PageSelectTool::new(tab_mgr.clone(), session_mgr.clone()), PageSelectTool::metadata()).await;
    tool_bus.register_with_metadata(PagePressKeyTool::new(tab_mgr.clone(), session_mgr.clone()), PagePressKeyTool::metadata()).await;
    tool_bus.register_with_metadata(PageSubmitTool::new(tab_mgr.clone(), session_mgr.clone()), PageSubmitTool::metadata()).await;
    tool_bus.register_with_metadata(PageScrollTool::new(tab_mgr.clone(), session_mgr.clone()), PageScrollTool::metadata()).await;
    tool_bus.register_with_metadata(PageHoverTool::new(tab_mgr.clone(), session_mgr.clone()), PageHoverTool::metadata()).await;

    // Verify metadata for PageClickTool
    let click_meta = PageClickTool::metadata();
    assert_eq!(click_meta.tool_id, "page.click");
    assert_eq!(click_meta.category, ToolCategory::PageInteraction);
    assert_eq!(click_meta.idempotency, IdempotencyClassification::NonIdempotent);
    assert_eq!(click_meta.tier, PermissionTier::StateMutating);

    // Verify schema violation if neither selector nor coords are provided
    let ctx = PartialPolicyContext::new("agent", "sess", "ws", true);
    let invalid_click_req = ToolRequest {
        task_id: None,
        step_id: None,
        execution_id: None,
        tool_id: "page.click".to_string(),
        args: json!({}),
        request_id: "req_invalid_click".to_string(),
        reason: "Test schema validation".to_string(),
    };

    let result = tool_bus.dispatch(invalid_click_req, ctx, CancellationToken::new()).await;
    assert!(result.is_err(), "Missing selector/coords must fail schema validation");
}

// ─── GATE-08-F: Canonical Page Observation Suite ─────────────────────────────

#[tokio::test]
async fn test_gate_08_f_page_observation_suite() {
    let (tool_bus, _, tab_mgr, session_mgr) = setup_test_harness().await;

    tool_bus.register_with_metadata(PageGetTextTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetTextTool::metadata()).await;
    tool_bus.register_with_metadata(PageGetElementTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetElementTool::metadata()).await;
    tool_bus.register_with_metadata(PageGetLinksTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetLinksTool::metadata()).await;
    tool_bus.register_with_metadata(PageGetFormsTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetFormsTool::metadata()).await;
    tool_bus.register_with_metadata(PageGetSemanticSnapshotTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetSemanticSnapshotTool::metadata()).await;
    tool_bus.register_with_metadata(PageGetAccessibilityTreeTool::new(tab_mgr.clone(), session_mgr.clone()), PageGetAccessibilityTreeTool::metadata()).await;

    // Verify all observation tools are marked ReadOnly tier and ReadOnly idempotency
    let obs_tools = vec![
        PageGetTextTool::metadata(),
        PageGetElementTool::metadata(),
        PageGetLinksTool::metadata(),
        PageGetFormsTool::metadata(),
        PageGetSemanticSnapshotTool::metadata(),
        PageGetAccessibilityTreeTool::metadata(),
    ];

    for meta in obs_tools {
        assert_eq!(meta.tier, PermissionTier::ReadOnly, "Tool {} must be ReadOnly tier", meta.tool_id);
        assert_eq!(meta.idempotency, IdempotencyClassification::ReadOnly, "Tool {} must be ReadOnly idempotency", meta.tool_id);
        assert_eq!(meta.category, ToolCategory::PageObservation, "Tool {} must be in PageObservation category", meta.tool_id);
    }
}

// ─── GATE-08-G: Dynamic Capability Registry & Metadata Indexing ──────────────

#[tokio::test]
async fn test_gate_08_g_capability_registry_and_metadata() {
    let registry = CapabilityRegistry::new();

    registry.register(BrowserNavigateTool::metadata()).await;
    registry.register(TabCreateTool::metadata()).await;
    registry.register(PageClickTool::metadata()).await;
    registry.register(PageGetTextTool::metadata()).await;
    registry.register(DownloadStartTool::metadata()).await;

    // Filter by categories
    let browser_tools = registry.filter_by_category(ToolCategory::Browser).await;
    assert_eq!(browser_tools.len(), 1);
    assert_eq!(browser_tools[0].tool_id, "browser.navigate");

    let tab_tools = registry.filter_by_category(ToolCategory::Tab).await;
    assert_eq!(tab_tools.len(), 1);
    assert_eq!(tab_tools[0].tool_id, "tab.create");

    let page_interact_tools = registry.filter_by_category(ToolCategory::PageInteraction).await;
    assert_eq!(page_interact_tools.len(), 1);
    assert_eq!(page_interact_tools[0].tool_id, "page.click");

    let page_obs_tools = registry.filter_by_category(ToolCategory::PageObservation).await;
    assert_eq!(page_obs_tools.len(), 1);
    assert_eq!(page_obs_tools[0].tool_id, "page.get_text");

    let dl_tools = registry.filter_by_category(ToolCategory::Download).await;
    assert_eq!(dl_tools.len(), 1);
    assert_eq!(dl_tools[0].tool_id, "download.start");
}

// ─── GATE-08-H: Strict Prohibition of Generic eval_js Backdoor ───────────────

#[tokio::test]
async fn test_gate_08_h_prohibition_of_generic_eval_js() {
    let (tool_bus, audit_sink, tab_mgr, session_mgr) = setup_test_harness().await;

    // Register devtools.runtime.evaluate marked as developer_only()
    let eval_meta = ToolMetadata::new(
        "devtools.runtime.evaluate",
        ToolCategory::Developer,
        PermissionTier::StateMutating,
        json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "format": "uuid" },
                "expression": { "type": "string" }
            },
            "required": ["tab_id", "expression"]
        }),
    )
    .with_description("Interactive DevTools JavaScript REPL evaluation (Developer Only)")
    .developer_only();

    tool_bus.register_with_metadata(
        RuntimeEvaluateTool::new(tab_mgr.clone(), session_mgr.clone()),
        eval_meta.clone(),
    ).await;

    // 1. Verify agent capability discovery completely filters out devtools.runtime.evaluate
    let registry = tool_bus.capability_registry();
    let agent_tools = registry.query_for_agent(false).await;
    assert!(
        !agent_tools.iter().any(|t| t.tool_id == "devtools.runtime.evaluate"),
        "GATE-08-H: devtools.runtime.evaluate must NEVER appear in agent capability discovery"
    );

    // But full query (for human developer) does include it
    let all_tools = registry.list_all().await;
    assert!(
        all_tools.iter().any(|t| t.tool_id == "devtools.runtime.evaluate"),
        "Human developer query must include devtools.runtime.evaluate"
    );

    // 2. Verify that if an unescalated autonomous agent attempts to execute devtools.runtime.evaluate,
    // ToolBus strictly blocks execution with ToolError::DeveloperToolProhibited (fail-closed).
    let agent_req = ToolRequest {
        task_id: Some(AgentTaskId::new().to_string()),
        step_id: Some(PlanStepId::new().to_string()),
        execution_id: Some(ToolExecutionId::new().to_string()),
        tool_id: "devtools.runtime.evaluate".to_string(),
        args: json!({
            "tab_id": Uuid::new_v4().to_string(),
            "expression": "document.cookie"
        }),
        request_id: "req_agent_eval_attempt".to_string(),
        reason: "Autonomous extraction of cookies".to_string(),
    };

    let agent_ctx = PartialPolicyContext::new(
        "agent_planner",
        "agent_session_01",
        "workspace_agent",
        true, // Even with session_granted, developer-only tools are strictly forbidden for agent_planner
    );

    let err = tool_bus
        .dispatch(agent_req, agent_ctx, CancellationToken::new())
        .await
        .expect_err("Autonomous agent MUST be prevented from running devtools.runtime.evaluate");

    match err {
        ToolError::DeveloperToolProhibited { tool_id } => {
            assert_eq!(tool_id, "devtools.runtime.evaluate");
        }
        other => panic!("Expected ToolError::DeveloperToolProhibited, got {other:?}"),
    }

    // 3. Verify fail-closed audit log recorded the denial
    let records = audit_sink.get_records().await;
    let denial = records
        .iter()
        .find(|r| r.request_id == "req_agent_eval_attempt" && r.status == AuditStatus::Denied);
    assert!(denial.is_some(), "Fail-closed audit denial record must be committed");
    assert_eq!(denial.unwrap().error_code, Some("DEVELOPER_TOOL_PROHIBITED_FOR_AGENT".to_string()));
}
