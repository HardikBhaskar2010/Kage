//! TabManager: Central coordinator for tab lifecycle, active focus, and surface management.
//!
//! Enforces:
//! - Separation of browser identity (CefBrowserId) from display surface (surface_id / HWND).
//! - Deterministic crash fail-closed semantics (INV-11A).
//! - Deadlock-hardened concurrency (locks strictly scoped, never held across async points).
//! - Authoritative state snapshot generation for EventGap resynchronization.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::errors::BrowserError;
use crate::events::{
    BrowserEventBus, BrowserEventKind, BrowserEventProducer, TabStateSnapshot,
};
use crate::navigation::NavigationController;
use crate::profile::ProfileManager;
use crate::tab::{
    BrowserSurfaceId, CdpBinding, ProfileId, RendererCrashDiagnostics, Tab, TabHealth, TabId,
    TabLifecycle, TabSummary,
};

/// Central browser controller managing active and background tabs.
pub struct TabManager {
    tabs: RwLock<HashMap<TabId, Arc<Tab>>>,
    active_tab: RwLock<Option<TabId>>,
    profile_manager: Arc<ProfileManager>,
    navigation: Arc<NavigationController>,
    event_bus: BrowserEventBus,
}

impl TabManager {
    pub fn new(
        profile_manager: Arc<ProfileManager>,
        event_bus: BrowserEventBus,
    ) -> Self {
        let navigation = Arc::new(NavigationController::new(event_bus.clone()));
        Self {
            tabs: RwLock::new(HashMap::new()),
            active_tab: RwLock::new(None),
            profile_manager,
            navigation,
            event_bus,
        }
    }

    /// Access the navigation controller.
    pub fn navigation(&self) -> &Arc<NavigationController> {
        &self.navigation
    }

    /// Access the event bus.
    pub fn event_bus(&self) -> &BrowserEventBus {
        &self.event_bus
    }

    /// Access the profile manager.
    pub fn profile_manager(&self) -> &Arc<ProfileManager> {
        &self.profile_manager
    }

    /// Generate an authoritative snapshot of the entire control plane state (for gap resync).
    pub async fn generate_state_snapshot(&self) -> TabStateSnapshot {
        let tabs = self.list_tabs().await;
        let active_tab = self.get_active_tab().await;
        let sequence = self.event_bus.current_sequence();
        let latest_global_sequence = sequence;
        let latest_critical_sequence = self.event_bus.current_critical_sequence();
        let state_revision = sequence;
        let generated_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        TabStateSnapshot {
            sequence,
            latest_global_sequence,
            latest_critical_sequence,
            state_revision,
            tabs,
            active_tab,
            generated_at_ms,
        }
    }

    /// Emit an authoritative state snapshot event (e.g. after EventGap recovery).
    pub async fn emit_state_snapshot(&self) -> TabStateSnapshot {
        let snapshot = self.generate_state_snapshot().await;
        self.event_bus.emit(
            BrowserEventProducer::BrowserControl,
            None,
            None,
            None,
            BrowserEventKind::StateSnapshot {
                snapshot: snapshot.clone(),
            },
        );
        snapshot
    }

    /// Create a new managed browser tab.
    pub async fn create_tab(
        &self,
        profile_id: ProfileId,
        initial_url: impl Into<String>,
    ) -> Result<TabId, BrowserError> {
        let url_str = initial_url.into();
        let tab_id = TabId::new();

        // Ensure target profile exists and has directory structure
        self.profile_manager.get_or_create(&profile_id).await?;

        let tab = Arc::new(Tab::new(tab_id, profile_id.clone(), url_str.clone()));
        {
            let mut tabs = self.tabs.write().await;
            tabs.insert(tab_id, Arc::clone(&tab));
        }

        // If this is the first tab, make it active
        {
            let mut active = self.active_tab.write().await;
            if active.is_none() {
                *active = Some(tab_id);
                *tab.lifecycle.write().await = TabLifecycle::Active;
            }
        }

        info!(tab_id = %tab_id, profile = %profile_id, url = %url_str, "created new tab");

        self.event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(tab_id),
            None,
            None,
            BrowserEventKind::TabCreated {
                profile_id,
                url: url_str,
            },
        );

