//! Integration test suite for Phase 3 Browser Lifecycle, Tab Identity, & Profiles.
//!
//! Enforces:
//! - `INV-10`: Every browser tab has an explicit (TabId, ProfileId, Option<CefBrowserId>) relationship.
//! - `INV-11A`: Renderer process failure cannot grant authority (Renderer crash fails closed).
//! - `INV-12`: Browser and surface identity are explicit and never inferred:
//!   - Authoritative browser identity: (TabId, ProfileId, Option<CefBrowserId>)
//!   - CDP target binding: TargetId (associated with browser identity, not identity itself)
//!   - CDP session: SessionId (ephemeral attachment handle, not identity)
//!   - BrowserSurface binding: (TabId, BrowserSurfaceId) (decoupled from tab identity)

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::oneshot;

use kage_browser::{
    BrowserError, BrowserEventBus, BrowserEventKind, BrowserEventProducer, CdpBinding, CdpSession,
    CefTerminationStatus, NavigationSource, NavigationState, ProfileId, ProfileKind,
    ProfileManager, RendererCrashDiagnostics, RendererTerminationStatus, TabHealth, TabId,
    TabLifecycle, TabManager,
};

#[tokio::test]
async fn test_tab_creation_and_lifecycle_aware_identity_inv_10_and_12() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let mut rx = event_bus.subscribe();

    let tab_manager = TabManager::new(profile_manager, event_bus);

    let profile_id = ProfileId::personal();
    let tab_id = tab_manager
        .create_tab(profile_id.clone(), "https://example.com")
        .await
        .expect("Tab creation must succeed");

    // Invariant 12 Pre-CEF: Authoritative browser identity is (TabId, ProfileId), cef_browser_id is None
    let tab = tab_manager.get_tab(tab_id).await.expect("Tab must exist");
    assert_eq!(tab.id, tab_id);
    assert_eq!(tab.profile_id, profile_id);

    {
        let ident = tab.identity.read().await;
        assert_eq!(ident.tab_id, tab_id);
        assert_eq!(ident.profile_id, profile_id);
        assert_eq!(ident.cef_browser_id, None, "Pre-CEF: cef_browser_id must be None");
        assert!(!ident.is_bound_to_cef());
    }

    // Invariant 10: Orthogonal state initialization
    let summary = tab.summary().await;
    assert_eq!(summary.profile_id(), &profile_id);
    assert_eq!(summary.url, "https://example.com");
    assert_eq!(summary.lifecycle, TabLifecycle::Active); // First tab is active
    assert_eq!(summary.navigation, NavigationState::Idle);
    assert_eq!(summary.health, TabHealth::Healthy);
    assert_eq!(summary.cef_browser_id(), None);
    assert!(summary.cdp.is_none());

    // Invariant 12 Post-CEF: Bind real CEF browser ID
    tab_manager
        .bind_cef_browser(tab_id, 101)
        .await
        .expect("Binding CEF browser must succeed");

    {
        let ident = tab.identity.read().await;
        assert_eq!(ident.cef_browser_id, Some(101));
        assert!(ident.is_bound_to_cef());
    }

    // Invariant 12 Post-CDP Discovery: CDP Target discovery attaches target_id (association, not identity)
    {
        let mut cdp_guard = tab.cdp.write().await;
        *cdp_guard = Some(CdpBinding::new("target_cef_101_discovery"));
    }
    assert_eq!(
        tab.cdp.read().await.as_ref().unwrap().target_id,
        "target_cef_101_discovery"
    );

    // Invariant 12 Post-CDP Attachment: CdpSession is an ephemeral connection handle
    let session = CdpSession::new("sess_agent_12345");
    assert_eq!(session.session_id, "sess_agent_12345");

    // Sequenced event verification with provenance
    let event = rx.recv().await.expect("Must receive TabCreated event");
    assert_eq!(event.sequence, 1, "First event must have sequence 1");
    assert_eq!(event.producer, BrowserEventProducer::BrowserControl);
    assert!(event.monotonic_time_ns > 0);
    assert!(event.wall_time_ms > 0);
    assert_eq!(event.tab_id, Some(tab_id));
    match event.kind {
        BrowserEventKind::TabCreated {
            profile_id: event_profile,
            url,
        } => {
            assert_eq!(event_profile, profile_id);
            assert_eq!(url, "https://example.com");
        }
        other => panic!("Unexpected event: {:?}", other),
    }
}

