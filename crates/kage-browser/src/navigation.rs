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
/// Calling `try_complete` atomically takes the sender out of the `Option` inside
/// the lock — if `take()` returns `Some`, this caller is the unique winner.
/// If `take()` returns `None`, the operation was already completed by a racing
/// caller (e.g. a renderer crash racing a navigation timeout).
///
/// This eliminates the split `AtomicBool + Mutex` pattern where:
///   CAS(false → true) succeeds → mutex poisoned → sender never delivered →
///   operation is terminal but its waiter hangs forever.
pub struct PendingOperation {
    pub id: BrowserOperationId,
    pub tab_id: TabId,
    pub nav_id: Option<NavigationId>,
    pub description: String,
    /// `Some(tx)` = pending; `None` = completed. Guarded by Mutex for exactly-once take.
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

    /// Exactly-once completion gate. Taking the sender IS the atomic completion.
    ///
    /// Returns `true` if this invocation was the unique winning resolver.
    /// Returns `false` if the operation was already completed by a racing caller.
    ///
    /// # Mutex poisoning
    ///
    /// If the mutex is poisoned the guard is **recovered** via `into_inner()` rather
    /// than discarding it. Returning `false` on a poisoned lock without inspecting
    /// the inner value would be wrong: the sender may still be `Some(tx)`, meaning
    /// the operation is NOT completed — dropping without sending would leave the
    /// waiter permanently pending.
    ///
    /// The recovered guard is used exactly the same way as an un-poisoned guard.
    /// The sender token — not the mutex health — is the terminal ownership token.
    pub fn try_complete(&self, result: Result<(), BrowserError>) -> bool {
        let mut guard = match self.sender.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.take() {
            Some(tx) => {
                let _ = tx.send(result);
                true
            }
            None => false, // Already completed by a racing caller.
        }
    }

    /// Returns `true` if the operation has reached a terminal completion state.
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
    #[doc(hidden)]
    pub fn is_poisoned_for_test(&self) -> bool {
        self.sender.is_poisoned()
    }
}

/// Coordinates navigation actions and callback processing for tabs.
pub struct NavigationController {
    event_bus: BrowserEventBus,
    next_nav_id: AtomicU64,
    next_op_id: AtomicU64,
    pending_operations: RwLock<HashMap<BrowserOperationId, Arc<PendingOperation>>>,
}

impl NavigationController {
    pub fn new(event_bus: BrowserEventBus) -> Self {
        Self {
            event_bus,
            next_nav_id: AtomicU64::new(1),
            next_op_id: AtomicU64::new(1),
            pending_operations: RwLock::new(HashMap::new()),
        }
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
            .unwrap_or_else(|| self.next_navigation_id());

        // Validate generation match if tab was already loading
        if let Some(active_id) = current_state.navigation_id() {
            if active_id != target_nav_id {
                warn!(active_id = %active_id, target_id = %target_nav_id, "ignoring stale on_load_start callback");
                return;
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
            .unwrap_or_else(|| self.next_navigation_id());

        if let Some(active_id) = current_state.navigation_id() {
            if active_id != target_nav_id {
                warn!(active_id = %active_id, target_id = %target_nav_id, "ignoring stale on_load_end callback");
                return;
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
