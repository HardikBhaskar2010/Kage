//! KAGE Browser Control Plane (`kage-browser`).
//!
//! Manages tab lifecycles, profile partitions, navigation actions,
//! surface synchronization, and renderer failure isolation.
//!
//! # Invariants Enforced
//! - `INV-10`: Every browser tab has an explicit (TabId, ProfileId, Option<CefBrowserId>) relationship.
//! - `INV-11A`: Renderer process failure cannot grant authority or produce success (Renderer crash fails closed).
//! - `INV-11B`: Engine host process failure aborts operations globally.
//! - `INV-12`: Browser and surface identity are explicit and never inferred.

pub mod errors;
pub mod events;
pub mod manager;
pub mod navigation;
pub mod profile;
pub mod tab;

pub use errors::BrowserError;
pub use events::{
    BrowserEvent, BrowserEventBus, BrowserEventKind, BrowserEventProducer, EventClass,
    ResyncRequired, TabStateSnapshot,
};
pub use manager::TabManager;
pub use navigation::{BrowserOperationId, NavigationController, PendingOperation};
pub use profile::{Profile, ProfileKind, ProfileManager};
pub use tab::{
    BrowserIdentity, BrowserSurfaceId, CdpBinding, CdpSession, CefTerminationStatus,
    NavigationCancelCause, NavigationId, NavigationRecord, NavigationSource, NavigationState,
    ProfileId, RendererCrashDiagnostics, RendererTerminationStatus, Tab, TabHealth, TabId,
    TabLifecycle, TabSummary,
};
