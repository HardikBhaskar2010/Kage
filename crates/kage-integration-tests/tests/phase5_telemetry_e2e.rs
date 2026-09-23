//! Phase 5 Empirical CEF-Backed Telemetry & Context Engine Integration Test Suite.
//!
//! Validates live CDP telemetry streaming, bounded ring buffers, sensitive data redaction,
//! 4-stage DOM pruning, untrusted XML framing, and sub-2ms active context switching
//! against real multi-process CEF execution:
//!
//! - **P5-GATE-01**: Live CDP Telemetry Stream (Console) from live page into TabTelemetry.
//! - **P5-GATE-02**: Bounded Ring Buffers & Deduplication (recurring console errors collapsed with [xN] count).
//! - **P5-GATE-03**: Zero-Leak Sensitive Redaction (Bearer tokens, passwords redacted before ring buffer storage).
//! - **P5-GATE-04**: Real DOM Tree Ingestion & 4-Stage Pruning (DOM.getDocument cached and pruned to token budget).
//! - **P5-GATE-05**: Untrusted Data Framing & BrowserObservation Assembly (<untrusted_web_content> within 4,000 tokens).
//! - **P5-GATE-06**: Sub-2ms Active Tab Context Swapping (< 2ms benchmark across multi-tab telemetry).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kage_browser::{TabId, TelemetryCoordinator};
use kage_cdp::{CdpBroker, CdpClient, CdpEvent};
use kage_context::DomPruner;
use kage_engine::composition::{ChromeLayoutConfig, NativeSurfaceManager};
use kage_engine::coordinates::DpiContext;
use kage_engine::runtime::{CefEngineState, CefRuntime, RuntimeConfig};
use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[cfg(target_os = "windows")]
fn find_subprocess_binary() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap().parent().unwrap();
    let debug_path = root
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join("debug")
        .join("kage-cef-subprocess.exe");

    if debug_path.exists() {
        return debug_path;
    }

    root.join("target")
        .join("debug")
        .join("kage-cef-subprocess.exe")
}

