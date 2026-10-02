//! Phase 7 Profiles, Storage & Permission State Integration Test Suite.
//!
//! Validates the multi-profile isolation, ephemeral sandbox wipe, centralized origin permissions,
//! and fail-closed Credential Broker boundary (INV-04, INV-05, INV-06, INV-10):
//! - **GATE-07-A**: Profile lifecycle & manifest persistence (`profiles.json`)
//! - **GATE-07-B**: Ephemeral agent sandbox wipe & physical directory non-existence check
//! - **GATE-07-C**: Origin permission matrix, URL normalization & persistent rules
//! - **GATE-07-D**: Credential Broker boundary with fail-closed two-stage audit commitment (`INV-06`)
//! - **GATE-07-E**: Tab & Profile binding invariant (`INV-10`) across multi-tab workloads

use std::sync::Arc;
use tempfile::TempDir;

use kage_browser::permission::{PermissionDecision, PermissionManager, PermissionType};
use kage_browser::profile::{
    record_session_escalation_intent, record_session_escalation_outcome, ProfileKind,
    ProfileMetadata, ProfileManager,
};
use kage_browser::{BrowserError, BrowserEventBus, ProfileId, TabManager};
use kage_core::audit::{AuditReader, AuditSink, AuditVerifier};
use kage_storage::AuditDb;

// ─── GATE-07-A: Profile Lifecycle & Manifest Persistence ─────────────────────

#[tokio::test]
async fn test_gate_07_a_profile_lifecycle_and_persistence() {
    let temp_dir = TempDir::new().expect("Create temp dir");
    let base_profiles_dir = temp_dir.path().join("profiles");
    let base_temp_dir = temp_dir.path().join("temp");

    // 1. Initialize ProfileManager and verify default profiles
    let pm = ProfileManager::with_custom_dirs(base_profiles_dir.clone(), base_temp_dir.clone());
    let default_metadata = pm.list_metadata().await;
    assert_eq!(default_metadata.len(), 3, "Must have personal, work, and agent_sandbox by default");

    let personal_meta = pm
        .get_metadata(&ProfileId::personal())
        .await
        .expect("Personal profile metadata must exist");
    assert_eq!(personal_meta.kind, ProfileKind::Personal);
    assert!(!personal_meta.is_ephemeral);

    // 2. Register a new custom profile
    let custom_id = ProfileId("work_engineering".to_string());
    let mut custom_meta = ProfileMetadata::new(custom_id.clone(), "Engineering Workspace", ProfileKind::Work);
    custom_meta.color = "#10B981".to_string();
    custom_meta.icon = "terminal".to_string();

    let created = pm
        .create_profile(custom_meta)
        .await
        .expect("Create custom profile");

    assert_eq!(created.id, custom_id);
    assert_eq!(created.metadata.name, "Engineering Workspace");
    assert_eq!(created.metadata.kind, ProfileKind::Work);
    assert_eq!(created.metadata.color, "#10B981");

    // Verify manifest file exists on disk
    let manifest_path = base_profiles_dir.join("profiles.json");
    assert!(manifest_path.exists(), "Manifest profiles.json must be written to disk");

    // 3. Re-initialize a second ProfileManager from the same disk directories (Persistence check)
    let pm2 = ProfileManager::with_custom_dirs(base_profiles_dir.clone(), base_temp_dir.clone());
    let reloaded_meta = pm2
        .get_metadata(&custom_id)
        .await
        .expect("Custom profile must persist across restart");
    assert_eq!(reloaded_meta.name, "Engineering Workspace");
    assert_eq!(reloaded_meta.color, "#10B981");
    assert_eq!(reloaded_meta.kind, ProfileKind::Work);

    // 4. Invariant: Default personal profile cannot be deleted
    let delete_personal_err = pm2
        .delete_profile(&ProfileId::personal())
        .await
        .expect_err("Deleting personal profile must fail");
    match delete_personal_err {
        BrowserError::ProfileError(msg) => {
            assert!(msg.contains("Cannot delete the default Personal profile"));
        }
        other => panic!("Expected ProfileError, got {other:?}"),
    }

    // 5. Deleting custom profile succeeds and removes metadata
    pm2.delete_profile(&custom_id)
        .await
        .expect("Delete custom profile");
    assert!(
        pm2.get_metadata(&custom_id).await.is_none(),
        "Deleted profile metadata must not be found"
    );
}

