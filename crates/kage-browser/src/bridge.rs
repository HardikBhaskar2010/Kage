//! CefTabBridge: Thread-safe, non-blocking bridge connecting native CEF lifecycle events
//! to the KAGE browser control plane (TabManager / NavigationController).
//!
//! # Architecture Invariant
//! CEF callbacks execute on native OS threads (or the CEF UI thread). They must NEVER
//! block or acquire Tokio async locks synchronously. Instead, all events are dispatched
//! onto an unbounded MPSC channel, where an asynchronous worker task routes them to
//! `TabManager` and `NavigationController` without stalling the CEF message loop.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use kage_engine::CefLifecycleObserver;

use crate::manager::TabManager;
use crate::tab::{
    CefTerminationStatus, RendererCrashDiagnostics, RendererTerminationStatus, TabId,
};

/// Internal message carrying native CEF lifecycle callbacks.
#[derive(Debug)]
pub enum CefBridgeEvent {
    AfterCreated {
        browser_id: i32,
    },
    BeforeClose {
        browser_id: i32,
    },
    LoadingStateChange {
        browser_id: i32,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    LoadStart {
        browser_id: i32,
        url: String,
        is_main: bool,
    },
    LoadEnd {
        browser_id: i32,
        url: String,
        http_status: i32,
        is_main: bool,
    },
    LoadError {
        browser_id: i32,
        error_code: i32,
        error_text: String,
        failed_url: String,
    },
    TitleChange {
        browser_id: i32,
        title: String,
    },
    RenderProcessTerminated {
        browser_id: i32,
        status: i32,
        error_code: i32,
    },
}

/// Thread-safe bridge implementing `CefLifecycleObserver` and dispatching
/// lifecycle transitions onto the `TabManager`.
pub struct CefTabBridge {
    sender: mpsc::UnboundedSender<CefBridgeEvent>,
    pending_tabs: Arc<Mutex<VecDeque<TabId>>>,
    unclaimed_browsers: Arc<Mutex<VecDeque<i32>>>,
}

impl CefTabBridge {
    /// Construct a new `CefTabBridge` and spawn its background processing loop on the Tokio runtime.
    pub fn new(tab_manager: Arc<TabManager>) -> Arc<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let pending_tabs = Arc::new(Mutex::new(VecDeque::new()));
        let unclaimed_browsers = Arc::new(Mutex::new(VecDeque::new()));

        let bridge = Arc::new(Self {
            sender,
            pending_tabs: pending_tabs.clone(),
            unclaimed_browsers: unclaimed_browsers.clone(),
        });

        tokio::spawn(Self::event_loop(
            receiver,
            tab_manager,
            pending_tabs,
            unclaimed_browsers,
        ));

        bridge
    }

