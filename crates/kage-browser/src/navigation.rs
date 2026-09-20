//! Navigation controller coordinating URL loading, history traversal, and operation synchronization.
//!
//! Enforces:
//! - Navigation state is event-driven from real CEF engine callbacks.
//! - Callbacks from older generations (`NavigationId`) are discarded (overlap race prevention).
//! - CEF request tracking via `cef_request_id` (correlation across callbacks).
//! - Subframe loads are strictly filtered (`if !is_main { return; }`).
//! - CEF load callback sequence:
//!   `OnLoadingStateChange(true)` -> `OnLoadStart(main frame)` -> `OnLoadEnd` / `OnLoadError` -> `OnLoadingStateChange(false)`.
//! - Pending operations enforce exactly-once completion (`INV-11A`) via a single `Mutex<Option<Sender>>`.
//!   `None` encodes "completed"; `Some(tx)` encodes "pending". Taking the sender IS the atomic completion
//!   gate — both the terminal-state flip and sender-ownership transfer happen under the same lock
//!   acquisition, preventing the poisoned-mutex completion-loss bug that a split `AtomicBool + Mutex`
//!   would allow (CAS succeeds → lock fails → waiter hung forever).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, RwLock};
use tracing::{info, warn};

use crate::errors::BrowserError;
use crate::events::{BrowserEventBus, BrowserEventKind, BrowserEventProducer};
use crate::tab::{
    NavigationCancelCause, NavigationId, NavigationRecord, NavigationSource, NavigationState, Tab,
    TabId,
};

/// Unique identifier for an asynchronous inflight browser operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BrowserOperationId(pub u64);

impl std::fmt::Display for BrowserOperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "op#{}", self.0)
    }
}

/// Inflight operation handle enforcing exactly-once completion (`INV-11A`).
///
/// # Completion correctness
///
/// The terminal state and sender ownership are unified under a **single `Mutex`**.
/// `sender == Some(_)` means pending; `sender == None` means completed.
/// Taking the sender via `guard.take()` is the atomic completion gate:
///
/// ```text
/// Mutex acquisition (recovering poisoned guard if needed)
///         ↓
/// guard.take()
///   ├─ Some(tx) → WINNER: deliver result, return true
///   └─ None     → LOSER: already completed by racing caller, return false
/// ```
///
/// This eliminates the race where a split `AtomicBool + Mutex` could mark completed
/// but fail to deliver the result (e.g. if the mutex lock were to panic).
pub struct PendingOperation {
    pub id: BrowserOperationId,
    pub tab_id: TabId,
    pub nav_id: Option<NavigationId>,
    pub description: String,
    sender: Mutex<Option<oneshot::Sender<Result<(), BrowserError>>>>,
}

impl PendingOperation {
    pub fn new(
        id: BrowserOperationId,
        tab_id: TabId,
        nav_id: Option<NavigationId>,
        description: String,
        sender: oneshot::Sender<Result<(), BrowserError>>,
    ) -> Self {
        Self {
            id,
            tab_id,
            nav_id,
            description,
            sender: Mutex::new(Some(sender)),
        }
    }

    /// Complete the operation exactly once.
    ///
    /// Returns `true` if this call won the completion race and delivered the result;
    /// `false` if the operation was already completed by a racing caller.
    ///
    /// If the internal mutex was poisoned by a prior thread panic, this method
    /// recovers the guard via `into_inner()` rather than propagating the panic or
    /// dropping the sender without notifying the waiter.
    pub fn try_complete(&self, result: Result<(), BrowserError>) -> bool {
        let mut guard = match self.sender.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Mutex is poisoned: recover the guard. The Option<Sender> inside
                // is still valid and must be drained to uphold INV-11A (fail-closed,
                // exactly-once notification, never hang the caller).
                poisoned.into_inner()
            }
        };
        match guard.take() {
            Some(tx) => {
                let _ = tx.send(result);
                true
            }
            None => false,
        }
    }

    /// Check if the operation is completed without consuming the sender.
    ///
    /// Recovers a poisoned mutex guard rather than assuming terminal state.
    pub fn is_completed(&self) -> bool {
        let guard = match self.sender.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.is_none()
    }

    /// Deliberately poisons the internal `sender` mutex for regression and concurrency testing.
    ///
    /// Spawns a thread that locks the internal `sender` mutex and panics while holding it.
    /// Used by regression tests to verify that `try_complete` and `is_completed` properly
    /// recover via `into_inner()` on the actual `PendingOperation` instance itself.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn poison_for_test(&self) {
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = match self.sender.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                panic!("deliberate panic while holding PendingOperation.sender mutex for testing");
            })
            .join()
        });
    }

    /// Returns `true` if the internal `sender` mutex is poisoned.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn is_poisoned_for_test(&self) -> bool {
        self.sender.is_poisoned()
    }
}

