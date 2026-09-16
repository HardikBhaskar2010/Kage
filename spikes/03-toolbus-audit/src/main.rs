use async_trait::async_trait;
use kage_spike_toolbus_audit::*;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// --- Sample Tool 1: DOM Inspect (Tier 1 Read-Only) ---
struct DomInspectTool;

#[async_trait]
impl KageTool for DomInspectTool {
    fn name(&self) -> &'static str {
        "dom.inspect"
    }
    fn description(&self) -> &'static str {
        "Inspects DOM element node by CSS selector"
    }
    fn tier(&self) -> PermissionTier {
        PermissionTier::Tier1ReadOnlyPassive
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["selector"],
            "properties": {
                "selector": { "type": "string" },
                "depth": { "type": "integer" }
            }
        })
    }
    async fn execute(&self, args: Value, _cancel: CancellationToken) -> Result<Value, ToolError> {
        let selector = args["selector"].as_str().unwrap_or("*");
        Ok(json!({
            "nodeId": 42,
            "nodeName": "BUTTON",
            "selector": selector,
            "attributes": {
                "class": "liquid-glass-btn",
                "aria-label": "Submit Form"
            }
        }))
    }
}

// --- Sample Tool 2: Page Click (Tier 2 State-Mutating) ---
struct PageClickTool;

#[async_trait]
impl KageTool for PageClickTool {
    fn name(&self) -> &'static str {
        "page.click"
    }
    fn description(&self) -> &'static str {
        "Simulates a click on a target DOM element"
    }
    fn tier(&self) -> PermissionTier {
        PermissionTier::Tier2StateMutating
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["selector"],
            "properties": {
                "selector": { "type": "string" }
            }
        })
    }
    async fn execute(&self, args: Value, cancel: CancellationToken) -> Result<Value, ToolError> {
        // Bounded cancellation point
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let selector = args["selector"].as_str().unwrap();
        Ok(json!({
            "status": "clicked",
            "target": selector,
            "timestamp": "2026-09-17T00:00:00Z"
        }))
    }
}

// --- Sample Tool 3: Network Replay (Tier 3 External High Risk) ---
struct NetworkReplayTool;

#[async_trait]
impl KageTool for NetworkReplayTool {
    fn name(&self) -> &'static str {
        "network.replay"
    }
    fn description(&self) -> &'static str {
        "Replays an HTTP request to external endpoint"
    }
    fn tier(&self) -> PermissionTier {
        PermissionTier::Tier3ExternalHighRisk
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url", "method"],
            "properties": {
                "url": { "type": "string" },
                "method": { "type": "string" },
                "headers": { "type": "object" },
                "body": { "type": "object" }
            }
        })
    }
    async fn execute(&self, args: Value, _cancel: CancellationToken) -> Result<Value, ToolError> {
        let url = args["url"].as_str().unwrap();
        Ok(json!({
            "status": 200,
            "replayed_url": url,
            "echo_headers": args.get("headers"),
            "response": "Success"
        }))
    }
}

// --- Sample Tool 4: System Exec (Tier 4 Dangerous / Blocked) ---
struct SystemExecTool;

#[async_trait]
impl KageTool for SystemExecTool {
    fn name(&self) -> &'static str {
        "system.exec"
    }
    fn description(&self) -> &'static str {
        "Executes a host system command (PROHIBITED)"
    }
    fn tier(&self) -> PermissionTier {
        PermissionTier::Tier4DangerousBlocked
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string" }
            }
        })
    }
    async fn execute(&self, _args: Value, _cancel: CancellationToken) -> Result<Value, ToolError> {
        panic!("FATAL SECURITY BREACH: Tier 4 tool executed directly!");
    }
}

// --- Sample Tool 5: Long Running (For Cancellation Verification) ---
struct LongRunningTool;