#[tokio::test]
async fn test_profile_storage_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path().join("profiles");
    let temp_profiles_dir = temp_dir.path().join("temp");

    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        base_dir.clone(),
        temp_profiles_dir.clone(),
    ));

    let personal = profile_manager
        .get_or_create(&ProfileId::personal())
        .await
        .unwrap();
    let work = profile_manager
        .get_or_create(&ProfileId::work())
        .await
        .unwrap();
    let agent = profile_manager
        .get_or_create(&ProfileId::agent_sandbox())
        .await
        .unwrap();
    let temp = profile_manager.create_temporary().await.unwrap();

    // Verify storage directories are physically distinct
    assert_ne!(personal.root_dir, work.root_dir);
    assert_ne!(work.root_dir, agent.root_dir);
    assert_ne!(agent.root_dir, temp.root_dir);

    assert_eq!(personal.kind, ProfileKind::Personal);
    assert_eq!(work.kind, ProfileKind::Work);
    assert_eq!(agent.kind, ProfileKind::AgentSandbox);
    assert_eq!(temp.kind, ProfileKind::Temporary);

    // Verify disk directories were created
    assert!(personal.cache_dir.exists());
    assert!(personal.cookie_store_dir.exists());
    assert!(agent.cache_dir.exists());
    assert!(temp.root_dir.exists());

    // Clean up temporary profile
    temp.cleanup_if_temporary();
    assert!(!temp.root_dir.exists());
}

#[tokio::test]
async fn test_multi_tab_switching_and_active_tracking() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab1 = tab_manager
        .create_tab(ProfileId::personal(), "https://alpha.com")
        .await
        .unwrap();
    let tab2 = tab_manager
        .create_tab(ProfileId::work(), "https://beta.com")
        .await
        .unwrap();
    let tab3 = tab_manager
        .create_tab(ProfileId::agent_sandbox(), "https://gamma.com")
        .await
        .unwrap();

    // Tab1 was first, so it is initially active
    assert_eq!(tab_manager.get_active_tab().await, Some(tab1));

    // Switch to Tab2
    tab_manager.switch_tab(tab2).await.unwrap();
    assert_eq!(tab_manager.get_active_tab().await, Some(tab2));

    // Switch to Tab3
    tab_manager.switch_tab(tab3).await.unwrap();
    assert_eq!(tab_manager.get_active_tab().await, Some(tab3));

    // Close Tab3, active should fallback to one of the remaining tabs
    tab_manager.close_tab(tab3).await.unwrap();
    let current_active = tab_manager.get_active_tab().await.unwrap();
    assert!(current_active == tab1 || current_active == tab2);

    let list = tab_manager.list_tabs().await;
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn test_navigation_generation_and_overlap_safety() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab_id = tab_manager
        .create_tab(ProfileId::personal(), "about:blank")
        .await
        .unwrap();
    let tab = tab_manager.get_tab(tab_id).await.unwrap();

    // 1. Initiate navigate(A) with CEF request ID 1001
    let nav_id_1 = tab_manager
        .navigation()
        .navigate_with_request_id(&tab, "https://example.com/page-a", NavigationSource::Programmatic, Some(1001))
        .await
        .unwrap();

    assert_eq!(tab.navigation.read().await.navigation_id(), Some(nav_id_1));

    // 2. Rapidly initiate navigate(B) with CEF request ID 1002 before A finishes -> A is superseded
    let nav_id_2 = tab_manager
        .navigation()
        .navigate_with_request_id(&tab, "https://example.com/page-b", NavigationSource::Programmatic, Some(1002))
        .await
        .unwrap();

    assert_ne!(nav_id_1, nav_id_2);
    assert_eq!(tab.navigation.read().await.navigation_id(), Some(nav_id_2));

    // 3. Late arriving callback for A must be ignored
    tab_manager
        .navigation()
        .handle_load_end(&tab, Some(nav_id_1), "https://example.com/page-a", 200, true)
        .await;

    // State is still Loading for B!
    assert_eq!(tab.navigation.read().await.navigation_id(), Some(nav_id_2));
    assert!(tab.navigation.read().await.is_loading());

    // 4. Callback for B completes navigation
    tab_manager
        .navigation()
        .handle_load_end(&tab, Some(nav_id_2), "https://example.com/page-b", 200, true)
        .await;

    let nav_state = tab.navigation.read().await.clone();
    match nav_state {
        NavigationState::Completed { id, url, http_status } => {
            assert_eq!(id, nav_id_2);
            assert_eq!(url, "https://example.com/page-b");
            assert_eq!(http_status, 200);
        }
        other => panic!("Expected Completed state for nav_id_2, got: {:?}", other),
    };

    // Verify NavigationRecord contains final URL and timestamps
    let record = tab.active_navigation_record.read().await.clone().unwrap();
    assert_eq!(record.navigation_id, nav_id_2);
    assert_eq!(record.cef_request_id, Some(1002));
    assert_eq!(record.final_url, Some("https://example.com/page-b".to_string()));
    assert!(record.finished_at_ms.is_some());
}