        Ok(tab_id)
    }

    /// Bind real CEF browser ID to a Tab (CEF Browser identity only).
    pub async fn bind_cef_browser(
        &self,
        tab_id: TabId,
        cef_browser_id: i32,
    ) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;
        {
            let mut ident = tab.identity.write().await;
            ident.cef_browser_id = Some(cef_browser_id);
        }
        self.navigation.bind_browser_id(tab_id, cef_browser_id).await;
        info!(
            tab_id = %tab_id,
            cef_browser_id = cef_browser_id,
            "bound real CEF browser instance to tab identity"
        );
        Ok(())
    }

    /// Bind physical surface ID to a Tab (Surface / HWND decoupled from identity).
    ///
    /// `surface_id` is a `BrowserSurfaceId` newtype, not a raw `usize`, to prevent
    /// accidental aliasing with array indices or other opaque integer handles.
    pub async fn bind_browser_surface(
        &self,
        tab_id: TabId,
        surface_id: BrowserSurfaceId,
    ) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;
        *tab.surface_id.write().await = Some(surface_id);
        info!(
            tab_id = %tab_id,
            surface_id = %surface_id,
            "bound surface descriptor to tab"
        );
        Ok(())
    }

    /// Bind CDP Target ID to a Tab (CDP association only, does NOT mutate browser identity).
    pub async fn bind_cdp_target(
        &self,
        tab_id: TabId,
        target_id: impl Into<String>,
    ) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;
        let tid = target_id.into();
        let binding = CdpBinding::new(tid.clone());
        *tab.cdp.write().await = Some(binding);

        info!(
            tab_id = %tab_id,
            target_id = %tid,
            "bound CDP target association to tab"
        );

        let cef_browser_id = tab.identity.read().await.cef_browser_id;

        self.event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(tab_id),
            cef_browser_id,
            Some(tid.clone()),
            BrowserEventKind::CdpTargetBound {
                target_id: tid,
            },
        );

        Ok(())
    }

    /// Close and teardown a tab.
    pub async fn close_tab(&self, tab_id: TabId) -> Result<(), BrowserError> {
        let tab = {
            let mut tabs = self.tabs.write().await;
            tabs.remove(&tab_id).ok_or(BrowserError::TabNotFound(tab_id))?
        };

        *tab.lifecycle.write().await = TabLifecycle::Closed;

        // Drain any pending operations for this tab via atomic CAS
        self.navigation
            .drain_operations_for_tab(tab_id, || BrowserError::ActionCancelled("tab closed".to_string()))
            .await;

        // If closed tab was active, switch to next available tab
        let next_tab = {
            let active = self.active_tab.read().await;
            if *active == Some(tab_id) {
                let tabs = self.tabs.read().await;
                tabs.keys().copied().next()
            } else {
                None
            }
        };

        if let Some(new_active) = next_tab {
            info!(new_active = %new_active, "switching active tab after close");
            self.switch_tab(new_active).await?;
        } else {
            let mut active = self.active_tab.write().await;
            if *active == Some(tab_id) {
                *active = None;
            }
        }

        info!(tab_id = %tab_id, "tab closed");
        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(tab_id),
            cef_id,
            cdp_id,
            BrowserEventKind::TabClosed,
        );
        Ok(())
    }

    /// Switch active focused tab.
    pub async fn switch_tab(&self, target_id: TabId) -> Result<(), BrowserError> {
        let tabs = self.tabs.read().await;
        let target_tab = tabs.get(&target_id).cloned().ok_or(BrowserError::TabNotFound(target_id))?;

        let previous = {
            let mut active = self.active_tab.write().await;
            let prev = *active;
            *active = Some(target_id);
            prev
        };

        *target_tab.lifecycle.write().await = TabLifecycle::Active;

        let (cef_id, cdp_id) = {
            let ident = target_tab.identity.read().await;
            let cdp = target_tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        info!(previous = ?previous, current = %target_id, "active tab switched");
        self.event_bus.emit(
            BrowserEventProducer::BrowserControl,
            Some(target_id),
            cef_id,
            cdp_id,
            BrowserEventKind::ActiveTabSwitched {
                previous,
                current: target_id,
            },
        );

        Ok(())
    }

    /// Retrieve tab instance by TabId.
    pub async fn get_tab(&self, tab_id: TabId) -> Result<Arc<Tab>, BrowserError> {
        let tabs = self.tabs.read().await;
        tabs.get(&tab_id)
            .cloned()
            .ok_or(BrowserError::TabNotFound(tab_id))
    }

    /// Currently active tab.
    pub async fn get_active_tab(&self) -> Option<TabId> {
        *self.active_tab.read().await
    }

    /// List all open tabs.
    pub async fn list_tabs(&self) -> Vec<TabSummary> {
        let tabs = self.tabs.read().await;
        let mut summaries = Vec::with_capacity(tabs.len());
        for tab in tabs.values() {
            summaries.push(tab.summary().await);
        }
        summaries
    }

    /// Handle renderer process termination (INV-11A: Failure cannot grant authority).
    pub async fn handle_renderer_crash(
        &self,
        tab_id: TabId,
        diagnostics: RendererCrashDiagnostics,
    ) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;

        error!(
            tab_id = %tab_id,
            status = ?diagnostics.termination_status,
            raw_cef = ?diagnostics.raw_cef_status,
            observed_at_ms = diagnostics.observed_at_ms,
            "CRITICAL: Renderer process terminated unexpectedly. Enforcing INV-11A fail-closed."
        );

        let new_health = TabHealth::RendererTerminated {
            status: diagnostics.termination_status,
            diagnostics: diagnostics.clone(),
        };
        *tab.health.write().await = new_health.clone();

        // Drain and fail-closed all inflight operations for this tab via atomic CAS
        let term_status = diagnostics.termination_status;
        self.navigation
            .drain_operations_for_tab(tab_id, move || BrowserError::RendererTerminated {
                tab_id,
                status: term_status,
            })
            .await;

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab_id),
            cef_id,
            cdp_id.clone(),
            BrowserEventKind::RendererProcessTerminated { diagnostics },
        );

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab_id),
            cef_id,
            cdp_id,
            BrowserEventKind::TabHealthChanged { health: new_health },
        );

        Ok(())
    }

    /// Handle address change from CefDisplayHandler.
    pub async fn handle_address_change(&self, tab_id: TabId, url: &str) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;
        *tab.url.write().await = url.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab_id),
            cef_id,
            cdp_id,
            BrowserEventKind::TabAddressChanged {
                url: url.to_string(),
            },
        );
        Ok(())
    }

    /// Handle title change from CefDisplayHandler.
    pub async fn handle_title_change(&self, tab_id: TabId, title: &str) -> Result<(), BrowserError> {
        let tab = self.get_tab(tab_id).await?;
        *tab.title.write().await = title.to_string();

        let (cef_id, cdp_id) = {
            let ident = tab.identity.read().await;
            let cdp = tab.cdp.read().await;
            (ident.cef_browser_id, cdp.as_ref().map(|c| c.target_id.clone()))
        };

        self.event_bus.emit(
            BrowserEventProducer::Cef,
            Some(tab_id),
            cef_id,
            cdp_id,
            BrowserEventKind::TabTitleChanged {
                title: title.to_string(),
            },
        );
        Ok(())
    }
}