#[async_trait]
impl KageTool for LongRunningTool {
    fn name(&self) -> &'static str {
        "test.long_running"
    }
    fn description(&self) -> &'static str {
        "Simulates a long-running operation with bounded cancellation checkpoints"
    }
    fn tier(&self) -> PermissionTier {
        PermissionTier::Tier1ReadOnlyPassive
    }
    fn schema(&self) -> Value {
        json!({ "type": "object" })
    }
    async fn execute(&self, _args: Value, cancel: CancellationToken) -> Result<Value, ToolError> {
        for step in 1..=100 {
            // Bounded cooperative cancellation checkpoint
            if cancel.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            if step > 5 {
                break;
            }
        }
        Ok(json!({ "completed_steps": 5 }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("  KAGE TECHNICAL SPIKE 3: Tool Bus + Policy + Audit Integrity");
    println!("============================================================");

    let audit_logger = Arc::new(AuditLogger::new_in_memory()?);
    let mut tool_bus = ToolBus::new(audit_logger.clone());

    // Register all test tools
    tool_bus.register_tool(Arc::new(DomInspectTool));
    tool_bus.register_tool(Arc::new(PageClickTool));
    tool_bus.register_tool(Arc::new(NetworkReplayTool));
    tool_bus.register_tool(Arc::new(SystemExecTool));
    tool_bus.register_tool(Arc::new(LongRunningTool));

    let ai_caller = CallerContext {
        caller_id: "kage-ai-agent-v1".to_string(),
        caller_type: CallerType::AiAgent,
        session_id: "sess_test_123".to_string(),
        workspace_id: "ws_default".to_string(),
        origin: Some("https://example.com".to_string()),
    };

    // --- TEST 1: Tier 1 Auto-Allow ---
    println!("\n[1/7] Testing Tier 1 (Read-Only Passive): 'dom.inspect'...");
    let cancel = CancellationToken::new();
    let res = tool_bus
        .dispatch(
            "dom.inspect",
            json!({ "selector": "button#submit" }),
            &ai_caller,
            None,
            cancel,
        )
        .await?;
    println!("  -> Result: {}", res);
    assert_eq!(res["nodeName"], "BUTTON");
    println!("  -> PASS: Tier 1 automatically allowed");

    // --- TEST 2: Tier 2 Session Grant Requirement ---
    println!("\n[2/7] Testing Tier 2 (State-Mutating): 'page.click' before session grant...");
    let cancel = CancellationToken::new();
    let res = tool_bus
        .dispatch("page.click", json!({ "selector": "#login" }), &ai_caller, None, cancel)
        .await;
    match res {
        Err(ToolError::PermissionDenied { reason, .. }) => {
            println!("  -> Correctly blocked prior to session grant: {}", reason);
        }
        _ => panic!("Expected permission prompt requirement for Tier 2"),
    }

    println!("  -> Granting session permission for 'page.click' in session '{}'...", ai_caller.session_id);
    tool_bus
        .policy_engine()
        .grant_session_permission(&ai_caller.session_id, "page.click");

    let cancel = CancellationToken::new();
    let res = tool_bus
        .dispatch("page.click", json!({ "selector": "#login" }), &ai_caller, None, cancel)
        .await?;
    println!("  -> Result with session grant: {}", res);
    assert_eq!(res["status"], "clicked");
    println!("  -> PASS: Tier 2 granted via session permission");

    // --- TEST 3: Tier 3 Prompt Token Requirement ---
    println!("\n[3/7] Testing Tier 3 (External High-Risk): 'network.replay'...");
    let cancel = CancellationToken::new();
    let unapproved = tool_bus
        .dispatch(
            "network.replay",
            json!({ "url": "https://api.example.com/checkout", "method": "POST" }),
            &ai_caller,
            None,
            cancel,
        )
        .await;
    assert!(unapproved.is_err(), "Tier 3 must require approval token");
    println!("  -> Unapproved call blocked as expected.");

    let cancel = CancellationToken::new();
    let approved = tool_bus
        .dispatch(
            "network.replay",
            json!({ "url": "https://api.example.com/checkout", "method": "POST" }),
            &ai_caller,
            Some("APPROVED:network.replay:user_confirmed"),
            cancel,
        )
        .await?;
    println!("  -> Result with explicit user token: {}", approved);
    assert_eq!(approved["status"], 200);
    println!("  -> PASS: Tier 3 executed only with explicit approval token");

    // --- TEST 4: Tier 4 Hard Block ---
    println!("\n[4/7] Testing Tier 4 (Dangerous / System): 'system.exec'...");
    let cancel = CancellationToken::new();
    let blocked = tool_bus
        .dispatch(
            "system.exec",
            json!({ "command": "rm -rf /" }),
            &ai_caller,
            Some("APPROVED:bypass"), // Even with token, Tier 4 must be blocked!
            cancel,
        )
        .await;
    match blocked {
        Err(ToolError::PermissionDenied { tier, reason, .. }) => {
            println!("  -> HARD BLOCKED ({}): {}", tier, reason);
            assert_eq!(tier, PermissionTier::Tier4DangerousBlocked);
        }
        _ => panic!("FATAL: Tier 4 was not blocked!"),
    }
    println!("  -> PASS: Tier 4 unconditionally blocked");

    // --- TEST 5: Deep Recursive Secret Redaction ---
    println!("\n[5/7] Testing Deep Recursive Secret Redaction (Objects, Arrays & Headers)...");
    let raw_secret_token = "ghp_9876543210abcdef9876543210abcdef";
    let raw_api_key = "sk-live-0123456789abcdef0123456789";
    let raw_password = "SuperSecretAdminPassword123!";

    let payload_with_secrets = json!({
        "url": "https://api.example.com/login",
        "method": "POST",
        "headers": {
            "Authorization": format!("Bearer {}", raw_secret_token),
            "X-Api-Key": raw_api_key
        },
        "body": {
            "auth": {
                "password": raw_password
            },
            "history": [
                { "token": "temp_12345" },
                { "Authorization": "Bearer sk-nested-999" }
            ]
        }
    });

    let cancel = CancellationToken::new();
    let _ = tool_bus
        .dispatch(
            "network.replay",
            payload_with_secrets,
            &ai_caller,
            Some("APPROVED:network.replay:test"),
            cancel,
        )
        .await?;

    // Verify audit log has ZERO instances of the raw secrets!
    let all_records = audit_logger.get_all_records()?;
    for record in &all_records {
        assert!(
            !record.sanitized_args.contains(raw_secret_token),
            "SECURITY VIOLATION: Raw bearer token leaked into audit log!"
        );
        assert!(
            !record.sanitized_args.contains(raw_api_key),
            "SECURITY VIOLATION: Raw API key leaked into audit log!"
        );
        assert!(
            !record.sanitized_args.contains(raw_password),
            "SECURITY VIOLATION: Raw password leaked into audit log!"
        );
    }
    println!("  -> Verified: All deep nested secrets were redacted before entering the audit DB");
    println!("  -> PASS: Zero raw secret leakage in audit records");

    // --- TEST 6: Bounded Cooperative Cancellation ---
    println!("\n[6/7] Testing Bounded Cooperative Cancellation...");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    
    // Trigger cancellation after 20ms (before tool finishes)
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        cancel_clone.cancel();
    });

    let cancelled_res = tool_bus
        .dispatch("test.long_running", json!({}), &ai_caller, None, cancel)
        .await;

    match cancelled_res {
        Err(ToolError::Cancelled) => {
            println!("  -> Tool cleanly terminated at cooperative cancellation point");
        }
        other => panic!("Expected ToolError::Cancelled, got {:?}", other),
    }
    println!("  -> PASS: Bounded cooperative cancellation verified");

    // --- TEST 7: Cryptographic Hash-Chain Integrity Verification ---
    println!("\n[7/7] Testing SHA-256 Tamper-Evident Hash-Chain Integrity...");
    let integrity_result = audit_logger.verify_integrity()?;
    match integrity_result {
        VerificationResult::Pass { total_records_verified } => {
            println!("  -> Hash chain fully intact! Verified {} consecutive audit records", total_records_verified);
            assert!(total_records_verified >= 6);
        }
        VerificationResult::IntegrityFailure { record_id, reason } => {
            panic!("Integrity failure at record {}: {}", record_id, reason);
        }
    }
    println!("  -> PASS: Audit chain verified tamper-evident");

    println!("\n============================================================");
    println!("  SUMMARY OF APPEND-ONLY AUDIT DB (All Hash-Chained):");
    println!("============================================================");
    for r in audit_logger.get_all_records()? {
        println!(
            "  [{}] {:<18} | Tier {} | {:<18} | Chain: {}..{}",
            r.id,
            r.tool_name,
            r.tier,
            r.status,
            &r.chain_hash[..8],
            &r.chain_hash[r.chain_hash.len() - 6..]
        );
    }

    println!("\n[ALL SPIKE 3 TESTS PASSED SUCCESSFULLY] 🚀");
    Ok(())
}