#[tokio::test]
async fn test_main_frame_filtering_contract() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab_id = tab_manager
        .create_tab(ProfileId::personal(), "about:blank")
        .await
        .unwrap();
    let tab = tab_manager.get_tab(tab_id).await.unwrap();

    let nav_id = tab_manager
        .navigation()
        .navigate(&tab, "https://example.com/main", NavigationSource::Programmatic)
        .await
        .unwrap();

    // An iframe subframe load callback arrives
    tab_manager
        .navigation()
        .handle_load_start(&tab, Some(nav_id), "https://ad-network.com/iframe", false)
        .await;

    // Must NOT affect tab navigation state (still Loading, not Committed for iframe)
    let nav_state = tab.navigation.read().await.clone();
    match nav_state {
        NavigationState::Loading { requested_url, .. } => {
            assert_eq!(requested_url, "https://example.com/main");
        }
        other => panic!("Subframe load altered navigation state unexpectedly: {:?}", other),
    }

    // Subframe ends load
    tab_manager
        .navigation()
        .handle_load_end(&tab, Some(nav_id), "https://ad-network.com/iframe", 200, false)
        .await;

    // Must NOT complete the tab!
    assert!(tab.navigation.read().await.is_loading());

    // Main frame commits and completes
    tab_manager
        .navigation()
        .handle_load_start(&tab, Some(nav_id), "https://example.com/main", true)
        .await;
    assert!(matches!(*tab.navigation.read().await, NavigationState::Committed { .. }));

    tab_manager
        .navigation()
        .handle_load_end(&tab, Some(nav_id), "https://example.com/main", 200, true)
        .await;
    assert!(matches!(*tab.navigation.read().await, NavigationState::Completed { .. }));
}

#[tokio::test]
async fn test_same_document_navigation() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab_id = tab_manager
        .create_tab(ProfileId::personal(), "https://example.com/page")
        .await
        .unwrap();
    let tab = tab_manager.get_tab(tab_id).await.unwrap();

    // Handle hash fragment change
    tab_manager
        .navigation()
        .handle_same_document_nav(&tab, "https://example.com/page#heading-2")
        .await;

    assert_eq!(*tab.url.read().await, "https://example.com/page#heading-2");
    // Navigation state remains Idle (not forced into Loading -> Committed -> Completed)
    assert_eq!(*tab.navigation.read().await, NavigationState::Idle);
}