/// Active navigation correlation record.
///
/// Implements the host correlation table bridging:
/// `BrowserId + main-frame identity + active navigation generation (NavigationId) + navigation request metadata (cef_request_id, redirects) ↓ CEF load callbacks (OnLoadStart, OnLoadEnd)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationCorrelation {
    pub nav_id: NavigationId,
    pub tab_id: TabId,
    pub cef_browser_id: Option<i32>,
    pub cef_request_id: Option<u64>,
    pub requested_url: String,
    pub redirect_chain: Vec<String>,
    pub is_main_frame: bool,
    pub started_at_ms: u64,
    pub committed_url: Option<String>,
    pub committed_at_ms: Option<u64>,
    pub completed_url: Option<String>,
    pub completed_at_ms: Option<u64>,
    pub http_status: Option<i32>,
    pub source: NavigationSource,
    pub transition_type: Option<u32>,
    pub is_redirect: bool,
    pub user_gesture: Option<bool>,
}

/// Coordinates navigation actions and callback processing for tabs.
pub struct NavigationController {
    event_bus: BrowserEventBus,
    next_nav_id: AtomicU64,
    next_op_id: AtomicU64,
    pending_operations: RwLock<HashMap<BrowserOperationId, Arc<PendingOperation>>>,
    correlations: RwLock<HashMap<TabId, NavigationCorrelation>>,
    browser_to_tab: RwLock<HashMap<i32, TabId>>,
}

impl NavigationController {
    pub fn new(event_bus: BrowserEventBus) -> Self {
        Self {
            event_bus,
            next_nav_id: AtomicU64::new(1),
            next_op_id: AtomicU64::new(1),
            pending_operations: RwLock::new(HashMap::new()),
            correlations: RwLock::new(HashMap::new()),
            browser_to_tab: RwLock::new(HashMap::new()),
        }
    }

    /// Associate a CEF browser ID with a TabId for callback lookup.
    pub async fn bind_browser_id(&self, tab_id: TabId, cef_browser_id: i32) {
        self.browser_to_tab.write().await.insert(cef_browser_id, tab_id);
        if let Some(corr) = self.correlations.write().await.get_mut(&tab_id) {
            corr.cef_browser_id = Some(cef_browser_id);
        }
    }

    /// Record a CEF request identifier discovered via GetResourceRequestHandler.
    pub async fn record_cef_request(&self, tab_id: TabId, cef_request_id: u64, url: &str) {
        if let Some(corr) = self.correlations.write().await.get_mut(&tab_id) {
            corr.cef_request_id = Some(cef_request_id);
            if corr.requested_url != url && !corr.redirect_chain.contains(&url.to_string()) {
                corr.redirect_chain.push(url.to_string());
            }
        }
    }

    /// Record a CEF request identifier by CEF browser ID.
    pub async fn record_cef_request_by_browser(
        &self,
        cef_browser_id: i32,
        cef_request_id: u64,
        url: &str,
    ) -> Option<NavigationId> {
        let tab_id = self.browser_to_tab.read().await.get(&cef_browser_id).copied()?;
        let mut corrs = self.correlations.write().await;
        let corr = corrs.get_mut(&tab_id)?;
        corr.cef_request_id = Some(cef_request_id);
        if corr.requested_url != url && !corr.redirect_chain.contains(&url.to_string()) {
            corr.redirect_chain.push(url.to_string());
        }
        Some(corr.nav_id)
    }