// ─── GATE-07-B: Ephemeral Agent Sandbox Measurable Wipe ──────────────────────

#[tokio::test]
async fn test_gate_07_b_ephemeral_agent_sandbox_wipe() {
    let temp_dir = TempDir::new().expect("Create temp dir");
    let base_profiles_dir = temp_dir.path().join("profiles");
    let base_temp_dir = temp_dir.path().join("temp");

    let pm = ProfileManager::with_custom_dirs(base_profiles_dir, base_temp_dir);

    // 1. Create an ephemeral agent sandbox profile
    let sandbox_id = ProfileId("agent_sandbox_ephemeral_test".to_string());
    let sandbox_meta = ProfileMetadata::new(sandbox_id.clone(), "Disposable Agent Sandbox", ProfileKind::AgentSandbox);

    let created_profile = pm
        .create_profile(sandbox_meta)
        .await
        .expect("Create sandbox profile");

    assert!(created_profile.metadata.is_ephemeral, "Agent sandbox must be flagged ephemeral");

    // 2. Locate physical disk directories for this profile
    let profile = pm
        .get_or_create(&sandbox_id)
        .await
        .expect("Profile instance must exist");

    let root_dir = profile.root_dir.clone();
    let cache_dir = profile.cache_dir.clone();
    let cookie_dir = profile.cookie_store_dir.clone();

    // Verify directories were created
    assert!(root_dir.exists(), "Root dir must exist on disk");
    assert!(cache_dir.exists(), "Cache dir must exist on disk");
    assert!(cookie_dir.exists(), "Cookie dir must exist on disk");

    // 3. Write simulated session artifact markers (cookies, auth tokens, cache files)
    let marker_cookie = cookie_dir.join("Cookies.sqlite");
    let marker_token = root_dir.join("auth_tokens.json");
    let marker_cache = cache_dir.join("cached_asset.bin");

    std::fs::write(&marker_cookie, b"AGENT_SECRET_SESSION_COOKIE").expect("write cookie");
    std::fs::write(&marker_token, b"{\"bearer\":\"secret123\"}").expect("write token");
    std::fs::write(&marker_cache, b"PRECOMPILED_RESOURCE").expect("write cache");

    assert!(marker_cookie.exists());
    assert!(marker_token.exists());
    assert!(marker_cache.exists());

    // 4. Invoke physical wipe (INV-06 / Clean Sandbox Guarantee)
    profile.wipe_disk().expect("Physical disk wipe must succeed");

    // 5. Measure and verify physical absence of directories and files
    assert!(!root_dir.exists(), "Root directory must be physically deleted from disk");
    assert!(!cache_dir.exists(), "Cache directory must be physically deleted from disk");
    assert!(!cookie_dir.exists(), "Cookie directory must be physically deleted from disk");
    assert!(!marker_cookie.exists(), "Marker cookie file must not exist");
    assert!(!marker_token.exists(), "Marker token file must not exist");

    // 6. Test manager-level ephemeral purge
    pm.wipe_ephemeral_profile(&sandbox_id)
        .await
        .expect("Manager-level wipe");
}

// ─── GATE-07-C: Origin Permission Matrix & Normalization ────────────────────

