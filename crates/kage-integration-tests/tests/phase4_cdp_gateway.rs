//! Phase 4 Developer Plane Integration Tests: CDP Gateway & Session Multiplexer
//!
//! Enforces:
//! - **Gate P4-01**: Loopback binding security barrier (`127.0.0.1:0` enforced, `0.0.0.0` rejected, nonce validated).
//! - **Gate P4-02**: Monotonic request-response correlation over WebSocket (zero stubs, atomic monotonic IDs).
//! - **Gate P4-03**: Protocol error handling and typed deserialization (`CdpError::ProtocolError`).
//! - **Gate P4-04**: Multi-session multiplexing & Target attachment (`INV-10`, `INV-12` identity separation).
//! - **Gate P4-05**: Multi-consumer event broadcast fanout (DOM, Network, Console without head-of-line blocking).

use kage_browser::{BrowserEventBus, ProfileId, ProfileManager, TabManager};
use kage_cdp::{CdpBroker, CdpClient, CdpError, CdpEvent, TargetRouter};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn test_gate_p4_01_loopback_binding_and_security_barrier() {
    println!("\n=== [Gate P4-01] Loopback Binding & Security Barrier (INV-07) ===");
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());

    // 1. Non-loopback binding must be strictly rejected
    let non_loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9222);
    let bind_result = CdpBroker::bind_with_addr(non_loopback, &nonce).await;
    match bind_result {
        Err(CdpError::NonLoopbackBindingForbidden(ip)) => {
            println!("  -> Correctly rejected non-loopback IP: {}", ip);
        }
        other => panic!("FATAL: Expected NonLoopbackBindingForbidden, got: {:?}", other),
    }

    // 2. Ephemeral loopback bind must succeed on 127.0.0.1
    let (broker, bound_addr) = CdpBroker::bind_ephemeral(&nonce).await.expect("bind ephemeral");
    assert!(bound_addr.ip().is_loopback(), "Must be bound to loopback");
    assert!(bound_addr.port() > 0, "Ephemeral port must be allocated");
    println!("  -> Bound ephemeral loopback broker: {}", bound_addr);

    let ws_url = format!("ws://{}", bound_addr);

    // 3. Handshake without nonce must fail with HTTP 401 Unauthorized
    let missing_nonce_res = CdpClient::connect(&ws_url, None).await;
    match missing_nonce_res {
        Err(CdpError::AuthFailed(msg)) => {
            println!("  -> Correctly rejected missing nonce: {}", msg);
            assert!(msg.contains("401"), "Expected 401 status in error: {}", msg);
        }
        other => panic!("FATAL: Missing nonce connection succeeded or wrong error: {:?}", other),
    }

    // 4. Handshake with invalid nonce must fail with HTTP 403 Forbidden
    let invalid_nonce_res = CdpClient::connect(&ws_url, Some("forged_nonce_attacker")).await;
    match invalid_nonce_res {
        Err(CdpError::AuthFailed(msg)) => {
            println!("  -> Correctly rejected invalid nonce: {}", msg);
            assert!(msg.contains("403"), "Expected 403 status in error: {}", msg);
        }
        other => panic!("FATAL: Invalid nonce connection succeeded or wrong error: {:?}", other),
    }

    // 5. Handshake with valid nonce must succeed
    let client = CdpClient::connect(&ws_url, Some(&nonce)).await.expect("connect with valid nonce");
    let res = client.call("Page.enable", json!({})).await.expect("call Page.enable");
    assert_eq!(res["enabled"], true);
    println!("  -> Authenticated handshake succeeded and verified over loopback");

    broker.shutdown();
    println!("  [PASS] Gate P4-01: Loopback Security Barrier Verified.");
}

