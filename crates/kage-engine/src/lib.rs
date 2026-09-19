//! KAGE Native Chromium/CEF Engine Foundation (Phase 2).
//!
//! Exposes core production foundations:
//! - [`CefRuntime`]: Central lifecycle coordinator (CEF-01, CEF-03b, CEF-04, CEF-10).
//! - [`CefBrowserSurface`]: Win32 child browser surface manager (CEF-05, CEF-06, CEF-13).
//! - [`CefUiExecutor`]: Cross-thread task marshaller (Tauri thread ──► CEF UI thread).
//! - [`NativeSurfaceManager`]: Win32 physical coordinate and surface composition coordinator.
//! - [`SubprocessManager`]: CEF helper executable and runtime packaging validator (CEF-02, CEF-12).
//! - Strongly typed coordinate systems (`TauriLogicalPoint`, `Win32ClientPoint`, `CefViewPoint`, etc.).
//! - Unified error hierarchy ([`EngineError`], [`CefExecutorError`]).

pub mod composition;
pub mod coordinates;
pub mod errors;
pub mod executor;
pub mod runtime;
pub mod subprocess;
pub mod surface;

#[cfg(test)]
pub mod integration_tests;

pub use composition::{ChromeLayoutConfig, DualSurfaceLayout, NativeSurfaceManager, ViewportRect};
pub use coordinates::{
    transform_logical_to_css, BrowserViewportTransform, CefViewPoint, CssPoint, DpiAwareness,
    DpiContext, PhysicalPixelPoint, TauriLogicalPoint, Win32ClientPoint, WindowGeometry,
};
pub use errors::{CefExecutorError, EngineError};
pub use executor::{BoxedUiTask, CefUiExecutor};
pub use runtime::{CefEngineState, CefRuntime, RuntimeConfig};
pub use subprocess::SubprocessManager;
pub use surface::{
    CefBrowserSurface, PopupPolicyDecision, PopupRequest, SurfaceLifecycleState,
};
