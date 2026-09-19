//! Native CEF child browser surface manager (Phase 2).
//!
//! Enforces:
//! - **CEF-05**: Native child HWND attachment (`GetParent(child_hwnd) == parent_hwnd`).
//! - **CEF-06**: Physical bounds synchronization with zero overlap.
//! - **CEF-10**: Non-blocking asynchronous lifecycle state transitions.
//! - **CEF-13**: Popup policy & orphan prevention (`on_before_popup` inspection).

use crate::composition::ViewportRect;
use crate::errors::EngineError;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::info;

/// Lifecycle state of an individual CEF browser surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLifecycleState {
    /// Actively rendering and interactive.
    Active = 0,
    /// Teardown requested via `try_close_browser()`; waiting for `OnBeforeClose`.
    Closing = 1,
    /// Completely destroyed via `OnBeforeClose`. Child HWND is no longer valid.
    Closed = 2,
}

impl From<u8> for SurfaceLifecycleState {
    fn from(val: u8) -> Self {
        match val {
            0 => SurfaceLifecycleState::Active,
            1 => SurfaceLifecycleState::Closing,
            _ => SurfaceLifecycleState::Closed,
        }
    }
}

/// Represents a popup interception event (Gate CEF-13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupRequest {
    pub opener_surface_id: usize,
    pub target_url: String,
    pub target_frame_name: Option<String>,
}

/// Policy decision for popup requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPolicyDecision {
    /// Reject the popup; CEF safely discards the popup window.
    Block,
    /// Defer creation to the host (Phase 3 TabManager opens a new tab).
    DeferToHost,
}

/// A native Win32 child browser surface hosting Chromium.
pub struct CefBrowserSurface {
    surface_id: usize,
    parent_hwnd: isize,
    child_hwnd: Option<isize>,
    current_bounds: ViewportRect,
    state: AtomicU8,
    active_counter: Arc<AtomicUsize>,
}

impl CefBrowserSurface {
    /// Create a new surface instance in `Active` state.
    pub fn new(
        surface_id: usize,
        parent_hwnd: isize,
        initial_bounds: ViewportRect,
        active_counter: Arc<AtomicUsize>,
    ) -> Self {
        active_counter.fetch_add(1, Ordering::SeqCst);
        Self {
            surface_id,
            parent_hwnd,
            child_hwnd: None,
            current_bounds: initial_bounds,
            state: AtomicU8::new(SurfaceLifecycleState::Active as u8),
            active_counter,
        }
    }

    pub fn surface_id(&self) -> usize {
        self.surface_id
    }

    pub fn parent_hwnd(&self) -> isize {
        self.parent_hwnd
    }

    pub fn child_hwnd(&self) -> Option<isize> {
        self.child_hwnd
    }

    pub fn attach_child_hwnd(&mut self, hwnd: isize) {
        self.child_hwnd = Some(hwnd);
    }

    pub fn current_bounds(&self) -> ViewportRect {
        self.current_bounds
    }

    pub fn lifecycle_state(&self) -> SurfaceLifecycleState {
        self.state.load(Ordering::SeqCst).into()
    }

    /// Request graceful close of the surface (CEF-10).
    pub fn request_close(&self) -> Result<(), EngineError> {
        let prev = self.state.swap(SurfaceLifecycleState::Closing as u8, Ordering::SeqCst);
        if prev == SurfaceLifecycleState::Closed as u8 {
            return Err(EngineError::Lifecycle(
                "Cannot close an already-closed surface".to_string(),
            ));
        }
        Ok(())
    }

    /// Confirms that Chromium called `OnBeforeClose` (CEF-10).
    pub fn confirm_closed(&mut self) {
        let prev = self.state.swap(SurfaceLifecycleState::Closed as u8, Ordering::SeqCst);
        if prev != SurfaceLifecycleState::Closed as u8 {
            self.active_counter.fetch_sub(1, Ordering::SeqCst);
        }
        self.child_hwnd = None;
    }

    /// Synchronize the child HWND dimensions to new viewport bounds.
    #[cfg(windows)]
    pub fn update_bounds(&mut self, new_bounds: ViewportRect) -> Result<(), EngineError> {
        self.current_bounds = new_bounds;
        if let Some(child) = self.child_hwnd {
            unsafe {
                crate::composition::NativeSurfaceManager::set_hwnd_bounds(child, &new_bounds)?;
            }
        }
        Ok(())
    }

    /// Evaluate a popup request deterministically (Gate CEF-13).
    ///
    /// In Phase 2, popups are rejected/blocked to prevent orphaned unmanaged HWNDs.
    /// In Phase 3, this cleanly defers to `TabManager::create_tab`.
    pub fn evaluate_popup_policy(&self, request: &PopupRequest) -> PopupPolicyDecision {
        info!(
            opener = request.opener_surface_id,
            url = %request.target_url,
            "Gate CEF-13: Intercepted popup request"
        );
        // Phase 2 baseline: reject unmanaged popups to prevent orphaned windows
        PopupPolicyDecision::Block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_lifecycle_transitions() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut surface = CefBrowserSurface::new(
            1,
            0x1234,
            ViewportRect::new(320, 88, 1120, 812),
            counter.clone(),
        );

        assert_eq!(surface.lifecycle_state(), SurfaceLifecycleState::Active);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Request close
        assert!(surface.request_close().is_ok());
        assert_eq!(surface.lifecycle_state(), SurfaceLifecycleState::Closing);
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Still 1 until OnBeforeClose

        // OnBeforeClose callback
        surface.confirm_closed();
        assert_eq!(surface.lifecycle_state(), SurfaceLifecycleState::Closed);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_cef_13_popup_interception() {
        let counter = Arc::new(AtomicUsize::new(0));
        let surface = CefBrowserSurface::new(
            42,
            0x1234,
            ViewportRect::new(0, 0, 800, 600),
            counter,
        );

        let req = PopupRequest {
            opener_surface_id: 42,
            target_url: "https://example.com/popup".to_string(),
            target_frame_name: Some("popup_win".to_string()),
        };

        let decision = surface.evaluate_popup_policy(&req);
        assert_eq!(decision, PopupPolicyDecision::Block);
    }
}