#[tokio::test]
async fn test_renderer_crash_fails_closed_inv_11a() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let mut rx = event_bus.subscribe();
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab_id = tab_manager
        .create_tab(ProfileId::agent_sandbox(), "https://untrusted-site.com")
        .await
        .unwrap();
    let tab = tab_manager.get_tab(tab_id).await.unwrap();

    // Register a pending inflight operation
    let (tx, rx_op) = oneshot::channel();
    let op_id = tab_manager
        .navigation()
        .register_operation(tab_id, None, "evaluate_js", tx)
        .await;

    // Renderer crashes
    let diagnostics = RendererCrashDiagnostics {
        termination_status: RendererTerminationStatus::Crashed,
        raw_cef_status: CefTerminationStatus::ProcessCrashed,
        observed_at_ms: 12345678,
    };
    tab_manager
        .handle_renderer_crash(tab_id, diagnostics.clone())
        .await
        .unwrap();

    // Invariant 11A: Tab enters RendererTerminated state
    let health = tab.health.read().await.clone();
    assert!(health.is_crashed());
    match health {
        TabHealth::RendererTerminated { status, diagnostics: diag } => {
            assert_eq!(status, RendererTerminationStatus::Crashed);
            assert_eq!(diag.raw_cef_status, CefTerminationStatus::ProcessCrashed);
            assert_eq!(diag.observed_at_ms, 12345678);
        }
        _ => panic!("Expected RendererTerminated health status"),
    }

    // Invariant 11A: Pending operation fails closed exactly once via CAS
    let op_res = rx_op.await.expect("Sender must deliver terminal result");
    assert!(
        matches!(op_res, Err(BrowserError::RendererCrashed(..))),
        "Pending operation must fail closed with RendererCrashed"
    );

    // Attempting late completion on op_id returns false (atomic CAS prevents double-resolve)
    assert!(
        !tab_manager.navigation().complete_operation(op_id).await,
        "Late completion after crash must return false (CAS prevented double-completion)"
    );

    // Attempting to navigate on crashed tab fails closed immediately
    let nav_res = tab_manager
        .navigation()
        .navigate(&tab, "https://another-page.com", NavigationSource::Programmatic)
        .await;
    match nav_res {
        Err(BrowserError::RendererCrashed(failed_tab_id, _)) => {
            assert_eq!(failed_tab_id, tab_id);
        }
        other => panic!("Expected RendererCrashed error, got: {:?}", other),
    }

    // Verify RendererProcessTerminated event was emitted with producer Cef
    let mut received_crash_event = false;
    while let Ok(evt) = rx.try_recv() {
        if let BrowserEventKind::RendererProcessTerminated { diagnostics: d } = evt.kind {
            assert_eq!(evt.tab_id, Some(tab_id));
            assert_eq!(evt.producer, BrowserEventProducer::Cef);
            assert_eq!(d.termination_status, RendererTerminationStatus::Crashed);
            received_crash_event = true;
            break;
        }
    }
    assert!(
        received_crash_event,
        "BrowserEventKind::RendererProcessTerminated must be emitted"
    );
}