    /// Record an HTTP redirect URL in the active correlation record.
    pub async fn record_redirect(&self, tab_id: TabId, new_url: &str) {
        if let Some(corr) = self.correlations.write().await.get_mut(&tab_id) {
            corr.is_redirect = true;
            corr.redirect_chain.push(new_url.to_string());
        }
    }

    /// Record an HTTP redirect URL by CEF browser ID.
    pub async fn record_redirect_by_browser(
        &self,
        cef_browser_id: i32,
        new_url: &str,
    ) -> Option<NavigationId> {
        let tab_id = self.browser_to_tab.read().await.get(&cef_browser_id).copied()?;
        let mut corrs = self.correlations.write().await;
        let corr = corrs.get_mut(&tab_id)?;
        corr.is_redirect = true;
        corr.redirect_chain.push(new_url.to_string());
        Some(corr.nav_id)
    }

    /// Record auxiliary navigation transition metadata (transition type, user gesture).
    pub async fn record_navigation_metadata(
        &self,
        tab_id: TabId,
        transition_type: Option<u32>,
        user_gesture: Option<bool>,
    ) {
        if let Some(corr) = self.correlations.write().await.get_mut(&tab_id) {
            if transition_type.is_some() {
                corr.transition_type = transition_type;
            }
            if user_gesture.is_some() {
                corr.user_gesture = user_gesture;
            }
        }
    }

    /// Retrieve the active navigation correlation record for a tab.
    pub async fn get_correlation(&self, tab_id: TabId) -> Option<NavigationCorrelation> {
        self.correlations.read().await.get(&tab_id).cloned()
    }

    /// Retrieve the active navigation correlation record by CEF browser ID.
    pub async fn get_correlation_by_browser(&self, cef_browser_id: i32) -> Option<NavigationCorrelation> {
        let tab_id = self.browser_to_tab.read().await.get(&cef_browser_id).copied()?;
        self.correlations.read().await.get(&tab_id).cloned()
    }