#[tokio::test]
async fn test_gate_07_c_origin_permission_matrix_and_normalization() {
    let temp_dir = TempDir::new().expect("Create temp dir");
    let permissions_dir = temp_dir.path().join("permissions");
    std::fs::create_dir_all(&permissions_dir).expect("create dir");

    let perm_manager = PermissionManager::new(permissions_dir.clone());

    let personal_profile = ProfileId::personal();
    let sandbox_profile = ProfileId::agent_sandbox();

    // 1. Query defaults based on profile isolation
    // Personal profile defaults to interactive prompt for hardware/sensors
    let personal_geo = perm_manager
        .query(&personal_profile, "https://maps.google.com", PermissionType::Geolocation)
        .await;
    assert_eq!(personal_geo, PermissionDecision::Prompt);

    let personal_cam = perm_manager
        .query(&personal_profile, "https://meet.google.com", PermissionType::Camera)
        .await;
    assert_eq!(personal_cam, PermissionDecision::Prompt);

    // Agent sandbox profile denies sensors and ambient authority by default
    let sandbox_geo = perm_manager
        .query(&sandbox_profile, "https://maps.google.com", PermissionType::Geolocation)
        .await;
    assert_eq!(sandbox_geo, PermissionDecision::Deny, "Sandbox must deny geolocation by default");

    let sandbox_cam = perm_manager
        .query(&sandbox_profile, "https://meet.google.com", PermissionType::Camera)
        .await;
    assert_eq!(sandbox_cam, PermissionDecision::Deny, "Sandbox must deny camera by default");

    // 2. Origin normalization test: Case insensitivity, port stripping, and path stripping
    let raw_origin = "  HTTPS://My-App.Example.COM/deep/nested/path/  ";
    perm_manager
        .set(
            personal_profile.clone(),
            raw_origin,
            PermissionType::ClipboardRead,
            PermissionDecision::Allow,
        )
        .await
        .expect("Set permission rule");

    // Query with normalized form
    let decision1 = perm_manager
        .query(&personal_profile, "https://my-app.example.com", PermissionType::ClipboardRead)
        .await;
    assert_eq!(decision1, PermissionDecision::Allow);

    // Query with trailing slash
    let decision2 = perm_manager
        .query(&personal_profile, "https://my-app.example.com/", PermissionType::ClipboardRead)
        .await;
    assert_eq!(decision2, PermissionDecision::Allow);

    // Verify origin isolation: Sandbox profile still resolves default for the same origin
    let sandbox_decision = perm_manager
        .query(&sandbox_profile, "https://my-app.example.com", PermissionType::ClipboardRead)
        .await;
    assert_eq!(sandbox_decision, PermissionDecision::Prompt);

    // 3. Persistence verification across restart
    let perm_manager2 = PermissionManager::new(permissions_dir);
    let reloaded_decision = perm_manager2
        .query(&personal_profile, "https://my-app.example.com", PermissionType::ClipboardRead)
        .await;
    assert_eq!(reloaded_decision, PermissionDecision::Allow, "Permission rule must survive restart");
}

// ─── GATE-07-D: Credential Broker Escalation & Two-Stage Audit Commit ────────

#[tokio::test]
async fn test_gate_07_d_credential_broker_escalation_and_audit() {
    let audit_db = Arc::new(AuditDb::open_in_memory().expect("open audit db"));
    let audit_sink: Arc<dyn AuditSink> = audit_db.clone();
    let tab_id = kage_browser::TabId::new();
    let source_profile = ProfileId::agent_sandbox();
    let target_profile = ProfileId::personal();
    let req_id = "req_esc_001";
    let reason = "Agent requires personal GitHub session to open PR";

    // 1. Two-stage audit commitment: Record Intent (Started)
    let seq_intent = record_session_escalation_intent(
        &audit_sink,
        tab_id,
        &source_profile,
        &target_profile,
        req_id,
        reason,
    )
    .await
    .expect("Record escalation intent");

    assert!(seq_intent > 0);

    // 2. Case A: User rejects escalation -> Denied status committed
    let seq_denied = record_session_escalation_outcome(
        &audit_sink,
        tab_id,
        &target_profile,
        req_id,
        false, // rejected
        150,
    )
    .await
    .expect("Record escalation denial");

    assert!(seq_denied > seq_intent);

    // 3. Case B: Another request is authorized by user
    let req_id_2 = "req_esc_002";
    let seq_intent_2 = record_session_escalation_intent(
        &audit_sink,
        tab_id,
        &source_profile,
        &target_profile,
        req_id_2,
        reason,
    )
    .await
    .expect("Record intent 2");

    let seq_approved = record_session_escalation_outcome(
        &audit_sink,
        tab_id,
        &target_profile,
        req_id_2,
        true, // approved
        220,
    )
    .await
    .expect("Record escalation approval");

    assert!(seq_approved > seq_intent_2);

    // 4. Verify cryptographic SHA-256 hash chaining of audit trail
    let chain_audit_db = Arc::new(AuditDb::open_in_memory().expect("chain audit db"));
    let chain_sink: Arc<dyn AuditSink> = chain_audit_db.clone();

    record_session_escalation_intent(
        &chain_sink,
        tab_id,
        &source_profile,
        &target_profile,
        "chain_req_01",
        "Reason 1",
    )
    .await
    .unwrap();

    record_session_escalation_outcome(
        &chain_sink,
        tab_id,
        &target_profile,
        "chain_req_01",
        true,
        50,
    )
    .await
    .unwrap();

    // Verify sequence & records
    let recent = AuditReader::get_recent_records(&*chain_audit_db, 10)
        .await
        .expect("Fetch records");

    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].status, "success");
    assert_eq!(recent[0].tool_id, "profile.escalate_session");
    assert_eq!(recent[1].status, "started");
    assert_eq!(recent[1].tool_id, "profile.escalate_session");

    // Cryptographic audit chain verification
    AuditVerifier::verify_chain(&*chain_audit_db)
        .await
        .expect("Cryptographic hash chain must be 100% valid and untampered");
}