#[tokio::test]
async fn test_browser_event_bus_loss_and_replay_awareness() {
    let event_bus = BrowserEventBus::with_replay_capacity(16, 10);
    let tab_id = TabId::new();

    // Emit 5 critical events
    for i in 1..=5 {
        event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(tab_id),
            Some(1),
            None,
            BrowserEventKind::TabTitleChanged {
                title: format!("Title {}", i),
            },
        );
    }

    assert_eq!(event_bus.current_sequence(), 6);

    // Replay critical events from sequence 3
    let replayed = event_bus.replay_critical_events(3).expect("Replay must succeed");
    assert_eq!(replayed.len(), 3);
    assert_eq!(replayed[0].sequence, 3);
    assert_eq!(replayed[1].sequence, 4);
    assert_eq!(replayed[2].sequence, 5);

    // Emit 10 more events to overflow the 10-item replay buffer
    for i in 6..=15 {
        event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(tab_id),
            Some(1),
            None,
            BrowserEventKind::TabTitleChanged {
                title: format!("Title {}", i),
            },
        );
    }

    // Attempting to replay from sequence 2 should now return ResyncRequired error
    let resync_err = event_bus.replay_critical_events(2);
    assert!(
        resync_err.is_err(),
        "Replaying pruned sequence must trigger ResyncRequired"
    );
    let gap = resync_err.unwrap_err();
    assert_eq!(gap.requested_sequence, 2);
    assert!(gap.oldest_available_sequence > 2);

    // Test authoritative StateSnapshot recovery contract
    let temp_dir = TempDir::new().unwrap();
    let profile_mgr = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let tab_manager = TabManager::new(profile_mgr, event_bus);
    let snapshot = tab_manager.emit_state_snapshot().await;
    assert_eq!(snapshot.tabs.len(), 0);
    assert!(snapshot.generated_at_ms > 0);
}

#[tokio::test]
async fn test_navigation_correlation_table_and_redirect_tracking() {
    let temp_dir = TempDir::new().unwrap();
    let profile_manager = Arc::new(ProfileManager::with_custom_dirs(
        temp_dir.path().join("profiles"),
        temp_dir.path().join("temp"),
    ));
    let event_bus = BrowserEventBus::new(32);
    let tab_manager = TabManager::new(profile_manager, event_bus);

    let tab_id = tab_manager
        .create_tab(ProfileId::personal(), "about:blank")
        .await
        .unwrap();
    let tab = tab_manager.get_tab(tab_id).await.unwrap();

    // Bind real CEF browser ID 42
    tab_manager.bind_cef_browser(tab_id, 42).await.unwrap();

    // 1. Initiate navigation with known initial URL
    let nav_id = tab_manager
        .navigation()
        .navigate(&tab, "http://example.com/initial", NavigationSource::UserGesture)
        .await
        .unwrap();

    // 2. Lookup correlation entry by tab_id and browser_id
    let corr = tab_manager
        .navigation()
        .get_correlation(tab_id)
        .await
        .expect("correlation record must exist");
    assert_eq!(corr.nav_id, nav_id);
    assert_eq!(corr.cef_browser_id, Some(42));
    assert_eq!(corr.requested_url, "http://example.com/initial");
    assert!(corr.redirect_chain.is_empty());

    // 3. CEF GetResourceRequestHandler discovers request ID 10001
    tab_manager
        .navigation()
        .record_cef_request_by_browser(42, 10001, "http://example.com/initial")
        .await
        .expect("request registration must succeed");

    // 4. CEF notifies redirect to https://example.com/login
    tab_manager
        .navigation()
        .record_redirect_by_browser(42, "https://example.com/login")
        .await
        .expect("redirect registration must succeed");

    // 5. CEF fires OnLoadStart (committed) without passing NavId directly
    tab_manager
        .navigation()
        .handle_load_start(&tab, None, "https://example.com/login", true)
        .await;

    // 6. CEF fires OnLoadEnd (completed 200 OK) without passing NavId directly
    tab_manager
        .navigation()
        .handle_load_end(&tab, None, "https://example.com/login", 200, true)
        .await;

    // Verify correlation record has full end-to-end trace:
    // TabId | NavId | cef_request_id | redirects | committed | completed
    let final_corr = tab_manager
        .navigation()
        .get_correlation_by_browser(42)
        .await
        .unwrap();
    assert_eq!(final_corr.nav_id, nav_id);
    assert_eq!(final_corr.cef_request_id, Some(10001));
    assert_eq!(
        final_corr.redirect_chain,
        vec!["https://example.com/login".to_string()]
    );
    assert_eq!(
        final_corr.committed_url.as_deref(),
        Some("https://example.com/login")
    );
    assert_eq!(
        final_corr.completed_url.as_deref(),
        Some("https://example.com/login")
    );
    assert_eq!(final_corr.http_status, Some(200));
}
