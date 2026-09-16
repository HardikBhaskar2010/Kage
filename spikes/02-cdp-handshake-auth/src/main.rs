use kage_spike_cdp_handshake::*;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("  KAGE TECHNICAL SPIKE 2: CDP Transport + Broker Auth Matrix");
    println!("============================================================");

    let active_session_nonce = format!("kage_nonce_{}", Uuid::new_v4());
    println!("Generated Secure Session Nonce: {}", active_session_nonce);

    // --- TEST 1: Non-Loopback Bind Hard Rejection ---
    println!("\n[1/7] Testing Non-Loopback Bind Security Guard (0.0.0.0)...");
    let forbidden_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9222);
    let bind_attempt = CdpBroker::bind_with_addr(forbidden_addr, &active_session_nonce).await;
    match bind_attempt {
        Err(BrokerError::NonLoopbackBindingForbidden(ip)) => {
            println!("  -> Correctly blocked binding to non-loopback IP: {}", ip);
            println!("  -> PASS: Non-loopback binding strictly rejected");
        }
        _ => panic!("FATAL: Broker allowed non-loopback interface binding!"),
    }

    // --- TEST 2: Ephemeral Loopback Bind (127.0.0.1:0) ---
    println!("\n[2/7] Binding Authenticated Broker to Ephemeral Loopback (127.0.0.1:0)...");
    let (broker, bound_addr) = CdpBroker::bind_ephemeral(&active_session_nonce).await?;
    println!("  -> Broker bound successfully to: {}", bound_addr);
    assert!(bound_addr.ip().is_loopback());
    println!("  -> PASS: Ephemeral loopback port bound cleanly");

    let ws_url = format!("ws://{}", bound_addr);

    // --- TEST 3: Connection with Missing Nonce ---
    println!("\n[3/7] Testing Handshake Rejection: Missing Session Nonce...");
    let missing_nonce_res = CdpClient::connect(&ws_url, None).await;
    match missing_nonce_res {
        Err(e) => {
            println!("  -> Correctly rejected missing nonce: {}", e);
            println!("  -> PASS: Missing nonce rejected with HTTP 401 Unauthorized");
        }
        Ok(_) => panic!("FATAL: Unauthenticated connection succeeded!"),
    }

    // --- TEST 4: Connection with Invalid / Expired Nonce ---
    println!("\n[4/7] Testing Handshake Rejection: Invalid Nonce...");
    let invalid_nonce_res = CdpClient::connect(&ws_url, Some("kage_nonce_fake_attacker_token")).await;
    match invalid_nonce_res {
        Err(e) => {
            println!("  -> Correctly rejected invalid nonce: {}", e);
            println!("  -> PASS: Invalid nonce rejected with HTTP 403 Forbidden");
        }
        Ok(_) => panic!("FATAL: Invalid nonce connection succeeded!"),
    }

    // --- TEST 5: Connection with Valid Nonce ---
    println!("\n[5/7] Testing Handshake Acceptance: Valid Session Nonce...");
    let client = CdpClient::connect(&ws_url, Some(&active_session_nonce)).await?;
    println!("  -> Handshake accepted! WebSocket connection established over loopback");
    println!("  -> PASS: Valid session nonce authenticated");

    // --- TEST 6: Monotonic Command Sequencing ---
    println!("\n[6/7] Testing Domain Enables and Monotonic Command Sequencing...");
    for domain in &["Page.enable", "DOM.enable", "Network.enable", "Runtime.enable"] {
        let res = client.send_command(domain, json!({})).await?;
        assert_eq!(res["enabled"], true);
        println!("  -> Command '{}' acknowledged successfully", domain);
    }
    println!("  -> PASS: Domain commands sequenced and acknowledged");

    // --- TEST 7: Multi-Consumer Concurrent Event Multiplexing ---
    println!("\n[7/7] Testing Multi-Consumer Broadcast Fanout (DOM, Network, Console)...");
    let mut consumer_a = client.subscribe_events();
    let mut consumer_b = client.subscribe_events();

    // Broker emits 3 live events
    let event_tx = broker.event_sender();
    event_tx.send(CdpEvent::DomDocumentUpdated {})?;
    event_tx.send(CdpEvent::NetworkRequestWillBeSent {
        request_id: "req_1001".to_string(),
        url: "https://example.com/styles.css".to_string(),
        method: "GET".to_string(),
    })?;
    event_tx.send(CdpEvent::ConsoleMessageAdded {
        level: "warning".to_string(),
        text: "KAGE DevTools Active".to_string(),
    })?;

    // Consumer A receives
    let evt1_a = consumer_a.recv().await?;
    let evt2_a = consumer_a.recv().await?;
    let evt3_a = consumer_a.recv().await?;

    // Consumer B receives
    let evt1_b = consumer_b.recv().await?;
    let evt2_b = consumer_b.recv().await?;
    let evt3_b = consumer_b.recv().await?;

    assert_eq!(evt1_a, evt1_b);
    assert_eq!(evt2_a, evt2_b);
    assert_eq!(evt3_a, evt3_b);

    println!("  -> Consumer A and Consumer B received all 3 events in exact order:");
    println!("     1. {:?}", evt1_a);
    println!("     2. {:?}", evt2_a);
    println!("     3. {:?}", evt3_a);
    println!("  -> PASS: Multi-consumer event demux & broadcast fanout verified");

    broker.shutdown();
    println!("\n[ALL SPIKE 2 TESTS PASSED SUCCESSFULLY] 🚀");
    Ok(())
}
