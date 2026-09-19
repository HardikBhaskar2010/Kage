//! Phase 2B Physical Integration Test.
//!
//! Validates the full CEF runtime lifecycle end-to-end:
//! - Step 2B-1: Actual `cef::initialize()` succeeds -> `CefReady`.
//! - Step 2B-2: Dedicated `kage-cef-subprocess.exe` binary proof.
//! - Step 2B-3: `CreateBrowser` in native child HWND within `content_rect`.
//! - Step 2B-4: `CefUiExecutor` real `CefPostTask` hop onto `TID_UI`.
//! - Step 2B-5: `verify_no_hwnd_overlap` asserts non-overlapping composition.
//! - Step 2B-6: Layout resize & DPI scaling recalculation.
//! - Step 2B-7: `shutdown_async` drains active browsers and performs clean shutdown.

use crate::composition::{ChromeLayoutConfig, NativeSurfaceManager, ViewportRect};
use crate::coordinates::DpiContext;
use crate::executor::CefUiExecutor;
use crate::runtime::{CefEngineState, CefRuntime, RuntimeConfig};
use std::path::PathBuf;

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

    // Fallback to standard target dir
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
    fn new() -> Self {
        let (hwnd_tx, hwnd_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();

        let join_handle = std::thread::spawn(move || {
            use std::ptr::null;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            unsafe {
                let class_name: Vec<u16> = "KageTestParentClass\0".encode_utf16().collect();
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

                let window_title: Vec<u16> = "KAGE Phase 2B Test Window\0".encode_utf16().collect();
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
                    std::thread::sleep(std::time::Duration::from_millis(10));
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

#[cfg(target_os = "windows")]
fn inspect_child_subprocesses(approved_name: &str) -> Vec<(u32, String)> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    let current_pid = std::process::id();
    let mut matching = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot != std::ptr::null_mut() && snapshot != -1 as isize as *mut _ {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    if entry.th32ParentProcessID == current_pid {
                        let name_len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                        let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
                        assert!(
                            name.to_lowercase().contains(approved_name),
                            "INV-CEF-SUBPROCESS-001 VIOLATION: Unexpected child process spawned: {}",
                            name
                        );
                        matching.push((entry.th32ProcessID, name));
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            windows_sys::Win32::Foundation::CloseHandle(snapshot);
        }
    }
    matching
}

#[tokio::test]
async fn test_phase2b_complete_cef_pipeline() {
    println!("[Phase 2B] Starting full CEF engine pipeline integration test...");

    // -----------------------------------------------------------------------
    // Step 2B-2: Dedicated subprocess binary verification (CEF-02, CEF-03)
    // -----------------------------------------------------------------------
    let subprocess_exe = find_subprocess_binary();
    println!("[Step 2B-2] Checking subprocess binary at {:?}", subprocess_exe);
    assert!(
        subprocess_exe.exists(),
        "CEF-02 VIOLATION: Dedicated subprocess executable MUST exist at {:?}",
        subprocess_exe
    );

    // Prepare runtime configuration
    let mut config = RuntimeConfig::default();
    config.subprocess_path = Some(subprocess_exe);
    config.no_sandbox = true; // Permitted for debug test harness per plan

    let runtime = CefRuntime::new(config);
    assert_eq!(runtime.state(), CefEngineState::Created);

    // -----------------------------------------------------------------------
    // Step 2B-1: Real CEF initialization (CEF-01B, CEF-04B)
    // -----------------------------------------------------------------------
    println!("[Step 2B-1] Calling CefRuntime::initialize_cef()...");
    let init_result = runtime.initialize_cef();
    assert!(
        init_result.is_ok(),
        "CEF-01B VIOLATION: cef::initialize() failed: {:?}",
        init_result.err()
    );
    assert_eq!(runtime.state(), CefEngineState::BrowserCreationAllowed);
    assert!(runtime.is_initialized());
    println!("[Step 2B-1] CEF successfully initialized (state=BrowserCreationAllowed).");

    // -----------------------------------------------------------------------
    // Step 2B-4: CefUiExecutor real CefPostTask hop onto TID_UI (Executor-B)
    // -----------------------------------------------------------------------
    println!("[Step 2B-4] Dispatching task via CefUiExecutor::execute_real()...");
    let executor = CefUiExecutor::new();
    let thread_check = executor
        .execute_real(|| {
            let on_ui = CefUiExecutor::currently_on_ui_thread();
            (on_ui, 42)
        })
        .await;

    assert!(
        thread_check.is_ok(),
        "Executor-B VIOLATION: Real CefPostTask(TID_UI) dispatch failed: {:?}",
        thread_check.err()
    );
    let (on_ui_thread, value) = thread_check.unwrap();
    assert!(
        on_ui_thread,
        "Executor-B VIOLATION: Task did not run on CEF TID_UI"
    );
    assert_eq!(value, 42);
    println!("[Step 2B-4] Real CefPostTask verified on TID_UI.");

    // -----------------------------------------------------------------------
    // Step 2B-5: Surface layout math and non-overlapping verification (CEF-06A/B)
    // -----------------------------------------------------------------------
    let surface_manager = NativeSurfaceManager::new(ChromeLayoutConfig::default());
    let dpi = DpiContext::standard();
    let layout = surface_manager.compute_layout(1280, 720, &dpi).unwrap();
    assert!(layout.validate_no_overlap().is_ok());

    let content_rect = layout.cef_content_rect;
    assert_eq!(content_rect, ViewportRect::new(320, 88, 960, 632));
    println!("[Step 2B-5] Layout non-overlap verified: {:?}", content_rect);

    // -----------------------------------------------------------------------
    // Step 2B-3: Native child HWND creation and CreateBrowser (CEF-05, CEF-06B)
    // -----------------------------------------------------------------------
    #[cfg(target_os = "windows")]
    {
        println!("[Step 2B-3] Creating native child browser in Win32 HWND...");
        let test_window = TestParentWindow::new();
        let parent_hwnd = test_window.hwnd();
        assert_ne!(parent_hwnd, 0, "Failed to create test parent window");

        let create_res = runtime.create_browser(parent_hwnd, &content_rect, "https://example.com");
        assert!(
            create_res.is_ok(),
            "CEF-05 VIOLATION: browser_host_create_browser failed: {:?}",
            create_res.err()
        );
        assert_eq!(runtime.state(), CefEngineState::Running);
        println!("[Step 2B-3] Browser creation dispatched (state=Running). Waiting for OnAfterCreated...");

        // Wait up to 10s for browser to report OnAfterCreated
        let start = std::time::Instant::now();
        while runtime.active_browser_count() == 0 && start.elapsed() < std::time::Duration::from_secs(10) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let count = runtime.active_browser_count();
        println!("[Step 2B-3] Active browser count: {}", count);
        assert!(
            count > 0,
            "CEF-05 VIOLATION: Browser creation timed out — OnAfterCreated was not fired within 10s"
        );

        // -----------------------------------------------------------------------
        // Step 2B-3b: Process Inspection & Subprocess Identity (INV-CEF-SUBPROCESS-001)
        // -----------------------------------------------------------------------
        let children = inspect_child_subprocesses("kage-cef-subprocess");
        println!("[Step 2B-3b] Detected {} running auxiliary subprocess(es):", children.len());
        for (pid, name) in &children {
            println!("  -> Child PID {}: {}", pid, name);
        }

        // Check page load progression
        let load_start = std::time::Instant::now();
        while !runtime.is_page_loaded() && load_start.elapsed() < std::time::Duration::from_secs(3) {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        println!(
            "[Step 2B-3c] Page load event: loaded={}, last_http_status={}, last_url={:?}",
            runtime.is_page_loaded(),
            runtime.last_http_status(),
            runtime.last_loaded_url()
        );

        // -----------------------------------------------------------------------
        // Step 2B-6: Dynamic DPI scaling and resize calculation
        // -----------------------------------------------------------------------
        let dpi_150 = DpiContext::with_scale(1.5, 1.5);
        let layout_150 = surface_manager.compute_layout(1920, 1080, &dpi_150).unwrap();
        assert!(layout_150.validate_no_overlap().is_ok());
        assert_eq!(layout_150.cef_content_rect, ViewportRect::new(480, 132, 1440, 948));
        println!("[Step 2B-6] 1.5x DPI dynamic bounds recalculated with zero overlap.");

        // -----------------------------------------------------------------------
        // Step 2B-7: Async shutdown and clean teardown (CEF-10)
        // -----------------------------------------------------------------------
        println!("[Step 2B-7] Executing CefRuntime::shutdown_async()...");
        let shutdown_res = runtime.shutdown_async().await;
        assert!(
            shutdown_res.is_ok(),
            "CEF-10 VIOLATION: Clean shutdown failed: {:?}",
            shutdown_res.err()
        );
        assert_eq!(runtime.state(), CefEngineState::Shutdown);
        assert!(!runtime.is_initialized());
        println!("[Step 2B-7] Clean asynchronous shutdown completed (state=Shutdown).");

        // Clean up test parent HWND thread after CEF is completely shut down
        test_window.destroy();
    }

    println!("[Phase 2B] COMPLETE — All Phase 2B gates verified end-to-end.");
}