    /// Register a `TabId` that is expecting a CEF browser instance to be created.
    ///
    /// If a browser was already created and unclaimed, it is immediately bound.
    pub fn register_pending_tab(&self, tab_id: TabId, tab_manager: &Arc<TabManager>) {
        if let Ok(mut unclaimed) = self.unclaimed_browsers.lock() {
            if let Some(browser_id) = unclaimed.pop_front() {
                let tm = tab_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = tm.bind_cef_browser(tab_id, browser_id).await {
                        error!(tab_id = %tab_id, browser_id, "Failed to bind unclaimed browser: {e}");
                    }
                });
                return;
            }
        }

        if let Ok(mut pending) = self.pending_tabs.lock() {
            pending.push_back(tab_id);
        }
    }

    /// Background task processing events from CEF callbacks.
    async fn event_loop(
        mut receiver: mpsc::UnboundedReceiver<CefBridgeEvent>,
        tab_manager: Arc<TabManager>,
        pending_tabs: Arc<Mutex<VecDeque<TabId>>>,
        unclaimed_browsers: Arc<Mutex<VecDeque<i32>>>,
    ) {
        while let Some(event) = receiver.recv().await {
            match event {
                CefBridgeEvent::AfterCreated { browser_id } => {
                    debug!(browser_id, "CEF browser created callback received");
                    let mut matched_tab = None;
                    if let Ok(mut pending) = pending_tabs.lock() {
                        matched_tab = pending.pop_front();
                    }

                    if let Some(tab_id) = matched_tab {
                        info!(tab_id = %tab_id, browser_id, "Binding newly created CEF browser to pending tab");
                        if let Err(e) = tab_manager.bind_cef_browser(tab_id, browser_id).await {
                            error!(tab_id = %tab_id, browser_id, "Failed to bind CEF browser to tab: {e}");
                        }
                    } else {
                        // Check if active tab exists and has no browser bound yet
                        if let Some(active_id) = tab_manager.get_active_tab().await {
                            if let Ok(tab) = tab_manager.get_tab(active_id).await {
                                if tab.identity.read().await.cef_browser_id.is_none() {
                                    info!(tab_id = %active_id, browser_id, "Binding created CEF browser to active unbound tab");
                                    let _ = tab_manager.bind_cef_browser(active_id, browser_id).await;
                                    continue;
                                }
                            }
                        }

                        // Otherwise stash as unclaimed
                        if let Ok(mut unclaimed) = unclaimed_browsers.lock() {
                            unclaimed.push_back(browser_id);
                        }
                    }
                }

                CefBridgeEvent::BeforeClose { browser_id } => {
                    debug!(browser_id, "CEF browser closing callback received");
                }

                CefBridgeEvent::LoadingStateChange {
                    browser_id,
                    is_loading: _,
                    can_go_back,
                    can_go_forward,
                } => {
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        if let Ok(tab) = tab_manager.get_tab(tab_id).await {
                            tab_manager
                                .navigation()
                                .handle_loading_state_change(&tab, can_go_back, can_go_forward)
                                .await;
                        }
                    }
                }

                CefBridgeEvent::LoadStart {
                    browser_id,
                    url,
                    is_main,
                } => {
                    if !is_main {
                        continue;
                    }
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        if let Ok(tab) = tab_manager.get_tab(tab_id).await {
                            tab_manager
                                .navigation()
                                .handle_load_start(&tab, None, &url, is_main)
                                .await;
                            let _ = tab_manager.handle_address_change(tab_id, &url).await;
                        }
                    }
                }

                CefBridgeEvent::LoadEnd {
                    browser_id,
                    url,
                    http_status,
                    is_main,
                } => {
                    if !is_main {
                        continue;
                    }
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        if let Ok(tab) = tab_manager.get_tab(tab_id).await {
                            tab_manager
                                .navigation()
                                .handle_load_end(&tab, None, &url, http_status, is_main)
                                .await;
                        }
                    }
                }

                CefBridgeEvent::LoadError {
                    browser_id,
                    error_code,
                    error_text,
                    failed_url,
                } => {
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        if let Ok(tab) = tab_manager.get_tab(tab_id).await {
                            tab_manager
                                .navigation()
                                .handle_load_error(&tab, None, &failed_url, error_code, &error_text, true)
                                .await;
                        }
                    }
                }

                CefBridgeEvent::TitleChange { browser_id, title } => {
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        let _ = tab_manager.handle_title_change(tab_id, &title).await;
                    }
                }

                CefBridgeEvent::RenderProcessTerminated {
                    browser_id,
                    status,
                    error_code: _,
                } => {
                    if let Some(tab_id) = tab_manager.tab_id_for_browser(browser_id).await {
                        let raw_cef = CefTerminationStatus::from_raw(status);
                        let term_status = RendererTerminationStatus::from(raw_cef);
                        let now_ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let diagnostics = RendererCrashDiagnostics {
                            termination_status: term_status,
                            raw_cef_status: raw_cef,
                            observed_at_ms: now_ms,
                        };
                        let _ = tab_manager.handle_renderer_crash(tab_id, diagnostics).await;
                    }
                }
            }
        }
    }
}

impl CefLifecycleObserver for CefTabBridge {
    fn on_after_created(&self, browser_id: i32) {
        let _ = self.sender.send(CefBridgeEvent::AfterCreated { browser_id });
    }

    fn on_before_close(&self, browser_id: i32) {
        let _ = self.sender.send(CefBridgeEvent::BeforeClose { browser_id });
    }

    fn on_loading_state_change(
        &self,
        browser_id: i32,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    ) {
        let _ = self.sender.send(CefBridgeEvent::LoadingStateChange {
            browser_id,
            is_loading,
            can_go_back,
            can_go_forward,
        });
    }

    fn on_load_start(&self, browser_id: i32, url: &str, is_main: bool) {
        let _ = self.sender.send(CefBridgeEvent::LoadStart {
            browser_id,
            url: url.to_string(),
            is_main,
        });
    }

    fn on_load_end(&self, browser_id: i32, url: &str, http_status: i32, is_main: bool) {
        let _ = self.sender.send(CefBridgeEvent::LoadEnd {
            browser_id,
            url: url.to_string(),
            http_status,
            is_main,
        });
    }

    fn on_load_error(
        &self,
        browser_id: i32,
        error_code: i32,
        error_text: &str,
        failed_url: &str,
    ) {
        let _ = self.sender.send(CefBridgeEvent::LoadError {
            browser_id,
            error_code,
            error_text: error_text.to_string(),
            failed_url: failed_url.to_string(),
        });
    }

    fn on_title_change(&self, browser_id: i32, title: &str) {
        let _ = self.sender.send(CefBridgeEvent::TitleChange {
            browser_id,
            title: title.to_string(),
        });
    }

    fn on_render_process_terminated(&self, browser_id: i32, status: i32, error_code: i32) {
        let _ = self.sender.send(CefBridgeEvent::RenderProcessTerminated {
            browser_id,
            status,
            error_code,
        });
    }
}
