//! Host library root — exposes [`run`] and IPC modules for testability.
//!
//! # CEF Lifecycle (Phase 2)
//!
//! On startup, before Tauri initialises:
//! 1. [`CefRuntime`] validates configuration and creates cache directories.
//! 2. CEF subsystem will be initialised on the main thread when Tauri's
//!    setup callback fires.
//! 3. `NativeSurfaceManager` manages physical WebView2 and CEF HWND bounds.
//! 4. On window close, CEF teardown follows the engine state machine
//!    (`CloseRequested → BrowserClosing → BrowserClosed → CefShutdownPending → Shutdown`).

pub mod ipc;

use std::sync::Arc;
use kage_core::ToolBus;
use kage_cdp::CdpBroker;
use kage_storage::AuditDb;
use kage_engine::{CefRuntime, ChromeLayoutConfig, NativeSurfaceManager, RuntimeConfig};

/// Initialise and run the Tauri application.
pub fn run() {
    // Initialise structured logging from RUST_LOG env var.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kage=info".parse().unwrap()),
        )
        .init();

    tracing::info!("KAGE host starting — v{}", env!("CARGO_PKG_VERSION"));

    // -----------------------------------------------------------------------
    // [CEF-01] Validate and prepare CEF runtime configuration BEFORE Tauri
    // starts. This MUST happen on the main OS thread.
    // -----------------------------------------------------------------------
    let subprocess_path = std::env::current_exe().ok().and_then(|mut p| {
        p.pop();
        let sub = p.join("kage-cef-subprocess.exe");
        if sub.exists() {
            Some(sub)
        } else {
            let fallback = std::path::PathBuf::from("target/x86_64-pc-windows-msvc/debug/kage-cef-subprocess.exe");
            if fallback.exists() {
                Some(fallback)
            } else {
                None
            }
        }
    });

    let mut cef_config = RuntimeConfig::default();
    cef_config.subprocess_path = subprocess_path;
    cef_config.no_sandbox = true; // Permitted for dev profile; release enforces false per CEF-03b
    let cef_runtime = Arc::new(CefRuntime::new(cef_config));

    if let Err(e) = cef_runtime.validate_config() {
        tracing::error!("CEF runtime configuration is invalid: {e}. Halting.");
        std::process::exit(1);
    }

    if let Err(e) = cef_runtime.ensure_cache_directories() {
        tracing::error!("Failed to create CEF cache directories: {e}. Halting.");
        std::process::exit(1);
    }

    if let Err(e) = cef_runtime.initialize_cef() {
        tracing::error!("Failed to initialize CEF subsystem: {e}. Halting.");
        std::process::exit(1);
    }

    tracing::info!(
        "CEF runtime initialized: state={:?}",
        cef_runtime.state()
    );

    // Create the NativeSurfaceManager with the default chrome layout configuration.
    // This is the production layout authority: top bar = 88 CSS px, sidebar = 320 CSS px.
    let surface_manager = Arc::new(NativeSurfaceManager::new(ChromeLayoutConfig::default()));

    // -----------------------------------------------------------------------
    // Determine audit database location (%LOCALAPPDATA%\KAGE\security_audit.db)
    // -----------------------------------------------------------------------
    let audit_path = if let Ok(custom) = std::env::var("KAGE_AUDIT_DB") {
        std::path::PathBuf::from(custom)
    } else if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let mut dir = std::path::PathBuf::from(local_app_data);
        dir.push("KAGE");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("security_audit.db");
        dir
    } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let mut dir = std::path::PathBuf::from(home);
        dir.push(".kage");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("security_audit.db");
        dir
    } else {
        std::path::PathBuf::from("security_audit.db")
    };

    let audit_db = Arc::new(
        AuditDb::open(audit_path.to_str().unwrap_or("security_audit.db")).unwrap_or_else(|e| {
            tracing::warn!("Could not open persistent audit DB ({e}); using in-memory audit DB");
            AuditDb::open_in_memory().expect("In-memory audit DB must open")
        }),
    );

    // Reconcile any non-terminal audit intents orphaned by a previous crash or abrupt exit.
    let host_instance_id = format!("host_{}", uuid::Uuid::new_v4().simple());
    tracing::info!("KAGE host instance ID: {host_instance_id}");

    let reconcile_db = audit_db.clone();
    tauri::async_runtime::block_on(async move {
        match reconcile_db.reconcile_unresolved_intents().await {
            Ok(reconciled) if !reconciled.is_empty() => {
                tracing::warn!(
                    "Startup audit reconciliation: marked {} orphaned intents as Unresolved: {:?}",
                    reconciled.len(),
                    reconciled
                );
            }
            Ok(_) => {
                tracing::info!("Startup audit reconciliation: ledger clean, zero orphaned intents");
            }
            Err(e) => {
                tracing::error!("Startup audit reconciliation failed: {e}");
            }
        }
    });

    // Construct core singletons wired to real AuditSink.
    let tool_bus = Arc::new(
        ToolBus::new()
            .with_host_instance_id(host_instance_id)
            .with_audit_sink(audit_db.clone()),
    );
    let cdp_broker = Arc::new(CdpBroker::new());

    // Phase 3: Browser Control Plane (Tabs, Profiles, Event Bus)
    let profile_manager = Arc::new(kage_browser::ProfileManager::new());
    let event_bus = kage_browser::BrowserEventBus::new(64);
    let tab_manager = Arc::new(kage_browser::TabManager::new(profile_manager, event_bus));

    tracing::info!("ToolBus, TabManager, and ProfileManager initialized");

    tauri::Builder::default()
        .setup({
            let cef_runtime = cef_runtime.clone();
            let surface_manager = surface_manager.clone();
            let tab_manager = tab_manager.clone();
            move |app| {
                use tauri::Manager;
                let window = app.get_webview_window("main").expect("main window must exist");
                #[cfg(windows)]
                {
                    let hwnd = window.hwnd().expect("HWND must exist");
                    let hwnd_isize = hwnd.0 as isize;
                    tracing::info!("[Tauri setup] Main window HWND: {} ({:?})", hwnd_isize, hwnd);

                    let scale_factor = window.scale_factor().unwrap_or(1.0);
                    let inner_size = window.inner_size().unwrap_or(tauri::PhysicalSize { width: 1440, height: 900 });

                    let layout = surface_manager
                        .update_layout(inner_size.width as i32, inner_size.height as i32, scale_factor)
                        .expect("Initial layout computation must succeed");

                    tracing::info!(
                        "[Tauri setup] DualSurfaceLayout: top_chrome={:?}, cef_content={:?}",
                        layout.top_chrome_rect,
                        layout.cef_content_rect
                    );

                    let initial_url = "https://example.com";
                    tracing::info!(
                        "[Tauri setup] Creating child CEF browser in HWND {} at {:?} for {}",
                        hwnd_isize,
                        layout.cef_content_rect,
                        initial_url
                    );

                    match cef_runtime.create_browser(hwnd_isize, &layout.cef_content_rect, initial_url) {
                        Ok(()) => {
                            tracing::info!("[Tauri setup] Child CEF browser creation dispatched successfully (CEF-06C)");
                            // Phase 3: Register initial tab in TabManager and attach child surface
                            let tm = tab_manager.clone();
                            tauri::async_runtime::block_on(async move {
                                if let Ok(tab_id) = tm.create_tab(kage_browser::ProfileId::personal(), initial_url).await {
                                    let _ = tm.attach_surface_hwnd(tab_id, hwnd_isize).await;
                                    tracing::info!("[Tauri setup] Initial tab registered in TabManager: {tab_id}");
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("[Tauri setup] Failed to create child CEF browser: {e}");
                        }
                    }
                }
                Ok(())
            }
        })
        .on_window_event({
            let surface_manager = surface_manager.clone();
            let cef_runtime = cef_runtime.clone();
            move |window, event| {
                match event {
                    tauri::WindowEvent::Resized(physical_size) => {
                        let scale_factor = window.scale_factor().unwrap_or(1.0);
                        if let Ok(layout) = surface_manager.update_layout(
                            physical_size.width as i32,
                            physical_size.height as i32,
                            scale_factor,
                        ) {
                            tracing::debug!(
                                "[WindowEvent::Resized] CEF content rect: {:?}",
                                layout.cef_content_rect
                            );
                        }
                    }
                    tauri::WindowEvent::CloseRequested { .. } => {
                        tracing::info!("[WindowEvent::CloseRequested] Window close requested — initiating CEF shutdown (CEF-10A/B)...");
                        let runtime = cef_runtime.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = runtime.shutdown_async().await {
                                tracing::error!("[WindowEvent::CloseRequested] CEF shutdown error: {e}");
                            } else {
                                tracing::info!("[WindowEvent::CloseRequested] CEF shutdown successfully finished");
                            }
                        });
                    }
                    _ => {}
                }
            }
        })
        .manage(tool_bus)
        .manage(cdp_broker)
        .manage(audit_db)
        .manage(cef_runtime)              // CefRuntime: lifecycle state machine
        .manage(surface_manager)          // NativeSurfaceManager: physical HWND bounds authority
        .manage(tab_manager)              // TabManager: tab lifecycle coordinator (Phase 3)
        .invoke_handler(tauri::generate_handler![
            ipc::tool_dispatch,
            ipc::get_cdp_connection,
            ipc::get_cdp_nonce,
            ipc::create_tab,
            ipc::close_tab,
            ipc::switch_tab,
            ipc::navigate_to,
            ipc::go_back,
            ipc::go_forward,
            ipc::reload_tab,
            ipc::list_tabs,
            ipc::get_active_tab,
            ipc::inspect_at_location,
            ipc::inspect_node,
            ipc::eval_js,
            ipc::get_audit_logs,
            ipc::verify_audit_chain,
            ipc::sync_viewport_bounds,
            ipc::get_engine_state,
        ])
        .run(tauri::generate_context!())
        .expect("KAGE Tauri application failed to start");
}
