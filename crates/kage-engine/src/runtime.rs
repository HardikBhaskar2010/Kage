//! CEF Runtime lifecycle manager (Phase 2).
//!
//! Enforces:
//! - **CEF-01**: Main thread initialization with valid `CefSettings` (`root_cache_path` + `cache_path`).
//! - **CEF-03b**: Sandbox integrity gate (release builds strictly forbid disabling sandbox).
//! - **CEF-04**: Decoupled message loop (`multi_threaded_message_loop = true`).
//! - **CEF-10**: Non-blocking asynchronous shutdown via `OnBeforeClose` tracking.

use crate::errors::EngineError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::warn;

/// Runtime configuration for the Chromium Embedded Framework host.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Parent root cache directory: `%LOCALAPPDATA%\KAGE\cef\`.
    pub root_cache_path: PathBuf,
    /// Profile-specific cache directory: `%LOCALAPPDATA%\KAGE\cef\profiles\default\`.
    pub cache_path: PathBuf,
    /// Path to the dedicated subprocess helper (`kage-cef-subprocess.exe`).
    pub subprocess_path: Option<PathBuf>,
    /// Whether the Chromium sandbox is disabled (CEF-03b).
    pub no_sandbox: bool,
    /// Whether CEF operates its own UI pump on a dedicated OS thread.
    pub multi_threaded_message_loop: bool,
    /// Phase-2 default: persist session cookies across runs (profile-configurable in Phase 3).
    pub persist_session_cookies: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let root_cache_path = local_app_data.join("KAGE").join("cef");
        let cache_path = root_cache_path.join("profiles").join("default");

        Self {
            root_cache_path,
            cache_path,
            subprocess_path: None,
            // In debug/dev mode, sandbox may be relaxed for developer iteration;
            // in release builds, CEF-03b enforces sandbox activation.
            no_sandbox: cfg!(debug_assertions),
            multi_threaded_message_loop: true,
            persist_session_cookies: true,
        }
    }
}

/// Explicit lifecycle state machine for KAGE CEF host engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CefEngineState {
    Created,
    CefInitializing,
    CefReady,
    CefContextReady,
    BrowserCreationAllowed,
    BrowserCreating,
    BrowserReady,
    Running,
    CloseRequested,
    BrowserClosing,
    BrowserClosed,
    CefShutdownPending,
    Shutdown,
    Failed,
    Degraded,
}

use cef::*;

wrap_life_span_handler! {
    struct KageLifeSpanHandler {
        active_browsers: Arc<AtomicUsize>,
        browser_hosts: Arc<std::sync::Mutex<Vec<BrowserHost>>>,
        last_browser_id: Arc<std::sync::atomic::AtomicI32>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let prev = self.active_browsers.fetch_add(1, Ordering::SeqCst);
            if let Some(b) = browser {
                let id = b.identifier();
                self.last_browser_id.store(id, Ordering::SeqCst);
                println!("[KageLifeSpanHandler] on_after_created triggered (browser_id={}, prev_count={}, new_count={})", id, prev, prev + 1);
                if let Some(host) = b.host() {
                    if let Ok(mut lock) = self.browser_hosts.lock() {
                        lock.push(host);
                    }
                }
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            let prev = self.active_browsers.fetch_sub(1, Ordering::SeqCst);
            println!("[KageLifeSpanHandler] on_before_close triggered (prev_count={}, new_count={})", prev, prev - 1);
        }
    }
}