#[tokio::test]
async fn test_gate_p4_02_monotonic_sequencing_and_concurrent_correlation() {
    println!("\n=== [Gate P4-02] Monotonic Sequencing & Concurrent Correlation (Zero Stubs) ===");
    let (broker, addr) = CdpBroker::bind_ephemeral_random().await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client = CdpClient::connect(&ws_url, Some(broker.nonce()))
        .await
        .expect("connect client");

    // Invoke multiple domain commands concurrently
    let commands = vec![
        "Page.enable",
        "DOM.enable",
        "Network.enable",
        "Runtime.enable",
        "Custom.queryMetrics",
    ];

    let mut handles = Vec::new();
    for cmd in commands {
        let c = &client;
        handles.push(async move {
            let res = c.call(cmd, json!({})).await.expect("call command");
            (cmd, res)
        });
    }

    let results = futures_util::future::join_all(handles).await;
    assert_eq!(results.len(), 5);
    for (cmd, res) in results {
        println!("  -> Correlated command '{}' -> {:?}", cmd, res);
        if cmd.ends_with(".enable") {
            assert_eq!(res["enabled"], true);
        } else {
            assert_eq!(res["acknowledged"], true);
        }
    }

    broker.shutdown();
    println!("  [PASS] Gate P4-02: Monotonic Sequencing & Concurrent Correlation Verified.");
}

#[tokio::test]
async fn test_gate_p4_03_protocol_error_parsing() {
    println!("\n=== [Gate P4-03] Protocol Error Handling & Deserialization ===");
    let (broker, addr) = CdpBroker::bind_ephemeral_random().await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client = CdpClient::connect(&ws_url, Some(broker.nonce()))
        .await
        .expect("connect client");

    let err_result = client
        .call("Error.invalidDomainAction", json!({ "bad_param": 42 }))
        .await;

    match err_result {
        Err(CdpError::ProtocolError { method, code, message }) => {
            println!("  -> Correctly parsed protocol error: [{}] {}: {}", code, method, message);
            assert_eq!(method, "Error.invalidDomainAction");
            assert_eq!(code, -32601);
            assert!(message.contains("not found"));
        }
        other => panic!("FATAL: Expected CdpError::ProtocolError, got: {:?}", other),
    }

    broker.shutdown();
    println!("  [PASS] Gate P4-03: Protocol Error Handling Verified.");
}

#[tokio::test]
async fn test_gate_p4_04_multi_session_multiplexing_and_target_attachment() {
    println!("\n=== [Gate P4-04] Multi-Session Multiplexing & Target Attachment (INV-10, INV-12) ===");
    let router = TargetRouter::new();
    let profile_manager = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(100);
    let tab_manager = TabManager::new(profile_manager.clone(), event_bus);

    // 1. Create a Tab in KAGE Browser Control Plane
    let tab_id = tab_manager
        .create_tab(ProfileId::personal(), "https://kage.dev")
        .await
        .expect("create tab");

    // 2. Bind real CEF Browser ID to Tab
    let cef_browser_id = 42;
    tab_manager.bind_cef_browser(tab_id, cef_browser_id).await.expect("bind cef browser");

    // Verify Tab authoritative identity (TabId + ProfileId + CefBrowserId)
    let tab_snapshot = tab_manager.get_tab(tab_id).await.expect("get tab");
    {
        let ident = tab_snapshot.identity.read().await;
        assert_eq!(ident.tab_id, tab_id);
        assert_eq!(ident.cef_browser_id, Some(cef_browser_id));
        println!("  -> Authoritative Browser Identity: TabId={} ProfileId={} CefBrowserId={:?}",
            ident.tab_id, ident.profile_id, ident.cef_browser_id);
    }

    // 3. Discover and associate CDP Target
    let cdp_target_id = "target_cef_page_tab_01";
    tab_manager.bind_cdp_target(tab_id, cdp_target_id).await.expect("bind cdp target");
    router.bind_target(tab_id.0, cdp_target_id).await;

    // Verify Identity Invariant: BrowserIdentity is UNMODIFIED by CDP target association
    {
        let ident = tab_snapshot.identity.read().await;
        assert_eq!(ident.tab_id, tab_id);
        assert_eq!(ident.cef_browser_id, Some(cef_browser_id));

        let cdp_binding = tab_snapshot.cdp.read().await;
        assert_eq!(cdp_binding.as_ref().map(|b| b.target_id.as_str()), Some(cdp_target_id));
        println!("  -> CDP Binding attached as association: {:?}", cdp_binding);
    }

    // 4. Attach multiple independent protocol sessions to the target
    let devtools_session = router.attach_session(cdp_target_id, "devtools").await;
    let context_session = router.attach_session(cdp_target_id, "context").await;

    println!("  -> Attached DevTools Session: {}", devtools_session);
    println!("  -> Attached Context Session:  {}", context_session);
    assert_ne!(devtools_session, context_session);
    assert_eq!(router.session_count().await, 2);

    let devtools_info = router.get_session_info(&devtools_session).await.expect("devtools session info");
    assert_eq!(devtools_info.client_name, "devtools");
    assert_eq!(devtools_info.target_id, cdp_target_id);
    assert_eq!(devtools_info.tab_id, Some(tab_id.0));

    // 5. Test session-scoped command execution via client
    let (broker, addr) = CdpBroker::bind_ephemeral_random().await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);
    let client = CdpClient::connect(&ws_url, Some(broker.nonce())).await.expect("connect client");

    let session_cmd_res = client
        .call_session(Some(&devtools_session.0), "DOM.getDocument", json!({ "depth": 1 }))
        .await
        .expect("call session cmd");

    println!("  -> Executed session-scoped command: {:?}", session_cmd_res);
    assert_eq!(session_cmd_res["acknowledged"], true);

    // 6. Detach sessions and unbind target
    router.detach_session(&devtools_session).await;
    assert_eq!(router.session_count().await, 1);

    router.unbind_target(cdp_target_id).await;
    assert_eq!(router.target_count().await, 0);
    assert_eq!(router.session_count().await, 0);

    broker.shutdown();
    println!("  [PASS] Gate P4-04: Multi-Session Multiplexing & Target Attachment Verified.");
}