    /// Allocate a new monotonic `NavigationId`.
    pub fn next_navigation_id(&self) -> NavigationId {
        NavigationId(self.next_nav_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Allocate a new monotonic `BrowserOperationId`.
    pub fn next_operation_id(&self) -> BrowserOperationId {
        BrowserOperationId(self.next_op_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Register an inflight operation in the registry.
    pub async fn register_operation(
        &self,
        tab_id: TabId,
        nav_id: Option<NavigationId>,
        description: impl Into<String>,
        sender: oneshot::Sender<Result<(), BrowserError>>,
    ) -> BrowserOperationId {
        let op_id = self.next_operation_id();
        let op = Arc::new(PendingOperation::new(
            op_id,
            tab_id,
            nav_id,
            description.into(),
            sender,
        ));
        let mut ops = self.pending_operations.write().await;
        ops.insert(op_id, op);
        op_id
    }

    /// Complete an operation successfully by ID via atomic CAS.
    pub async fn complete_operation(&self, op_id: BrowserOperationId) -> bool {
        let op = {
            let mut ops = self.pending_operations.write().await;
            ops.remove(&op_id)
        };
        if let Some(op) = op {
            op.try_complete(Ok(()))
        } else {
            false
        }
    }

    /// Check if an operation is currently registered in the pending operations registry.
    pub async fn has_pending_operation(&self, op_id: BrowserOperationId) -> bool {
        let ops = self.pending_operations.read().await;
        ops.contains_key(&op_id)
    }

    /// Retrieve a reference to a registered pending operation.
    pub async fn get_pending_operation(&self, op_id: BrowserOperationId) -> Option<Arc<PendingOperation>> {
        let ops = self.pending_operations.read().await;
        ops.get(&op_id).cloned()
    }

    /// Complete operations associated with a specific `NavigationId` via atomic CAS.
    pub async fn complete_navigation_operations(
        &self,
        nav_id: NavigationId,
        result: Result<(), BrowserError>,
    ) {
        let matching_ops: Vec<Arc<PendingOperation>> = {
            let mut ops = self.pending_operations.write().await;
            let ids: Vec<BrowserOperationId> = ops
                .iter()
                .filter(|(_, op)| op.nav_id == Some(nav_id))
                .map(|(id, _)| *id)
                .collect();

            ids.into_iter().filter_map(|id| ops.remove(&id)).collect()
        };

        for op in matching_ops {
            let res = match &result {
                Ok(()) => Ok(()),
                Err(e) => Err(BrowserError::NavigationFailed(format!("{}", e))),
            };
            op.try_complete(res);
        }
    }

    /// Fail closed all pending operations for a tab via atomic CAS (INV-11A).
    pub async fn drain_operations_for_tab(&self, tab_id: TabId, reason: &str) {
        let matching_ops: Vec<Arc<PendingOperation>> = {
            let mut ops = self.pending_operations.write().await;
            let ids: Vec<BrowserOperationId> = ops
                .iter()
                .filter(|(_, op)| op.tab_id == tab_id)
                .map(|(id, _)| *id)
                .collect();

            ids.into_iter().filter_map(|id| ops.remove(&id)).collect()
        };

        for op in matching_ops {
            op.try_complete(Err(BrowserError::RendererCrashed(
                tab_id,
                reason.to_string(),
            )));
        }
    }

    /// Initiate navigation to a target URL.
    pub async fn navigate(
        &self,
        tab: &Arc<Tab>,
        url: &str,
        source: NavigationSource,
    ) -> Result<NavigationId, BrowserError> {
        self.navigate_with_request_id(tab, url, source, None).await
    }

    /// Initiate navigation with an optional CEF request identifier correlation.
    pub async fn navigate_with_request_id(
        &self,
        tab: &Arc<Tab>,
        url: &str,
        source: NavigationSource,
        cef_request_id: Option<u64>,
    ) -> Result<NavigationId, BrowserError> {
        let health = tab.health.read().await.clone();
        if health.is_crashed() {
            return Err(BrowserError::RendererCrashed(
                tab.id,
                "cannot navigate a tab with a terminated renderer".to_string(),
            ));
        }

        // Cancel any currently loading navigation as superseded
        {
            let current_state = tab.navigation.read().await.clone();
            if let Some(old_id) = current_state.navigation_id() {
                if current_state.is_loading() {
                    info!(tab_id = %tab.id, old_nav = %old_id, "superseding inflight navigation");
                    self.cancel_navigation(
                        tab,
                        old_id,
                        NavigationCancelCause::Superseded,
                    )
                    .await?;
                }
            }
        }

        let nav_id = self.next_navigation_id();
        info!(
            tab_id = %tab.id,
            nav_id = %nav_id,
            url = %url,
            source = ?source,
            cef_request_id = ?cef_request_id,
            "initiating navigation"
        );

        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut record = NavigationRecord::new(nav_id, source, url.to_string(), started_at_ms);
        record.cef_request_id = cef_request_id;
        *tab.active_navigation_record.write().await = Some(record);

        let new_state = NavigationState::Loading {
            id: nav_id,
            requested_url: url.to_string(),
            effective_url: None,
            source,
            cef_request_id,
            started_at_ms,
        };

        *tab.navigation.write().await = new_state.clone();
        *tab.url.write().await = url.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        if let Some(bid) = cef_id {
            self.browser_to_tab.write().await.insert(bid, tab.id);
        }

        let correlation = NavigationCorrelation {
            nav_id,
            tab_id: tab.id,
            cef_browser_id: cef_id,
            cef_request_id,
            requested_url: url.to_string(),
            redirect_chain: Vec::new(),
            is_main_frame: true,
            started_at_ms,
            committed_url: None,
            committed_at_ms: None,
            completed_url: None,
            completed_at_ms: None,
            http_status: None,
            source,
            transition_type: None,
            is_redirect: false,
            user_gesture: None,
        };
        self.correlations.write().await.insert(tab.id, correlation);

        self.event_bus.emit(
            BrowserEventProducer::NavigationController,
            Some(tab.id),
            cef_id,
            cdp_id.clone(),
            BrowserEventKind::NavigationStarted {
                id: nav_id,
                requested_url: url.to_string(),
                source,
            },
        );

        self.event_bus.emit(
            BrowserEventProducer::NavigationController,
            Some(tab.id),
            cef_id,
            cdp_id,
            BrowserEventKind::NavigationStateChanged { state: new_state },
        );

        Ok(nav_id)
    }

    /// Traverse backward in session history.
    pub async fn go_back(&self, tab: &Arc<Tab>) -> Result<Option<NavigationId>, BrowserError> {
        if !tab.can_go_back.load(Ordering::Relaxed) {
            warn!(tab_id = %tab.id, "go_back called but cannot go back");
            return Ok(None);
        }
        info!(tab_id = %tab.id, "initiating go_back");
        let url = tab.url.read().await.clone();
        let nav_id = self.navigate(tab, &url, NavigationSource::HistoryBack).await?;
        Ok(Some(nav_id))
    }

    /// Traverse forward in session history.
    pub async fn go_forward(&self, tab: &Arc<Tab>) -> Result<Option<NavigationId>, BrowserError> {
        if !tab.can_go_forward.load(Ordering::Relaxed) {
            warn!(tab_id = %tab.id, "go_forward called but cannot go forward");
            return Ok(None);
        }
        info!(tab_id = %tab.id, "initiating go_forward");
        let url = tab.url.read().await.clone();
        let nav_id = self.navigate(tab, &url, NavigationSource::HistoryForward).await?;
        Ok(Some(nav_id))
    }

    /// Reload active tab.
    pub async fn reload(&self, tab: &Arc<Tab>, _ignore_cache: bool) -> Result<NavigationId, BrowserError> {
        info!(tab_id = %tab.id, "initiating reload");
        let url = tab.url.read().await.clone();
        self.navigate(tab, &url, NavigationSource::Reload).await
    }

    /// Stop ongoing navigation.
    pub async fn stop(&self, tab: &Arc<Tab>) -> Result<(), BrowserError> {
        let current_state = tab.navigation.read().await.clone();
        if let Some(nav_id) = current_state.navigation_id() {
            if current_state.is_loading() {
                info!(tab_id = %tab.id, nav_id = %nav_id, "user stopping active navigation");
                return self
                    .cancel_navigation(tab, nav_id, NavigationCancelCause::UserStop)
                    .await;
            }
        }
        Ok(())
    }

    /// Cancel a specific navigation attempt.
    pub async fn cancel_navigation(
        &self,
        tab: &Arc<Tab>,
        nav_id: NavigationId,
        cause: NavigationCancelCause,
    ) -> Result<(), BrowserError> {
        let url = tab.url.read().await.clone();
        let new_state = NavigationState::Cancelled {
            id: nav_id,
            url: url.clone(),
            cause,
        };
        *tab.navigation.write().await = new_state.clone();

        if let Some(corr) = self.correlations.write().await.get_mut(&tab.id) {
            if corr.nav_id == nav_id {
                corr.http_status = Some(-1);
            }
        }

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::NavigationController,
            Some(tab.id),
            cef_id,
            cdp_id.clone(),
            BrowserEventKind::NavigationCancelled {
                id: nav_id,
                url,
                cause,
            },
        );

        self.event_bus.emit(
            BrowserEventProducer::NavigationController,
            Some(tab.id),
            cef_id,
            cdp_id,
            BrowserEventKind::NavigationStateChanged { state: new_state },
        );

        self.complete_navigation_operations(
            nav_id,
            Err(BrowserError::ActionCancelled(format!("{:?}", cause))),
        )
        .await;

        Ok(())
    }

    // ── CEF Callback Handlers ──────────────────────────────────────────

    /// Handle CEF `on_loading_state_change`.
    /// Note: `is_loading = false` resets loading flags, but does NOT transition state to Completed.
    pub async fn handle_loading_state_change(
        &self,
        tab: &Arc<Tab>,
        can_go_back: bool,
        can_go_forward: bool,
    ) {
        tab.can_go_back.store(can_go_back, Ordering::SeqCst);
        tab.can_go_forward.store(can_go_forward, Ordering::SeqCst);
    }

    /// Handle CEF `on_load_start` callback (Main frame committed).
    /// Mandatory check: `is_main` must be true.
    pub async fn handle_load_start(
        &self,
        tab: &Arc<Tab>,
        nav_id: Option<NavigationId>,
        url: &str,
        is_main: bool,
    ) {
        if !is_main {
            return;
        }

        let current_state = tab.navigation.read().await.clone();
        let target_nav_id = nav_id
            .or_else(|| current_state.navigation_id())
            .or_else(|| {
                self.correlations.try_read().ok()?.get(&tab.id).map(|c| c.nav_id)
            })
            .unwrap_or_else(|| self.next_navigation_id());

        // Validate generation match if tab was already loading
        if let Some(active_id) = current_state.navigation_id() {
            if active_id != target_nav_id {
                warn!(active_id = %active_id, target_id = %target_nav_id, "ignoring stale on_load_start callback");
                return;
            }
        }

        // Validate URL continuity against authoritative active generation and its redirect chain.
        // A late callback from a superseded navigation A cannot mutate B's state (P3-E2E-03).
        if let Some(corr) = self.correlations.read().await.get(&tab.id) {
            if corr.nav_id == target_nav_id {
                let matches_requested = corr.requested_url == url;
                let matches_redirect = corr.redirect_chain.iter().any(|r| r == url);
                let matches_tab_url = tab.url.read().await.as_str() == url;
                if !matches_requested && !matches_redirect && !matches_tab_url {
                    warn!(
                        tab_id = %tab.id,
                        target_nav_id = %target_nav_id,
                        callback_url = %url,
                        expected_url = %corr.requested_url,
                        "ignoring late on_load_start callback: URL does not match active generation"
                    );
                    return;
                }
            }
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Some(record) = tab.active_navigation_record.write().await.as_mut() {
            if record.navigation_id == target_nav_id {
                record.committed_url = Some(url.to_string());
                record.committed_at_ms = Some(now_ms);
            }
        }

        if let Some(corr) = self.correlations.write().await.get_mut(&tab.id) {
            if corr.nav_id == target_nav_id {
                corr.committed_url = Some(url.to_string());
                corr.committed_at_ms = Some(now_ms);
            }
        }

        let new_state = NavigationState::Committed {
            id: target_nav_id,
            url: url.to_string(),
            source: NavigationSource::Programmatic,
            cef_request_id: None,
        };

        *tab.navigation.write().await = new_state.clone();
        *tab.url.write().await = url.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab.id),
            cef_id,
            cdp_id.clone(),
            BrowserEventKind::NavigationCommitted {
                id: target_nav_id,
                url: url.to_string(),
                source: NavigationSource::Programmatic,
            },
        );

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab.id),
            cef_id,
            cdp_id,
            BrowserEventKind::NavigationStateChanged { state: new_state },
        );
    }