#[cfg(target_os = "windows")]
struct TestParentWindow {
    hwnd: isize,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl TestParentWindow {
    fn new(title: &str) -> Self {
        let (hwnd_tx, hwnd_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let title_owned = title.to_string();

        let join_handle = std::thread::spawn(move || {
            use std::ptr::null;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            unsafe {
                let class_name: Vec<u16> = "KageTelemetryE2EWindowClass\0".encode_utf16().collect();
                let wnd_class = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(DefWindowProcW),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: 0 as _,
                    hIcon: 0 as _,
                    hCursor: 0 as _,
                    hbrBackground: 0 as _,
                    lpszMenuName: null(),
                    lpszClassName: class_name.as_ptr(),
                };
                RegisterClassW(&wnd_class);

                let window_title: Vec<u16> = format!("{}\0", title_owned).encode_utf16().collect();
                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    window_title.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    1280,
                    720,
                    0 as _,
                    0 as _,
                    0 as _,
                    null(),
                );
                ShowWindow(hwnd, SW_SHOW);

                hwnd_tx.send(hwnd as isize).expect("Failed to send HWND");

                let mut msg = std::mem::zeroed();
                while stop_rx.try_recv().is_err() {
                    while PeekMessageW(&mut msg, 0 as _, 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_QUIT {
                            break;
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }

                DestroyWindow(hwnd);
            }
        });

        let hwnd = hwnd_rx.recv().expect("Failed to receive parent HWND");
        Self {
            hwnd,
            stop_tx: Some(stop_tx),
            join_handle: Some(join_handle),
        }
    }

    fn hwnd(&self) -> isize {
        self.hwnd
    }

    fn destroy(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn find_available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
}

async fn fetch_json_version(port: u16) -> Result<serde_json::Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| format!("Connect error: {e}"))?;

    let req = format!(
        "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        port
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Write error: {e}"))?;

    let mut buf = vec![0u8; 8192];
    let mut total_read = 0;
    while total_read < buf.len() {
        let n = stream.read(&mut buf[total_read..]).await.map_err(|e| format!("Read error: {e}"))?;
        if n == 0 {
            break;
        }
        total_read += n;
        let s = String::from_utf8_lossy(&buf[..total_read]);
        if let Some(pos) = s.find("\r\n\r\n") {
            let body = &s[pos + 4..];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                return Ok(v);
            }
        }
    }

    let s = String::from_utf8_lossy(&buf[..total_read]);
    if let Some(pos) = s.find("\r\n\r\n") {
        let body = &s[pos + 4..];
        serde_json::from_str(body).map_err(|e| format!("JSON parse error: {e}, body: {body}"))
    } else {
        Err("No HTTP header boundary found in /json/version".into())
    }
}

#[tokio::test]
async fn test_phase5_empirical_telemetry_e2e() {
    println!("\n================================================================================");
    println!("=== KAGE PHASE 5: EMPIRICAL CEF-BACKED TELEMETRY & CONTEXT E2E SUITE       ===");
    println!("================================================================================");

    let temp_dir = TempDir::new().expect("create temp dir");
    let cdp_port = find_available_port();
    println!("  -> Allocated dynamic CEF Remote Debugging Port: {}", cdp_port);

    // 1. CEF Runtime Setup with Remote Debugging Port Enabled
    let mut config = RuntimeConfig::default();
    config.root_cache_path = temp_dir.path().join("cef_root");
    config.cache_path = config.root_cache_path.join("profiles").join("default");
    config.subprocess_path = Some(find_subprocess_binary());
    config.multi_threaded_message_loop = true;
    config.no_sandbox = true;
    config.remote_debugging_port = cdp_port;

    let runtime = Arc::new(CefRuntime::new(config));
    runtime.initialize_cef().expect("initialize CEF");
    assert_eq!(runtime.state(), CefEngineState::BrowserCreationAllowed);
    println!("  -> CEF initialized with Remote Debugging Port: {}", cdp_port);

    let parent_window = TestParentWindow::new("Kage Phase 5 Telemetry Verification Host");
    let parent_hwnd = parent_window.hwnd();

    let surface_manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
    let dpi = DpiContext::standard();
    let layout = surface_manager.compute_layout(1280, 720, &dpi).unwrap();
    let content_rect = layout.cef_content_rect;

    // 2. Create Browser 1 (Tab 1) navigating to Example Domain
    let test_url = "https://example.com/";
    runtime
        .create_browser(parent_hwnd, &content_rect, test_url)
        .expect("create browser 1");

    // Wait for Browser 1 to complete loading
    let start = Instant::now();
    while !runtime.is_page_loaded() && start.elapsed() < Duration::from_secs(15) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(runtime.is_page_loaded(), "Browser 1 must complete loading");
    let browser_id_1 = runtime.last_browser_id();
    println!("  -> Browser 1 loaded (browser_id={}, url={})", browser_id_1, test_url);

    // 3. Poll Chromium DevTools HTTP Endpoint & Discover Debugger URL
    let start_poll = Instant::now();
    let mut version_json = serde_json::Value::Null;
    while start_poll.elapsed() < Duration::from_secs(10) {
        if let Ok(v) = fetch_json_version(cdp_port).await {
            version_json = v;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_ne!(version_json, serde_json::Value::Null, "Chromium DevTools /json/version must respond");

    let ws_debugger_url = version_json["webSocketDebuggerUrl"]
        .as_str()
        .expect("webSocketDebuggerUrl in version response")
        .to_string();
    println!("  -> Real Chromium Browser WebSocket Debugger URL: {}", ws_debugger_url);

    // 4. Bind KAGE CdpBroker Proxying to Real Chromium CDP Endpoint
    let session_nonce = format!("kage_p5_nonce_{}", uuid::Uuid::new_v4().simple());
    let (broker, broker_addr) = CdpBroker::bind_ephemeral_with_upstream(&session_nonce, ws_debugger_url)
        .await
        .expect("bind broker with upstream");
    println!("  -> KAGE CdpBroker bound to loopback: {}", broker_addr);
    let broker_ws_url = format!("ws://{}", broker_addr);

    let client = CdpClient::connect(&broker_ws_url, Some(&session_nonce))
        .await
        .expect("connect client with valid nonce");

    // Discover target and attach
    let targets_res = client
        .call("Target.getTargets", json!({}))
        .await
        .expect("call Target.getTargets");
    let target_infos = targets_res["targetInfos"]
        .as_array()
        .expect("targetInfos array in Chromium response");
    assert!(!target_infos.is_empty(), "Chromium must return at least one active target");

    let page_target = target_infos
        .iter()
        .find(|t| t["type"] == "page")
        .expect("page target must exist");
    let target_id = page_target["targetId"].as_str().unwrap().to_string();

    let attach_res = client
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
        )
        .await
        .expect("attach to target");
    let session_id = attach_res["sessionId"]
        .as_str()
        .expect("sessionId in attach response")
        .to_string();
    println!("  -> Target Attached: targetId={}, sessionId={}", target_id, session_id);

    // Initialize TelemetryCoordinator and register Tab 1
    let tab1_id = TabId::new();
    let telemetry_coordinator = Arc::new(TelemetryCoordinator::new());
    let tab1_telemetry = telemetry_coordinator.register_tab(tab1_id, "https://example.com/").await;
    telemetry_coordinator.bind_session(&session_id, tab1_id).await;
    telemetry_coordinator.set_active_tab(tab1_id).await;

    // Enable Runtime, Page, and DOM domains
    client.call_session(Some(&session_id), "Runtime.enable", json!({})).await.expect("Runtime.enable");
    client.call_session(Some(&session_id), "Page.enable", json!({})).await.expect("Page.enable");
    client.call_session(Some(&session_id), "DOM.enable", json!({})).await.expect("DOM.enable");

    // Spawn event dispatcher from client into telemetry_coordinator
    let mut event_rx = client.subscribe_events();
    let coord_clone = telemetry_coordinator.clone();
    let sid_clone = session_id.clone();
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                CdpEvent::RuntimeConsoleApiCalled { console_type, args } => {
                    coord_clone
                        .ingest_cdp_event(
                            &sid_clone,
                            "Runtime.consoleAPICalled",
                            &json!({ "type": console_type, "args": args }),
                        )
                        .await;
                }
                CdpEvent::DomSetChildNodes { parent_id, nodes } => {
                    coord_clone
                        .ingest_cdp_event(
                            &sid_clone,
                            "DOM.setChildNodes",
                            &json!({ "parentId": parent_id, "nodes": nodes }),
                        )
                        .await;
                }
                CdpEvent::DomChildNodeInserted { parent_node_id, node } => {
                    coord_clone
                        .ingest_cdp_event(
                            &sid_clone,
                            "DOM.childNodeInserted",
                            &json!({ "parentNodeId": parent_node_id, "previousNodeId": 0, "node": node }),
                        )
                        .await;
                }
                _ => {}
            }
        }
    });

    // =========================================================================
    // Gate P5-GATE-01: Live CDP Telemetry Stream (Console)
    // =========================================================================
    println!("\n=== [Gate P5-GATE-01] Live CDP Telemetry Stream ===");
    client
        .call_session(
            Some(&session_id),
            "Runtime.evaluate",
            json!({ "expression": "console.info('KAGE_P5_LIVE_TELEMETRY_LOG_INITIALIZED');" }),
        )
        .await
        .expect("Runtime.evaluate console.info failed");

    let start = Instant::now();
    let mut found_log = false;
    while start.elapsed() < Duration::from_secs(4) {
        let console = tab1_telemetry.console.lock().await;
        if console.entries().iter().any(|e| e.text.contains("KAGE_P5_LIVE_TELEMETRY_LOG_INITIALIZED")) {
            found_log = true;
            break;
        }
        drop(console);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(found_log, "Gate P5-GATE-01 Failed: Live console log not ingested into ConsoleRingBuffer");
    println!("  [PASS] Gate P5-GATE-01: Live CDP Telemetry Stream Ingested into ConsoleRingBuffer.");

    // =========================================================================
    // Gate P5-GATE-02: Bounded Ring Buffers & Deduplication
    // =========================================================================
    println!("\n=== [Gate P5-GATE-02] Bounded Ring Buffers & Deduplication ===");
    client
        .call_session(
            Some(&session_id),
            "Runtime.evaluate",
            json!({
                "expression": "for (let i = 0; i < 5; i++) console.error('KAGE_RECURRING_PAGE_ERROR_XYZ');"
            }),
        )
        .await
        .expect("Runtime.evaluate recurring error failed");

    let start = Instant::now();
    let mut deduplicated = false;
    while start.elapsed() < Duration::from_secs(4) {
        let console = tab1_telemetry.console.lock().await;
        if let Some(entry) = console.entries().iter().find(|e| e.text.contains("KAGE_RECURRING_PAGE_ERROR_XYZ")) {
            if entry.count >= 5 {
                deduplicated = true;
                break;
            }
        }
        drop(console);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(deduplicated, "Gate P5-GATE-02 Failed: Recurring errors not deduplicated into single entry with count >= 5");
    println!("  [PASS] Gate P5-GATE-02: Recurring Errors Deduplicated with [x5] Counter.");

    // =========================================================================
    // Gate P5-GATE-03: Zero-Leak Sensitive Redaction (INV-06)
    // =========================================================================
    println!("\n=== [Gate P5-GATE-03] Zero-Leak Sensitive Redaction ===");
    client
        .call_session(
            Some(&session_id),
            "Runtime.evaluate",
            json!({
                "expression": "console.error('Request failed with Bearer secret_live_token_7721 and password=hunter2');"
            }),
        )
        .await
        .expect("Runtime.evaluate sensitive log failed");

    let start = Instant::now();
    let mut redacted = false;
    while start.elapsed() < Duration::from_secs(4) {
        let console = tab1_telemetry.console.lock().await;
        for e in console.entries() {
            if e.text.contains("Request failed with") {
                assert!(!e.text.contains("secret_live_token_7721"), "Raw Bearer token leaked into ConsoleRingBuffer!");
                assert!(!e.text.contains("hunter2"), "Raw password leaked into ConsoleRingBuffer!");
                assert!(e.text.contains("[REDACTED"), "Redaction marker missing from sanitized log!");
                redacted = true;
                break;
            }
        }
        if redacted {
            break;
        }
        drop(console);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(redacted, "Gate P5-GATE-03 Failed: Sensitive credentials not scrubbed from console buffer");
    println!("  [PASS] Gate P5-GATE-03: Sensitive Credentials Sanitized Before Buffer Storage (Zero-Leak).");

    // =========================================================================
    // Gate P5-GATE-04: Real DOM Tree Ingestion & 4-Stage Pruning
    // =========================================================================
    println!("\n=== [Gate P5-GATE-04] Real DOM Tree Ingestion & 4-Stage Pruning ===");
    let dom_doc_resp = client
        .call_session(Some(&session_id), "DOM.getDocument", json!({ "depth": -1, "pierce": true }))
        .await
        .expect("DOM.getDocument failed");
    let root_val = &dom_doc_resp["root"];
    assert!(root_val.get("nodeId").is_some(), "DOM root must have nodeId");

    {
        let mut dom_store = tab1_telemetry.dom.lock().await;
        dom_store.set_document(root_val);
        assert!(dom_store.node_count() > 0, "DomTreeStore must contain parsed nodes");
        println!("  -> Ingested real Chromium DOM tree with {} nodes", dom_store.node_count());

        // Run 4-stage pruner
        let pruner = DomPruner::default_pruner();
        let pruned_dom = pruner.prune(&*dom_store, None);
        assert!(pruned_dom.contains("<html"), "Pruned DOM must contain <html>");
        assert!(pruned_dom.contains("<body"), "Pruned DOM must contain <body>");
        assert!(pruned_dom.contains("<h1"), "Pruned DOM must contain <h1>");
        assert!(pruned_dom.len() < 3000, "Pruned DOM must be compact within token limits");
        println!("  -> 4-Stage Pruned DOM:\n{}", pruned_dom.lines().take(6).collect::<Vec<_>>().join("\n"));
    }
    println!("  [PASS] Gate P5-GATE-04: Real DOM Cached & Pruned to Budget via 4-Stage Pipeline.");

    // =========================================================================
    // Gate P5-GATE-05: Untrusted Data Framing & BrowserObservation Assembly
    // =========================================================================
    println!("\n=== [Gate P5-GATE-05] Untrusted Data Framing & BrowserObservation Assembly ===");
    let obs = telemetry_coordinator
        .build_active_observation()
        .await
        .expect("Observation assembly failed")
        .expect("Active observation must be present");

    assert_eq!(obs.url, "https://example.com/");
    assert!(obs.formatted_untrusted_content.starts_with("<untrusted_web_content"), "Observation must start with <untrusted_web_content>");
    assert!(obs.formatted_untrusted_content.ends_with("</untrusted_web_content>"), "Observation must end with </untrusted_web_content>");
    assert!(obs.formatted_untrusted_content.contains("KAGE_RECURRING_PAGE_ERROR_XYZ"), "Observation must contain console errors");
    assert!(!obs.truncated, "Observation should fit comfortably within 4,000 token budget");
    assert!(obs.tokens_used <= 4000, "Tokens used ({} tokens) exceeded 4,000 budget!", obs.tokens_used);
    println!("  -> Assembled Observation Tokens Used: {} / 4,000", obs.tokens_used);
    println!("  [PASS] Gate P5-GATE-05: Untrusted Data Framing & BrowserObservation Budget Verified.");

    // =========================================================================
    // Gate P5-GATE-06: Sub-2ms Active Tab Context Swapping
    // =========================================================================
    println!("\n=== [Gate P5-GATE-06] Sub-2ms Active Tab Context Swapping ===");
    let tab2_id = TabId::new();
    let _tab2_telemetry = telemetry_coordinator.register_tab(tab2_id, "https://example.com/tab2").await;

    // Benchmark 100 context swaps to verify strict sub-2ms performance
    let mut max_swap_duration = Duration::ZERO;
    for _ in 0..100 {
        let t0 = Instant::now();
        telemetry_coordinator.set_active_tab(tab2_id).await;
        let active = telemetry_coordinator.get_active_telemetry().await.expect("Active telemetry missing");
        let elapsed = t0.elapsed();
        assert_eq!(active.tab_id, tab2_id);
        if elapsed > max_swap_duration {
            max_swap_duration = elapsed;
        }

        telemetry_coordinator.set_active_tab(tab1_id).await;
        let active1 = telemetry_coordinator.get_active_telemetry().await.expect("Active telemetry missing");
        assert_eq!(active1.tab_id, tab1_id);
    }

    println!("  -> Max Tab Swap Duration across 100 iterations: {:?}", max_swap_duration);
    assert!(
        max_swap_duration < Duration::from_millis(2),
        "Gate P5-GATE-06 Failed: Max tab swap duration {:?} exceeded 2ms benchmark",
        max_swap_duration
    );
    println!("  [PASS] Gate P5-GATE-06: Sub-2ms Tab Context Swapping Benchmark Verified.");

    println!("\n================================================================================");
    println!("=== KAGE PHASE 5: ALL 6 EMPIRICAL TELEMETRY GATES PASSED (100% LIVE CEF)   ===");
    println!("================================================================================\n");

    // Clean shutdown
    broker.shutdown();
    runtime.request_close_browser_by_id(browser_id_1, false).ok();
    parent_window.destroy();
}