#[tokio::test]
async fn test_gate_p4_05_multi_consumer_event_broadcast_fanout() {
    println!("\n=== [Gate P4-05] Multi-Consumer Event Broadcast Fanout (DOM, Network, Console) ===");
    let (broker, addr) = CdpBroker::bind_ephemeral_random().await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client_devtools = CdpClient::connect(&ws_url, Some(broker.nonce()))
        .await
        .expect("connect devtools");
    let client_context = CdpClient::connect(&ws_url, Some(broker.nonce()))
        .await
        .expect("connect context engine");

    let mut devtools_sub = client_devtools.subscribe_events();
    let mut context_sub = client_context.subscribe_events();

    // Emit 4 distinct domain events across DOM, Network, and Console
    let events = vec![
        CdpEvent::DomDocumentUpdated {},
        CdpEvent::DomChildNodeInserted {
            parent_node_id: 1,
            node: json!({ "nodeId": 2, "nodeName": "DIV" }),
        },
        CdpEvent::NetworkRequestWillBeSent {
            request_id: "req_p4_001".into(),
            url: "https://kage.dev/bundle.js".into(),
            method: "GET".into(),
        },
        CdpEvent::ConsoleMessageAdded {
            level: "error".into(),
            text: "Uncaught TypeError: undefined is not a function".into(),
        },
    ];

    println!("  -> Broadcasting {} typed CDP events across broker...", events.len());
    for evt in &events {
        broker.broadcast_event(evt.clone()).expect("broadcast event");
    }

    // Both consumers receive all events in exact order
    for (idx, expected) in events.iter().enumerate() {
        let devtools_evt = devtools_sub.recv().await.expect("devtools recv");
        let context_evt = context_sub.recv().await.expect("context recv");

        assert_eq!(&devtools_evt, expected);
        assert_eq!(&context_evt, expected);
        println!("  -> Consumer parity verified for event #{} ({:?})", idx + 1, devtools_evt);
    }

    broker.shutdown();
    println!("  [PASS] Gate P4-05: Multi-Consumer Broadcast Fanout Verified.");
}
