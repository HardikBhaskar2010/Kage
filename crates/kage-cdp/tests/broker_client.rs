//! Integration tests for `kage-cdp` broker, client, authentication, and multiplexer.

use kage_cdp::{CdpBroker, CdpClient, CdpError, CdpEvent, TargetRouter};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use uuid::Uuid;

#[tokio::test]
async fn test_non_loopback_binding_rejected() {
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
    let forbidden_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9222);
    let result = CdpBroker::bind_with_addr(forbidden_addr, &nonce).await;
    match result {
        Err(CdpError::NonLoopbackBindingForbidden(ip)) => {
            assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        }
        _ => panic!("Expected NonLoopbackBindingForbidden error"),
    }
}

#[tokio::test]
async fn test_handshake_authentication_matrix() {
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
    let (broker, addr) = CdpBroker::bind_ephemeral(&nonce).await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    // 1. Missing nonce -> 401 Unauthorized
    let err_missing = CdpClient::connect(&ws_url, None).await.unwrap_err();
    match err_missing {
        CdpError::AuthFailed(msg) => {
            assert!(msg.contains("401"), "Expected 401 in: {}", msg);
        }
        other => panic!("Expected AuthFailed(401), got: {:?}", other),
    }

    // 2. Invalid nonce -> 403 Forbidden
    let err_invalid = CdpClient::connect(&ws_url, Some("invalid_nonce_token"))
        .await
        .unwrap_err();
    match err_invalid {
        CdpError::AuthFailed(msg) => {
            assert!(msg.contains("403"), "Expected 403 in: {}", msg);
        }
        other => panic!("Expected AuthFailed(403), got: {:?}", other),
    }

    // 3. Valid nonce -> Success
    let client = CdpClient::connect(&ws_url, Some(&nonce))
        .await
        .expect("Valid nonce must connect");

    // Send domain enable
    let res = client.call("Page.enable", json!({})).await.expect("call Page.enable");
    assert_eq!(res["enabled"], true);

    broker.shutdown();
}

#[tokio::test]
async fn test_monotonic_command_sequencing_and_correlation() {
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
    let (broker, addr) = CdpBroker::bind_ephemeral(&nonce).await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client = CdpClient::connect(&ws_url, Some(&nonce))
        .await
        .expect("connect");

    // Concurrently invoke multiple distinct domain commands
    let domains = vec![
        "Page.enable",
        "DOM.enable",
        "Network.enable",
        "Runtime.enable",
    ];

    let mut handles = Vec::new();
    for domain in domains {
        let c = &client;
        handles.push(async move {
            let res = c.call(domain, json!({})).await.expect("call domain");
            (domain, res)
        });
    }

    let results = futures_util::future::join_all(handles).await;
    assert_eq!(results.len(), 4);
    for (_domain, res) in results {
        assert_eq!(res["enabled"], true);
    }

    broker.shutdown();
}

#[tokio::test]
async fn test_protocol_error_parsing() {
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
    let (broker, addr) = CdpBroker::bind_ephemeral(&nonce).await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client = CdpClient::connect(&ws_url, Some(&nonce))
        .await
        .expect("connect");

    let err = client
        .call("Error.nonExistentMethod", json!({}))
        .await
        .unwrap_err();

    match err {
        CdpError::ProtocolError { method, code, message } => {
            assert_eq!(method, "Error.nonExistentMethod");
            assert_eq!(code, -32601);
            assert!(message.contains("Method 'Error.nonExistentMethod' not found"));
        }
        other => panic!("Expected ProtocolError, got: {:?}", other),
    }

    broker.shutdown();
}

#[tokio::test]
async fn test_multi_consumer_event_broadcast_fanout() {
    let nonce = format!("kage_nonce_{}", Uuid::new_v4().simple());
    let (broker, addr) = CdpBroker::bind_ephemeral(&nonce).await.expect("bind ephemeral");
    let ws_url = format!("ws://{}", addr);

    let client_a = CdpClient::connect(&ws_url, Some(&nonce))
        .await
        .expect("connect client A");
    let client_b = CdpClient::connect(&ws_url, Some(&nonce))
        .await
        .expect("connect client B");

    let mut sub_a = client_a.subscribe_events();
    let mut sub_b = client_b.subscribe_events();

    // Broadcast 3 typed events from the broker
    let evt1 = CdpEvent::DomDocumentUpdated {};
    let evt2 = CdpEvent::NetworkRequestWillBeSent {
        request_id: "req_test_100".into(),
        url: "https://example.com/api/data".into(),
        method: "GET".into(),
    };
    let evt3 = CdpEvent::ConsoleMessageAdded {
        level: "warning".into(),
        text: "Low memory warning".into(),
    };

    broker.broadcast_event(evt1.clone()).expect("broadcast evt1");
    broker.broadcast_event(evt2.clone()).expect("broadcast evt2");
    broker.broadcast_event(evt3.clone()).expect("broadcast evt3");

    // Both subscribers should receive all 3 events in exact order
    let a1 = sub_a.recv().await.expect("sub_a recv 1");
    let a2 = sub_a.recv().await.expect("sub_a recv 2");
    let a3 = sub_a.recv().await.expect("sub_a recv 3");

    let b1 = sub_b.recv().await.expect("sub_b recv 1");
    let b2 = sub_b.recv().await.expect("sub_b recv 2");
    let b3 = sub_b.recv().await.expect("sub_b recv 3");

    assert_eq!(a1, evt1);
    assert_eq!(a2, evt2);
    assert_eq!(a3, evt3);

    assert_eq!(b1, evt1);
    assert_eq!(b2, evt2);
    assert_eq!(b3, evt3);

    broker.shutdown();
}

#[tokio::test]
async fn test_target_session_multiplexing() {
    let router = TargetRouter::new();
    let tab_id = Uuid::new_v4();
    let target_id = "target_page_cef_42";

    // Bind tab to target (maintains TabId + ProfileId + CefBrowserId identity separation)
    router.bind_target(tab_id, target_id).await;

    // Attach two distinct client sessions for DevTools and Context Engine
    let devtools_sid = router.attach_session(target_id, "devtools").await;
    let context_sid = router.attach_session(target_id, "context").await;

    assert_ne!(devtools_sid, context_sid);

    let devtools_info = router.get_session_info(&devtools_sid).await.expect("devtools session info");
    assert_eq!(devtools_info.client_name, "devtools");
    assert_eq!(devtools_info.target_id, target_id);
    assert_eq!(devtools_info.tab_id, Some(tab_id));

    let context_info = router.get_session_info(&context_sid).await.expect("context session info");
    assert_eq!(context_info.client_name, "context");
    assert_eq!(context_info.target_id, target_id);
    assert_eq!(context_info.tab_id, Some(tab_id));

    // List target sessions
    let sessions = router.list_sessions_for_target(target_id).await;
    assert_eq!(sessions.len(), 2);

    // Detach context session
    let detached = router.detach_session(&context_sid).await;
    assert!(detached.is_some());
    assert_eq!(router.session_count().await, 1);
}