wrap_load_handler! {
    struct KageLoadHandler {
        is_loading: Arc<AtomicBool>,
        load_started: Arc<AtomicBool>,
        load_start_url: Arc<std::sync::RwLock<Option<String>>>,
        page_loaded: Arc<AtomicBool>,
        last_http_status: Arc<std::sync::atomic::AtomicI32>,
        last_loaded_url: Arc<std::sync::RwLock<Option<String>>>,
        load_failed: Arc<AtomicBool>,
        last_error_code: Arc<std::sync::atomic::AtomicI32>,
        last_error_text: Arc<std::sync::RwLock<Option<String>>>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            _can_go_back: ::std::os::raw::c_int,
            _can_go_forward: ::std::os::raw::c_int,
        ) {
            println!("[KageLoadHandler] on_loading_state_change: is_loading={}", is_loading);
            self.is_loading.store(is_loading != 0, Ordering::SeqCst);
        }

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            self.load_started.store(true, Ordering::SeqCst);
            if let Some(f) = frame {
                let url_str = CefStringUtf16::from(&f.url()).to_string();
                if let Ok(mut lock) = self.load_start_url.write() {
                    *lock = Some(url_str.clone());
                }
                println!("[KageLoadHandler] on_load_start: url={}", url_str);
            }
        }

        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            self.last_http_status.store(http_status_code as i32, Ordering::SeqCst);
            self.page_loaded.store(true, Ordering::SeqCst);
            if let Some(f) = frame {
                let url_str = CefStringUtf16::from(&f.url()).to_string();
                if let Ok(mut lock) = self.last_loaded_url.write() {
                    *lock = Some(url_str.clone());
                }
                println!("[KageLoadHandler] on_load_end: url={}, status={}", url_str, http_status_code);
                let js_code = CefString::from("document.title = document.title + ' [KAGE_CEF_OK]';");
                let script_url = CefString::from("about:blank");
                f.execute_java_script(Some(&js_code), Some(&script_url), 0);
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let code_i32 = *error_code.as_ref() as i32;
            self.load_failed.store(true, Ordering::SeqCst);
            self.last_error_code.store(code_i32, Ordering::SeqCst);
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
            if let Ok(mut lock) = self.last_error_text.write() {
                *lock = Some(text.clone());
            }
            println!("[KageLoadHandler] on_load_error: error_code={}, text={}, url={}", code_i32, text, url);
        }
    }
}

wrap_request_handler! {
    struct KageRequestHandler {
        renderer_terminated: Arc<AtomicBool>,
        termination_status: Arc<std::sync::atomic::AtomicI32>,
        termination_error_code: Arc<std::sync::atomic::AtomicI32>,
    }

    impl RequestHandler {
        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            status: TerminationStatus,
            error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            let status_i32 = *status.as_ref() as i32;
            println!("[KageRequestHandler] on_render_process_terminated: status={:?}, error_code={}", status, error_code);
            self.termination_status.store(status_i32, Ordering::SeqCst);
            self.termination_error_code.store(error_code as i32, Ordering::SeqCst);
            self.renderer_terminated.store(true, Ordering::SeqCst);
        }
    }
}

wrap_client! {
    struct KageBrowserClient {
        life_span_handler: LifeSpanHandler,
        load_handler: LoadHandler,
        request_handler: Option<RequestHandler>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span_handler.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            self.request_handler.clone()
        }
    }
}

impl CefEngineState {
    /// Validate valid state transitions according to KAGE lifecycle rules.
    pub fn can_transition_to(&self, next: CefEngineState) -> bool {
        match (self, next) {
            // Error transitions reachable from active operations
            (_, CefEngineState::Failed) => true,
            (CefEngineState::Running, CefEngineState::Degraded) => true,
            // Nominal progression
            (CefEngineState::Created, CefEngineState::CefInitializing) => true,
            (CefEngineState::CefInitializing, CefEngineState::CefReady) => true,
            (CefEngineState::CefReady, CefEngineState::CefContextReady) => true,
            (CefEngineState::CefReady, CefEngineState::CloseRequested) => true,
            (CefEngineState::CefReady, CefEngineState::CefShutdownPending) => true,
            (CefEngineState::CefContextReady, CefEngineState::BrowserCreationAllowed) => true,
            (CefEngineState::CefContextReady, CefEngineState::CloseRequested) => true,
            (CefEngineState::CefContextReady, CefEngineState::CefShutdownPending) => true,
            (CefEngineState::BrowserCreationAllowed, CefEngineState::BrowserCreating) => true,
            (CefEngineState::BrowserCreationAllowed, CefEngineState::CloseRequested) => true,
            (CefEngineState::BrowserCreationAllowed, CefEngineState::CefShutdownPending) => true,
            (CefEngineState::BrowserCreating, CefEngineState::BrowserReady) => true,
            (CefEngineState::BrowserReady, CefEngineState::Running) => true,
            (CefEngineState::Running, CefEngineState::CloseRequested) => true,
            (CefEngineState::CloseRequested, CefEngineState::BrowserClosing) => true,
            (CefEngineState::CloseRequested, CefEngineState::CefShutdownPending) => true,
            (CefEngineState::BrowserClosing, CefEngineState::BrowserClosed) => true,
            (CefEngineState::BrowserClosed, CefEngineState::CefShutdownPending) => true,
            (CefEngineState::CefShutdownPending, CefEngineState::Shutdown) => true,
            _ => false,
        }
    }
}