// ─── GATE-07-E: Tab & Profile Binding Invariant (INV-10) ──────────────────────

#[tokio::test]
async fn test_gate_07_e_tab_profile_binding_inv_10() {
    let temp_dir = TempDir::new().expect("Create temp dir");
    let pm = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(pm.clone(), event_bus);

    let personal_id = ProfileId::personal();
    let work_id = ProfileId::work();
    let sandbox_id = ProfileId::agent_sandbox();

    // 1. Create tab 1 in Personal profile
    let tab_1 = tab_manager
        .create_tab(personal_id.clone(), "https://personal-app.com")
        .await
        .expect("Create tab 1");

    // 2. Create tab 2 in Work profile
    let tab_2 = tab_manager
        .create_tab(work_id.clone(), "https://work-portal.internal")
        .await
        .expect("Create tab 2");

    // 3. Create tab 3 in Agent Sandbox profile
    let tab_3 = tab_manager
        .create_tab(sandbox_id.clone(), "https://untrusted-site.org")
        .await
        .expect("Create tab 3");

    // 4. Verify explicit (TabId, ProfileId) relationships (INV-10)
    let t1 = tab_manager.get_tab(tab_1).await.expect("tab 1");
    let t2 = tab_manager.get_tab(tab_2).await.expect("tab 2");
    let t3 = tab_manager.get_tab(tab_3).await.expect("tab 3");

    assert_eq!(t1.profile_id, personal_id);
    assert_eq!(t2.profile_id, work_id);
    assert_eq!(t3.profile_id, sandbox_id);

    {
        let id1 = t1.identity.read().await;
        assert_eq!(id1.profile_id, personal_id);
        let id2 = t2.identity.read().await;
        assert_eq!(id2.profile_id, work_id);
        let id3 = t3.identity.read().await;
        assert_eq!(id3.profile_id, sandbox_id);
    }

    // 5. Verify TabSummary explicitly reports correct ProfileId
    assert_eq!(t1.summary().await.profile_id(), &personal_id);
    assert_eq!(t2.summary().await.profile_id(), &work_id);
    assert_eq!(t3.summary().await.profile_id(), &sandbox_id);

    // 6. Test controlled session escalation of a tab
    tab_manager
        .escalate_tab_profile(tab_3, personal_id.clone())
        .await
        .expect("Escalate tab profile");

    let t3_escalated = tab_manager.get_tab(tab_3).await.expect("tab 3 after escalation");
    assert_eq!(t3_escalated.profile_id, personal_id);
    assert_eq!(t3_escalated.identity.read().await.profile_id, personal_id);
    assert_eq!(t3_escalated.summary().await.profile_id(), &personal_id);
}
