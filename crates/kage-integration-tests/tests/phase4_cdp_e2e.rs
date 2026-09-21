//! Phase 4 Empirical CEF-Backed CDP Integration Test Suite.
//!
//! Validates the physical Chromium Developer Plane against real multi-process CEF execution:
//! - **P4-E2E-01**: Real WebSocket <-> real CDP loopback proxy (handshake auth + Target.getTargets returning live Chromium targets)
//! - **P4-E2E-02**: Real Target discovery & attachment (real Chromium 32-hex targetId, Target.attachToTarget -> real sessionId, identity invariant)
//! - **P4-E2E-03**: Real DOM operation (anti-mock: DOM.getDocument -> root #document node, DOM.querySelector, DOM.getOuterHTML)
//! - **P4-E2E-04**: Real Runtime execution (Runtime.evaluate -> window.__KAGE_TEST = 42; 42 returning V8 number 42; document.title)
//! - **P4-E2E-05**: Real event causality (enable Runtime/Page/DOM, execute console.error, capture real Runtime.consoleAPICalled from Chromium)
//! - **P4-E2E-06**: Two-session isolation (two tabs, two attached sessions, evaluate differing variables, verify zero cross-talk)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kage_browser::{
    BrowserEventBus, ProfileId, ProfileManager, TabManager,
};
use kage_cdp::{CdpBroker, CdpClient, CdpEvent, TargetRouter};
use kage_engine::composition::{ChromeLayoutConfig, NativeSurfaceManager};
use kage_engine::coordinates::DpiContext;
use kage_engine::runtime::{CefEngineState, CefRuntime, RuntimeConfig};
use serde_json::json;
use tempfile::TempDir;

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
                let class_name: Vec<u16> = "KageCdpE2EWindowClass\0".encode_utf16().collect();
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("Connect error to 127.0.0.1:{}: {e}", port))?;

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
async fn test_phase4_empirical_cdp_e2e() {
    println!("\n================================================================================");
    println!("=== KAGE PHASE 4: EMPIRICAL CEF-BACKED CDP E2E VERIFICATION SUITE           ===");
    println!("================================================================================");

    // -------------------------------------------------------------------------
    // STEP 1: CEF Runtime Setup with Remote Debugging Port Enabled
    // -------------------------------------------------------------------------
    let temp_dir = TempDir::new().expect("create temp dir");
    let cdp_port = find_available_port();
    println!("  -> Allocated dynamic CEF Remote Debugging Port: {}", cdp_port);

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

    let parent_window = TestParentWindow::new("Kage Phase 4 CDP Verification Host");
    let parent_hwnd = parent_window.hwnd();

    let surface_manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
    let dpi = DpiContext::standard();
    let layout = surface_manager.compute_layout(1280, 720, &dpi).unwrap();
    let content_rect = layout.cef_content_rect;

    // Create Browser 1 (Tab 1) navigating to Example Domain
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

    // -------------------------------------------------------------------------
    // STEP 2: Poll Chromium DevTools HTTP Endpoint & Discover Debugger URL
    // -------------------------------------------------------------------------
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
    println!("  -> Chromium DevTools Version Response: {}", version_json);

    let ws_debugger_url = version_json["webSocketDebuggerUrl"]
        .as_str()
        .expect("webSocketDebuggerUrl in version response")
        .to_string();
    println!("  -> Real Chromium Browser WebSocket Debugger URL: {}", ws_debugger_url);

    // -------------------------------------------------------------------------
    // STEP 3: Bind KAGE CdpBroker Proxying to Real Chromium CDP Endpoint
    // -------------------------------------------------------------------------
    let session_nonce = format!("kage_sec_nonce_{}", uuid::Uuid::new_v4().simple());
    let (broker, broker_addr) = CdpBroker::bind_ephemeral_with_upstream(&session_nonce, ws_debugger_url)
        .await
        .expect("bind broker with upstream");

    println!("  -> KAGE CdpBroker bound to loopback: {}", broker_addr);
    let broker_ws_url = format!("ws://{}", broker_addr);

    // =========================================================================
    // GATE P4-E2E-01: Real WebSocket <-> Real CDP Endpoint (Anti-Mock Transport)
    // =========================================================================
    println!("\n=== [Gate P4-E2E-01] Real WebSocket <-> Real CDP Endpoint ===");
    // Connect client presenting x-kage-session-nonce
    let client = CdpClient::connect(&broker_ws_url, Some(&session_nonce))
        .await
        .expect("connect client with valid nonce");

    // Send Target.getTargets to real Chromium over the authenticated WebSocket
    let targets_res = client
        .call("Target.getTargets", json!({}))
        .await
        .expect("call Target.getTargets");

    println!("  -> Target.getTargets real Chromium response:\n     {}", serde_json::to_string(&targets_res).unwrap());

    let target_infos = targets_res["targetInfos"]
        .as_array()
        .expect("targetInfos array in Chromium response");
    assert!(!target_infos.is_empty(), "Chromium must return at least one active target");

    // Locate the page target corresponding to Browser 1
    let page_target = target_infos
        .iter()
        .find(|t| t["type"] == "page")
        .expect("Must find a page target in real Chromium target list");

    let real_target_id = page_target["targetId"]
        .as_str()
        .expect("targetId string")
        .to_string();
    let page_url = page_target["url"].as_str().unwrap_or("");
    let page_title = page_target["title"].as_str().unwrap_or("");

    println!("  -> Discovered Real Chromium Target: id='{}' type='{}' url='{}' title='{}'",
        real_target_id, page_target["type"], page_url, page_title);

    assert!(real_target_id.len() >= 16, "Real Chromium targetId must be a hex hash");
    println!("  [PASS] Gate P4-E2E-01: Real WebSocket <-> Real Chromium CDP Endpoint Verified.");

    // =========================================================================
    // GATE P4-E2E-02: Real Target Discovery + Attachment (INV-10, INV-12 Invariant)
    // =========================================================================
    println!("\n=== [Gate P4-E2E-02] Real Target Discovery & Attachment ===");
    let router = TargetRouter::new();
    let profile_manager = Arc::new(ProfileManager::new());
    let event_bus = BrowserEventBus::new(100);
    let tab_manager = TabManager::new(profile_manager.clone(), event_bus);

    let tab_id_1 = tab_manager
        .create_tab(ProfileId::personal(), test_url)
        .await
        .expect("create tab 1");
    tab_manager
        .bind_cef_browser(tab_id_1, browser_id_1)
        .await
        .expect("bind cef browser 1");
    tab_manager
        .bind_cdp_target(tab_id_1, &real_target_id)
        .await
        .expect("bind cdp target");
    router.bind_target(tab_id_1.0, &real_target_id).await;

    // Attach to the real Chromium target via protocol Target.attachToTarget
    let attach_res = client
        .call(
            "Target.attachToTarget",
            json!({ "targetId": real_target_id, "flatten": true }),
        )
        .await
        .expect("call Target.attachToTarget");

    println!("  -> Target.attachToTarget real Chromium response:\n     {}", serde_json::to_string(&attach_res).unwrap());

    let real_session_id = attach_res["sessionId"]
        .as_str()
        .expect("sessionId in attachToTarget response")
        .to_string();
    assert!(!real_session_id.is_empty(), "Real Chromium sessionId must be non-empty");

    println!("  -> TargetId: {} -> KAGE TabId: {} -> ProfileId: personal -> CefBrowserId: {} -> Real CDP SessionId: {}",
        real_target_id, tab_id_1, browser_id_1, real_session_id);

    // Verify Invariant INV-10 / INV-12: Tab BrowserIdentity is preserved and authoritative
    let tab_snap = tab_manager.get_tab(tab_id_1).await.expect("get tab snapshot");
    {
        let ident = tab_snap.identity.read().await;
        assert_eq!(ident.tab_id, tab_id_1);
        assert_eq!(ident.cef_browser_id, Some(browser_id_1));
        assert_eq!(ident.profile_id, ProfileId::personal());

        let cdp = tab_snap.cdp.read().await;
        assert_eq!(cdp.as_ref().unwrap().target_id, real_target_id);
    }
    println!("  [PASS] Gate P4-E2E-02: Real Target Discovery & Session Attachment Verified.");

    // =========================================================================
    // GATE P4-E2E-03: Real DOM Operation (Anti-Mock Root Node & Query)
    // =========================================================================
    println!("\n=== [Gate P4-E2E-03] Real DOM Operation (Anti-Mock) ===");
    let dom_res = client
        .call_session(Some(&real_session_id), "DOM.getDocument", json!({ "depth": 2 }))
        .await
        .expect("call DOM.getDocument");

    println!("  -> DOM.getDocument real Chromium response:\n     {}", serde_json::to_string(&dom_res).unwrap());

    // Physical Chromium assertion: nodeType == 9 (DOCUMENT_NODE), nodeName == "#document"
    let root = &dom_res["root"];
    assert_eq!(root["nodeType"], 9, "Root nodeType must be 9 (#document)");
    assert_eq!(root["nodeName"], "#document", "Root nodeName must be '#document'");
    let root_node_id = root["nodeId"].as_i64().expect("root nodeId");
    assert!(root_node_id > 0, "Root nodeId must be positive");

    // Execute DOM.querySelector against live DOM
    let query_res = client
        .call_session(
            Some(&real_session_id),
            "DOM.querySelector",
            json!({ "nodeId": root_node_id, "selector": "h1" }),
        )
        .await
        .expect("call DOM.querySelector for h1");

    println!("  -> DOM.querySelector real Chromium response: {}", query_res);
    let h1_node_id = query_res["nodeId"].as_i64().expect("h1 nodeId");
    assert!(h1_node_id > 0, "Must find h1 node in live page");

    // Query outer HTML of the h1 element
    let html_res = client
        .call_session(
            Some(&real_session_id),
            "DOM.getOuterHTML",
            json!({ "nodeId": h1_node_id }),
        )
        .await
        .expect("call DOM.getOuterHTML");

    println!("  -> DOM.getOuterHTML real Chromium response: {}", html_res);
    let outer_html = html_res["outerHTML"].as_str().expect("outerHTML string");
    assert!(outer_html.contains("Example Domain"), "Outer HTML must contain 'Example Domain'");
    println!("  [PASS] Gate P4-E2E-03: Real DOM Root Node and Query Verified.");

    // =========================================================================
    // GATE P4-E2E-04: Real Runtime Execution (V8 Remote Evaluation)
    // =========================================================================
    println!("\n=== [Gate P4-E2E-04] Real Runtime Execution (V8) ===");
    // 1. Evaluate deterministic computation
    let eval_res = client
        .call_session(
            Some(&real_session_id),
            "Runtime.evaluate",
            json!({ "expression": "window.__KAGE_TEST = 42; 42" }),
        )
        .await
        .expect("call Runtime.evaluate");

    println!("  -> Runtime.evaluate real Chromium response: {}", eval_res);
    assert_eq!(eval_res["result"]["type"], "number");
    assert_eq!(eval_res["result"]["value"], 42);

    // 2. Evaluate live document.title in V8
    let title_res = client
        .call_session(
            Some(&real_session_id),
            "Runtime.evaluate",
            json!({ "expression": "document.title" }),
        )
        .await
        .expect("call Runtime.evaluate for document.title");

    println!("  -> Runtime.evaluate document.title response: {}", title_res);
    assert_eq!(title_res["result"]["type"], "string");
    assert!(
        title_res["result"]["value"]
            .as_str()
            .unwrap()
            .starts_with("Example Domain"),
        "Title must start with 'Example Domain'"
    );
    println!("  [PASS] Gate P4-E2E-04: Real Runtime Execution Verified.");

    // =========================================================================
    // GATE P4-E2E-05: Real Event Causality (Chromium Event Pipeline)
    // =========================================================================
    println!("\n=== [Gate P4-E2E-05] Real Event Causality ===");
    let mut client_sub = client.subscribe_events();
    let mut broker_sub = broker.event_sender().subscribe();

    // Enable Runtime domain to receive console events
    client
        .call_session(Some(&real_session_id), "Runtime.enable", json!({}))
        .await
        .expect("call Runtime.enable");

    // Enable Page domain
    client
        .call_session(Some(&real_session_id), "Page.enable", json!({}))
        .await
        .expect("call Page.enable");

    println!("  -> Enabled Runtime and Page domains on real Chromium session");

    // Physically trigger a console.error event inside the live page via V8 execution
    let event_payload_text = "KAGE_PHYSICAL_EVENT_CAUSALITY_VERIFIED_7731";
    let trigger_res = client
        .call_session(
            Some(&real_session_id),
            "Runtime.evaluate",
            json!({
                "expression": format!("console.error('{}'); 'TRIGGERED'", event_payload_text)
            }),
        )
        .await
        .expect("call Runtime.evaluate console.error trigger");

    println!("  -> Triggered console.error execution: {}", trigger_res);

    // Await incoming event on client subscriber
    let start_wait = Instant::now();
    let mut received_console_event = false;

    while start_wait.elapsed() < Duration::from_secs(5) {
        tokio::select! {
            evt = client_sub.recv() => {
                if let Ok(CdpEvent::RuntimeConsoleApiCalled { console_type, args }) = evt {
                    println!("  -> Client received real Chromium event: Runtime.consoleAPICalled (type={}, args={:?})", console_type, args);
                    if console_type == "error" && args.iter().any(|a| a["value"] == event_payload_text) {
                        received_console_event = true;
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }

    assert!(received_console_event, "Client must receive real Chromium consoleAPICalled event");

    // Also assert direct broker broadcast subscriber captured the event
    let mut broker_captured = false;
    while let Ok(evt) = broker_sub.try_recv() {
        if let CdpEvent::RuntimeConsoleApiCalled { console_type, args } = evt {
            if console_type == "error" && args.iter().any(|a| a["value"] == event_payload_text) {
                broker_captured = true;
                break;
            }
        }
    }
    println!("  -> Broker direct subscriber captured event: {}", broker_captured);
    assert!(broker_captured, "Broker fanout must broadcast Chromium event to local subscribers");
    println!("  [PASS] Gate P4-E2E-05: Real Chromium Event Causality Verified.");

    // =========================================================================
    // GATE P4-E2E-06: Two-Session Isolation Across Live Tabs
    // =========================================================================
    println!("\n=== [Gate P4-E2E-06] Two-Session Isolation Across Live Tabs ===");
    // Create Browser 2 (Tab 2) navigating to the same host
    runtime
        .create_browser(parent_hwnd, &content_rect, test_url)
        .expect("create browser 2");

    let start_wait_b2 = Instant::now();
    while runtime.created_browser_ids().len() < 2 && start_wait_b2.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let all_browser_ids = runtime.created_browser_ids();
    assert!(all_browser_ids.len() >= 2, "Must have at least 2 active CEF browsers");
    let browser_id_2 = all_browser_ids[1];
    println!("  -> Created Browser 2 (CEF browser_id={})", browser_id_2);

    // Discover the new target via Target.getTargets
    let targets_res_2 = client
        .call("Target.getTargets", json!({}))
        .await
        .expect("call Target.getTargets for Tab 2");

    let target_infos_2 = targets_res_2["targetInfos"]
        .as_array()
        .expect("targetInfos array");

    // Find the second target that is not target 1
    let page_target_2 = target_infos_2
        .iter()
        .find(|t| t["type"] == "page" && t["targetId"] != real_target_id)
        .expect("Must find a distinct second page target");

    let real_target_id_2 = page_target_2["targetId"].as_str().unwrap().to_string();
    println!("  -> Discovered Tab 2 Target ID: {}", real_target_id_2);

    // Attach Session 2 to Tab 2
    let attach_res_2 = client
        .call(
            "Target.attachToTarget",
            json!({ "targetId": real_target_id_2, "flatten": true }),
        )
        .await
        .expect("attach to Tab 2");

    println!("  -> Target.attachToTarget real Chromium response:\n     {}", serde_json::to_string(&attach_res_2).unwrap());

    let real_session_id_2 = attach_res_2["sessionId"].as_str().unwrap().to_string();
    assert_ne!(real_session_id, real_session_id_2, "Sessions must have unique session IDs");

    // Bind Tab 2 identity in TabManager and TargetRouter (INV-10, INV-12)
    let tab_id_2 = tab_manager
        .create_tab(ProfileId::personal(), test_url)
        .await
        .expect("create tab 2");
    tab_manager
        .bind_cef_browser(tab_id_2, browser_id_2)
        .await
        .expect("bind cef browser 2");
    tab_manager
        .bind_cdp_target(tab_id_2, &real_target_id_2)
        .await
        .expect("bind cdp target 2");
    router.bind_target(tab_id_2.0, &real_target_id_2).await;

    println!("  -> Session 1 Provenance: TargetId={} -> TabId={} -> ProfileId=personal -> CefBrowserId={} -> SessionId={}",
        real_target_id, tab_id_1, browser_id_1, real_session_id);
    println!("  -> Session 2 Provenance: TargetId={} -> TabId={} -> ProfileId=personal -> CefBrowserId={} -> SessionId={}",
        real_target_id_2, tab_id_2, browser_id_2, real_session_id_2);

    // Mutate state in Session 1: window.__KAGE_SESSION_MARKER = 'TAB_01_EXCLUSIVE'
    client
        .call_session(
            Some(&real_session_id),
            "Runtime.evaluate",
            json!({ "expression": "window.__KAGE_SESSION_MARKER = 'TAB_01_EXCLUSIVE';" }),
        )
        .await
        .expect("set marker in Tab 1");

    // Mutate state in Session 2: window.__KAGE_SESSION_MARKER = 'TAB_02_EXCLUSIVE'
    client
        .call_session(
            Some(&real_session_id_2),
            "Runtime.evaluate",
            json!({ "expression": "window.__KAGE_SESSION_MARKER = 'TAB_02_EXCLUSIVE';" }),
        )
        .await
        .expect("set marker in Tab 2");

    // Assert Session 1 observes only Tab 1's marker
    let read_1 = client
        .call_session(
            Some(&real_session_id),
            "Runtime.evaluate",
            json!({ "expression": "window.__KAGE_SESSION_MARKER" }),
        )
        .await
        .expect("read marker in Tab 1");
    assert_eq!(read_1["result"]["value"], "TAB_01_EXCLUSIVE");

    // Assert Session 2 observes only Tab 2's marker
    let read_2 = client
        .call_session(
            Some(&real_session_id_2),
            "Runtime.evaluate",
            json!({ "expression": "window.__KAGE_SESSION_MARKER" }),
        )
        .await
        .expect("read marker in Tab 2");
    assert_eq!(read_2["result"]["value"], "TAB_02_EXCLUSIVE");

    println!("  -> Session 1 read: {}", read_1["result"]["value"]);
    println!("  -> Session 2 read: {}", read_2["result"]["value"]);
    println!("  -> Verified zero cross-talk between Session 1 (Tab 1) and Session 2 (Tab 2)!");
    println!("  [PASS] Gate P4-E2E-06: Two-Session Isolation Across Live Tabs Verified.");

    // -------------------------------------------------------------------------
    // CLEAN TEARDOWN
    // -------------------------------------------------------------------------
    broker.shutdown();
    runtime.request_close_browser_by_id(browser_id_1, false).ok();
    runtime.request_close_browser_by_id(browser_id_2, false).ok();
    parent_window.destroy();

    println!("\n================================================================================");
    println!("=== KAGE PHASE 4: ALL 6 EMPIRICAL CDP GATES PASSED (100% REAL CHROMIUM)      ===");
    println!("================================================================================\n");
}