/// Central lifecycle coordinator for CEF inside KAGE.
pub struct CefRuntime {
    state: std::sync::RwLock<CefEngineState>,
    initialized: AtomicBool,
    active_browsers: Arc<AtomicUsize>,
    browser_hosts: Arc<std::sync::Mutex<Vec<BrowserHost>>>,
    last_browser_id: Arc<std::sync::atomic::AtomicI32>,
    is_loading: Arc<AtomicBool>,
    load_started: Arc<AtomicBool>,
    load_start_url: Arc<std::sync::RwLock<Option<String>>>,
    page_loaded: Arc<AtomicBool>,
    last_http_status: Arc<std::sync::atomic::AtomicI32>,
    last_loaded_url: Arc<std::sync::RwLock<Option<String>>>,
    load_failed: Arc<AtomicBool>,
    last_error_code: Arc<std::sync::atomic::AtomicI32>,
    last_error_text: Arc<std::sync::RwLock<Option<String>>>,
    renderer_terminated: Arc<AtomicBool>,
    termination_status: Arc<std::sync::atomic::AtomicI32>,
    termination_error_code: Arc<std::sync::atomic::AtomicI32>,
    config: RuntimeConfig,
}

impl CefRuntime {
    /// Create a new runtime coordinator with given configuration.
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            state: std::sync::RwLock::new(CefEngineState::Created),
            initialized: AtomicBool::new(false),
            active_browsers: Arc::new(AtomicUsize::new(0)),
            browser_hosts: Arc::new(std::sync::Mutex::new(Vec::new())),
            last_browser_id: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            is_loading: Arc::new(AtomicBool::new(false)),
            load_started: Arc::new(AtomicBool::new(false)),
            load_start_url: Arc::new(std::sync::RwLock::new(None)),
            page_loaded: Arc::new(AtomicBool::new(false)),
            last_http_status: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            last_loaded_url: Arc::new(std::sync::RwLock::new(None)),
            load_failed: Arc::new(AtomicBool::new(false)),
            last_error_code: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            last_error_text: Arc::new(std::sync::RwLock::new(None)),
            renderer_terminated: Arc::new(AtomicBool::new(false)),
            termination_status: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            termination_error_code: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            config,
        }
    }

    /// Read the current lifecycle state of the CEF engine.
    pub fn state(&self) -> CefEngineState {
        *self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Transition to a new lifecycle state, validating the transition invariants.
    pub fn transition_to(&self, next: CefEngineState) -> Result<(), EngineError> {
        let mut guard = self.state.write().map_err(|e| {
            EngineError::Lifecycle(format!("State lock poisoned during transition: {e}"))
        })?;

        if !guard.can_transition_to(next) {
            return Err(EngineError::Lifecycle(format!(
                "Illegal engine state transition: {:?} -> {:?}",
                *guard, next
            )));
        }

        *guard = next;
        Ok(())
    }

    /// Access the active browser count counter for LifespanHandler registration.
    pub fn active_browser_counter(&self) -> Arc<AtomicUsize> {
        self.active_browsers.clone()
    }

    /// Return the active browser count.
    pub fn active_browser_count(&self) -> usize {
        self.active_browsers.load(Ordering::SeqCst)
    }

    /// Most recently assigned CEF browser ID.
    pub fn last_browser_id(&self) -> i32 {
        self.last_browser_id.load(Ordering::SeqCst)
    }

    /// Whether a load is currently in progress.
    pub fn is_loading(&self) -> bool {
        self.is_loading.load(Ordering::SeqCst)
    }

    /// Whether on_load_start has fired for the current load.
    pub fn is_load_started(&self) -> bool {
        self.load_started.load(Ordering::SeqCst)
    }

    /// Main frame URL observed at on_load_start.
    pub fn load_start_url(&self) -> Option<String> {
        self.load_start_url.read().ok().and_then(|g| g.clone())
    }

    /// Whether on_load_error has fired.
    pub fn is_load_failed(&self) -> bool {
        self.load_failed.load(Ordering::SeqCst)
    }

    /// Last observed error code from on_load_error.
    pub fn last_error_code(&self) -> i32 {
        self.last_error_code.load(Ordering::SeqCst)
    }

    /// Last observed error text from on_load_error.
    pub fn last_error_text(&self) -> Option<String> {
        self.last_error_text.read().ok().and_then(|g| g.clone())
    }

    /// Whether on_render_process_terminated has fired.
    pub fn is_renderer_terminated(&self) -> bool {
        self.renderer_terminated.load(Ordering::SeqCst)
    }

    /// Raw termination status code from on_render_process_terminated.
    pub fn last_termination_status(&self) -> i32 {
        self.termination_status.load(Ordering::SeqCst)
    }

    /// Termination error code from on_render_process_terminated.
    pub fn termination_error_code(&self) -> i32 {
        self.termination_error_code.load(Ordering::SeqCst)
    }

    /// Validate runtime configuration before passing to CEF (CEF-01, CEF-03b).
    pub fn validate_config(&self) -> Result<(), EngineError> {
        // Gate CEF-03b: Release configuration must NEVER disable the CEF sandbox
        #[cfg(not(debug_assertions))]
        {
            if self.config.no_sandbox {
                return Err(EngineError::Lifecycle(
                    "Violation of Gate CEF-03b: Release configuration cannot disable CEF sandbox"
                        .to_string(),
                ));
            }
        }

        #[cfg(debug_assertions)]
        {
            if self.config.no_sandbox {
                warn!("Gate CEF-03b notice: CEF sandbox is disabled in DEBUG mode for local development");
            }
        }

        // Validate root_cache_path and cache_path relationship:
        // CEF documentation requires cache_path to be equal to or a child of root_cache_path
        if !self.config.cache_path.starts_with(&self.config.root_cache_path) {
            return Err(EngineError::Packaging(format!(
                "Invalid CEF settings: cache_path ({:?}) must be inside root_cache_path ({:?})",
                self.config.cache_path, self.config.root_cache_path
            )));
        }

        Ok(())
    }

    /// Ensure cache and profile directories exist on disk.
    pub fn ensure_cache_directories(&self) -> Result<(), EngineError> {
        std::fs::create_dir_all(&self.config.root_cache_path).map_err(|e| {
            EngineError::Packaging(format!(
                "Failed to create root_cache_path {:?}: {e}",
                self.config.root_cache_path
            ))
        })?;

        std::fs::create_dir_all(&self.config.cache_path).map_err(|e| {
            EngineError::Packaging(format!(
                "Failed to create cache_path {:?}: {e}",
                self.config.cache_path
            ))
        })?;

        Ok(())
    }

    /// Mark runtime as initialized.
    pub fn mark_initialized(&self) {
        self.initialized.store(true, Ordering::SeqCst);
    }

    /// Whether CEF is currently initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    /// Verify whether clean shutdown can proceed (CEF-10).
    ///
    /// CEF invariant: CefShutdown() MUST only be called after all browsers
    /// have reached OnBeforeClose (active_browser_count == 0).
    pub fn can_shutdown(&self) -> bool {
        self.is_initialized() && self.active_browser_count() == 0
    }

    /// Step 2B-1 (CEF-01B): Real CEF initialization.
    pub fn initialize_cef(&self) -> Result<(), EngineError> {
        self.validate_config()?;
        self.ensure_cache_directories()?;
        self.transition_to(CefEngineState::CefInitializing)?;

        // Configure CEF API version so internal CToCpp wrappers recognize the API version
        let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

        let settings = Settings {
            multi_threaded_message_loop: if self.config.multi_threaded_message_loop { 1 } else { 0 },
            no_sandbox: if self.config.no_sandbox { 1 } else { 0 },
            root_cache_path: self.config.root_cache_path.to_string_lossy().as_ref().into(),
            cache_path: self.config.cache_path.to_string_lossy().as_ref().into(),
            browser_subprocess_path: self.config.subprocess_path
                .as_deref()
                .map(|p| p.to_string_lossy().as_ref().into())
                .unwrap_or_default(),
            persist_session_cookies: if self.config.persist_session_cookies { 1 } else { 0 },
            ..Default::default()
        };

        let args = cef::args::Args::new();
        let result = cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            None,
            std::ptr::null_mut(),
        );

        if result != 1 {
            let _ = self.transition_to(CefEngineState::Failed);
            return Err(EngineError::Lifecycle(format!(
                "cef::initialize() returned {result} — CEF-01B FAILED"
            )));
        }

        self.initialized.store(true, Ordering::SeqCst);
        self.transition_to(CefEngineState::CefReady)?;
        self.transition_to(CefEngineState::CefContextReady)?;
        self.transition_to(CefEngineState::BrowserCreationAllowed)?;
        Ok(())
    }

    /// Explicitly authorize browser creation after CefContextReady is reached.
    pub fn allow_browser_creation(&self) -> Result<(), EngineError> {
        self.transition_to(CefEngineState::BrowserCreationAllowed)
    }

    /// Whether a page load has completed (on_load_end fired).
    pub fn is_page_loaded(&self) -> bool {
        self.page_loaded.load(Ordering::SeqCst)
    }

    /// Last observed HTTP status code from on_load_end.
    pub fn last_http_status(&self) -> i32 {
        self.last_http_status.load(Ordering::SeqCst)
    }

    /// Last observed main frame URL from on_load_end.
    pub fn last_loaded_url(&self) -> Option<String> {
        self.last_loaded_url.read().ok().and_then(|g| g.clone())
    }

    /// Close a specific browser instance.
    /// - `force = false`: Normal graceful close (CEF-10A) allowing DoClose and unload handlers.
    /// - `force = true`: Forced immediate termination (CEF-10B).
    pub fn request_close_browser(&self, index: usize, force: bool) -> Result<(), EngineError> {
        if let Ok(hosts) = self.browser_hosts.lock() {
            if let Some(host) = hosts.get(index) {
                host.close_browser(if force { 1 } else { 0 });
                return Ok(());
            }
        }
        Err(EngineError::Lifecycle(format!("No active browser host found at index {index}")))
    }

    /// Step 2B-3 (CEF-05 / CEF-06B): Create a browser in a native child HWND.
    pub fn create_browser(
        &self,
        parent_hwnd: isize,
        content_rect: &crate::composition::ViewportRect,
        url: &str,
    ) -> Result<(), EngineError> {
        self.create_browser_with_context(parent_hwnd, content_rect, url, None)
    }

    /// Create a browser in a native child HWND with an optional custom RequestContext (P3-E2E-05).
    pub fn create_browser_with_context(
        &self,
        parent_hwnd: isize,
        content_rect: &crate::composition::ViewportRect,
        url: &str,
        mut request_context: Option<RequestContext>,
    ) -> Result<(), EngineError> {
        let current = self.state();
        if current != CefEngineState::BrowserCreationAllowed && current != CefEngineState::Running {
            return Err(EngineError::Lifecycle(format!(
                "Cannot create browser: engine state is {:?}, must be BrowserCreationAllowed or Running",
                current
            )));
        }

        if current == CefEngineState::BrowserCreationAllowed {
            self.transition_to(CefEngineState::BrowserCreating)?;
        }

        let mut window_info = WindowInfo::default();
        let rect = Rect {
            x: content_rect.x,
            y: content_rect.y,
            width: content_rect.width,
            height: content_rect.height,
        };
        window_info = window_info.set_as_child(cef::sys::HWND(parent_hwnd as *mut _), &rect);

        let settings = BrowserSettings::default();
        let lifespan = KageLifeSpanHandler::new(
            self.active_browsers.clone(),
            self.browser_hosts.clone(),
            self.last_browser_id.clone(),
        );
        let loader = KageLoadHandler::new(
            self.is_loading.clone(),
            self.load_started.clone(),
            self.load_start_url.clone(),
            self.page_loaded.clone(),
            self.last_http_status.clone(),
            self.last_loaded_url.clone(),
            self.load_failed.clone(),
            self.last_error_code.clone(),
            self.last_error_text.clone(),
        );
        let req_handler = KageRequestHandler::new(
            self.renderer_terminated.clone(),
            self.termination_status.clone(),
            self.termination_error_code.clone(),
        );
        let mut client = KageBrowserClient::new(lifespan, loader, Some(req_handler));
        let url_str = CefString::from(url);

        let result = cef::browser_host_create_browser(
            Some(&window_info),
            Some(&mut client),
            Some(&url_str),
            Some(&settings),
            None,
            request_context.as_mut(),
        );

        if result != 1 {
            let _ = self.transition_to(CefEngineState::Failed);
            return Err(EngineError::Lifecycle(format!(
                "browser_host_create_browser returned {result} — CEF-05 FAILED"
            )));
        }

        if current == CefEngineState::BrowserCreationAllowed {
            self.transition_to(CefEngineState::BrowserReady)?;
            self.transition_to(CefEngineState::Running)?;
        }
        Ok(())
    }

    /// Step 2B-7 (CEF-10): Asynchronously wait for active browsers to close, then shut down CEF.
    ///
    /// Implements two-stage shutdown:
    /// - Stage 1 (CEF-10A): Requests graceful close (`force=0`) on all active hosts.
    /// - Stage 2 (CEF-10B): If browsers do not close within 3s grace window, escalates to force close (`force=1`).
    /// - Stage 3: If 10s deadline expires, logs critical error and fails closed with `EngineError::ShutdownTimeout`.
    pub async fn shutdown_async(&self) -> Result<(), EngineError> {
        self.transition_to(CefEngineState::CloseRequested)?;
        println!("[shutdown_async] State transitioned to CloseRequested. Active count: {}", self.active_browser_count());

        // Stage 1 (CEF-10A): Graceful close
        {
            if let Ok(hosts) = self.browser_hosts.lock() {
                println!("[shutdown_async] Requesting graceful close (force=0) on {} browser host(s)...", hosts.len());
                for (i, host) in hosts.iter().enumerate() {
                    println!("[shutdown_async] Requesting close_browser(0) on host #{}...", i);
                    host.close_browser(0);
                }
            }
        }

        let counter = self.active_browsers.clone();
        let grace_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        let hard_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut escalated_to_force = false;

        while counter.load(Ordering::SeqCst) > 0 {
            if !escalated_to_force && tokio::time::Instant::now() > grace_deadline {
                println!("[shutdown_async] Graceful close window exceeded 3s; escalating to force close (force=1, CEF-10B)...");
                if let Ok(hosts) = self.browser_hosts.lock() {
                    for (i, host) in hosts.iter().enumerate() {
                        println!("[shutdown_async] Requesting close_browser(1) on host #{}...", i);
                        host.close_browser(1);
                    }
                }
                escalated_to_force = true;
            }

            if tokio::time::Instant::now() > hard_deadline {
                let err_msg = format!(
                    "CRITICAL: CefShutdownTimeout — {} active browser(s) failed to reach OnBeforeClose within 10s deadline. Aborting graceful shutdown.",
                    counter.load(Ordering::SeqCst)
                );
                tracing::error!("{}", err_msg);
                eprintln!("{}", err_msg);
                let _ = self.transition_to(CefEngineState::Failed);
                return Err(EngineError::ShutdownTimeout(err_msg));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        println!("[shutdown_async] All active browsers closed (count = 0).");

        // Allow CEF message loop to settle post-browser destruction
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if self.state() == CefEngineState::CloseRequested {
            let _ = self.transition_to(CefEngineState::CefShutdownPending);
        }

        println!("[shutdown_async] Invoking cef::shutdown()...");
        cef::shutdown();
        println!("[shutdown_async] cef::shutdown() completed successfully.");

        self.initialized.store(false, Ordering::SeqCst);
        self.transition_to(CefEngineState::Shutdown)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_cache_path_hierarchy() {
        let mut config = RuntimeConfig::default();
        let runtime = CefRuntime::new(config.clone());
        assert!(runtime.validate_config().is_ok());

        // Invalidate cache path relationship
        config.cache_path = PathBuf::from("C:\\Different\\Path");
        let invalid_runtime = CefRuntime::new(config);
        assert!(invalid_runtime.validate_config().is_err());
    }

    #[test]
    fn test_active_browser_tracking() {
        let runtime = CefRuntime::new(RuntimeConfig::default());
        runtime.mark_initialized();
        let counter = runtime.active_browser_counter();

        assert_eq!(runtime.active_browser_count(), 0);
        assert!(runtime.can_shutdown());

        counter.fetch_add(1, Ordering::SeqCst);
        assert_eq!(runtime.active_browser_count(), 1);
        assert!(!runtime.can_shutdown()); // Blocked: browser is active

        counter.fetch_sub(1, Ordering::SeqCst);
        assert_eq!(runtime.active_browser_count(), 0);
        assert!(runtime.can_shutdown()); // Permitted: browser closed
    }

    #[test]
    fn test_engine_state_nominal_progression() {
        let runtime = CefRuntime::new(RuntimeConfig::default());
        assert_eq!(runtime.state(), CefEngineState::Created);

        // Nominal progression
        runtime.transition_to(CefEngineState::CefInitializing).unwrap();
        assert_eq!(runtime.state(), CefEngineState::CefInitializing);

        runtime.transition_to(CefEngineState::CefReady).unwrap();
        runtime.transition_to(CefEngineState::CefContextReady).unwrap();
        runtime.transition_to(CefEngineState::BrowserCreationAllowed).unwrap();
        runtime.transition_to(CefEngineState::BrowserCreating).unwrap();
        runtime.transition_to(CefEngineState::BrowserReady).unwrap();
        runtime.transition_to(CefEngineState::Running).unwrap();
        runtime.transition_to(CefEngineState::CloseRequested).unwrap();
        runtime.transition_to(CefEngineState::BrowserClosing).unwrap();
        runtime.transition_to(CefEngineState::BrowserClosed).unwrap();
        runtime.transition_to(CefEngineState::CefShutdownPending).unwrap();
        runtime.transition_to(CefEngineState::Shutdown).unwrap();
        assert_eq!(runtime.state(), CefEngineState::Shutdown);
    }

    #[test]
    fn test_engine_state_illegal_transition_rejected() {
        let runtime = CefRuntime::new(RuntimeConfig::default());

        // Cannot skip Created → BrowserCreating (must pass through CefInitializing, CefReady)
        let err = runtime.transition_to(CefEngineState::BrowserCreating);
        assert!(err.is_err(), "Illegal skip transition must be rejected");

        // Must stay Created still
        assert_eq!(runtime.state(), CefEngineState::Created);
    }

    #[test]
    fn test_engine_state_failed_from_any_state() {
        for start_state in [
            CefEngineState::Created,
            CefEngineState::CefInitializing,
            CefEngineState::CefReady,
            CefEngineState::BrowserCreating,
            CefEngineState::BrowserReady,
            CefEngineState::Running,
        ] {
            assert!(
                start_state.can_transition_to(CefEngineState::Failed),
                "Every active state should be able to transition to Failed"
            );
        }
    }

    #[test]
    fn test_engine_state_degraded_only_from_running() {
        assert!(CefEngineState::Running.can_transition_to(CefEngineState::Degraded));
        assert!(!CefEngineState::BrowserReady.can_transition_to(CefEngineState::Degraded));
        assert!(!CefEngineState::CefReady.can_transition_to(CefEngineState::Degraded));
    }
}