    /// Handle CEF `on_load_end` callback (Main frame loaded).
    /// Mandatory check: `is_main` must be true.
    pub async fn handle_load_end(
        &self,
        tab: &Arc<Tab>,
        nav_id: Option<NavigationId>,
        url: &str,
        http_status: i32,
        is_main: bool,
    ) {
        if !is_main {
            return;
        }

        let current_state = tab.navigation.read().await.clone();
        let target_nav_id = nav_id
            .or_else(|| current_state.navigation_id())
            .or_else(|| {
                self.correlations.try_read().ok()?.get(&tab.id).map(|c| c.nav_id)
            })
            .unwrap_or_else(|| self.next_navigation_id());

        if let Some(active_id) = current_state.navigation_id() {
            if active_id != target_nav_id {
                warn!(active_id = %active_id, target_id = %target_nav_id, "ignoring stale on_load_end callback");
                return;
            }
        }

        // Validate URL continuity against authoritative active generation and its redirect chain.
        // A late callback from a superseded navigation A cannot mutate B's state (P3-E2E-03).
        if let Some(corr) = self.correlations.read().await.get(&tab.id) {
            if corr.nav_id == target_nav_id {
                let matches_requested = corr.requested_url == url;
                let matches_redirect = corr.redirect_chain.iter().any(|r| r == url);
                let matches_tab_url = tab.url.read().await.as_str() == url;
                if !matches_requested && !matches_redirect && !matches_tab_url {
                    warn!(
                        tab_id = %tab.id,
                        target_nav_id = %target_nav_id,
                        callback_url = %url,
                        expected_url = %corr.requested_url,
                        "ignoring late on_load_end callback: URL does not match active generation"
                    );
                    return;
                }
            }
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Some(record) = tab.active_navigation_record.write().await.as_mut() {
            if record.navigation_id == target_nav_id {
                record.final_url = Some(url.to_string());
                record.finished_at_ms = Some(now_ms);
            }
        }

        if let Some(corr) = self.correlations.write().await.get_mut(&tab.id) {
            if corr.nav_id == target_nav_id {
                corr.completed_url = Some(url.to_string());
                corr.completed_at_ms = Some(now_ms);
                corr.http_status = Some(http_status);
            }
        }

        let new_state = NavigationState::Completed {
            id: target_nav_id,
            url: url.to_string(),
            http_status,
        };

        *tab.navigation.write().await = new_state.clone();
        *tab.url.write().await = url.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab.id),
            cef_id,
            cdp_id.clone(),
            BrowserEventKind::NavigationCompleted {
                id: target_nav_id,
                url: url.to_string(),
                http_status,
            },
        );

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab.id),
            cef_id,
            cdp_id,
            BrowserEventKind::NavigationStateChanged { state: new_state },
        );

        self.complete_navigation_operations(target_nav_id, Ok(())).await;
    }

    /// Handle CEF `on_load_error` callback.
    /// Mandatory check: `is_main` must be true.
    pub async fn handle_load_error(
        &self,
        tab: &Arc<Tab>,
        nav_id: Option<NavigationId>,
        url: &str,
        error_code: i32,
        reason: &str,
        is_main: bool,
    ) {
        if !is_main {
            return;
        }

        let current_state = tab.navigation.read().await.clone();
        let target_nav_id = nav_id
            .or_else(|| current_state.navigation_id())
            .unwrap_or_else(|| self.next_navigation_id());

        if let Some(active_id) = current_state.navigation_id() {
            if active_id != target_nav_id {
                warn!(active_id = %active_id, target_id = %target_nav_id, "ignoring stale on_load_error callback");
                return;
            }
        }

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        // Chromium ERR_ABORTED is -3: represents intentional cancellation
        if error_code == -3 {
            let new_state = NavigationState::Cancelled {
                id: target_nav_id,
                url: url.to_string(),
                cause: NavigationCancelCause::CefAborted,
            };
            *tab.navigation.write().await = new_state.clone();

            self.event_bus.emit(
                BrowserEventProducer::Cef,
                Some(tab.id),
                cef_id,
                cdp_id.clone(),
                BrowserEventKind::NavigationCancelled {
                    id: target_nav_id,
                    url: url.to_string(),
                    cause: NavigationCancelCause::CefAborted,
                },
            );

            self.event_bus.emit(
                BrowserEventProducer::Cef,
                Some(tab.id),
                cef_id,
                cdp_id,
                BrowserEventKind::NavigationStateChanged { state: new_state },
            );

            self.complete_navigation_operations(
                target_nav_id,
                Err(BrowserError::ActionCancelled("ERR_ABORTED".to_string())),
            )
            .await;
        } else {
            let new_state = NavigationState::Failed {
                id: target_nav_id,
                url: url.to_string(),
                error_code,
                reason: reason.to_string(),
            };
            *tab.navigation.write().await = new_state.clone();

            self.event_bus.emit(
                BrowserEventProducer::Cef,
                Some(tab.id),
                cef_id,
                cdp_id.clone(),
                BrowserEventKind::NavigationFailed {
                    id: target_nav_id,
                    url: url.to_string(),
                    error_code,
                    reason: reason.to_string(),
                },
            );

            self.event_bus.emit(
                BrowserEventProducer::Cef,
                Some(tab.id),
                cef_id,
                cdp_id,
                BrowserEventKind::NavigationStateChanged { state: new_state },
            );

            self.complete_navigation_operations(
                target_nav_id,
                Err(BrowserError::NavigationFailed(format!("Error {}: {}", error_code, reason))),
            )
            .await;
        }
    }

    /// Handle in-page / same-document navigation (e.g. fragment navigation #anchor or pushState).
    pub async fn handle_same_document_nav(&self, tab: &Arc<Tab>, url: &str) {
        *tab.url.write().await = url.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab.id),
            cef_id,
            cdp_id,
            BrowserEventKind::SameDocumentNavigated {
                url: url.to_string(),
            },
        );
    }
}
