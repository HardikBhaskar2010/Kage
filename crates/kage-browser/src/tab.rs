//! Tab identity, orthogonal states, and internal representations.
//!
//! Enforces:
//! - `INV-10`: Every browser tab has an explicit (TabId, ProfileId, Option<CefBrowserId>) relationship.
//! - `INV-12`: Browser and surface identity are explicit and never inferred:
//!   - Authoritative browser identity: `(TabId, ProfileId, Option<CefBrowserId>)`.
//!   - CDP target binding: `TargetId` (associated with browser identity during Phase 4, not identity itself).
//!   - CDP session: `SessionId` (ephemeral multiplexed client session attachment, not identity).
//!   - BrowserSurface binding: `(TabId, BrowserSurfaceId)` (HWND/DPI/bounds decoupled from tab).
//!   - Never manufacture synthetic TargetIds before real CDP discovery.
//! - `Recovering` → `Healthy` transition:
//!   Driven exclusively by the real CEF `OnRenderViewReady` callback (or equivalent browser-readiness
//!   signal). KAGE must never self-transition to `Healthy` on a timer or assumption.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Authoritative unique identifier for a browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub Uuid);

impl TabId {
    pub fn new() -> Self {
        TabId(Uuid::new_v4())
    }
}

impl Default for TabId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Profile identifier defining storage and cookie partition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        ProfileId(id.into())
    }

    pub fn personal() -> Self {
        ProfileId("personal".to_string())
    }

    pub fn work() -> Self {
        ProfileId("work".to_string())
    }

    pub fn agent_sandbox() -> Self {
        ProfileId("agent_sandbox".to_string())
    }

    pub fn temporary() -> Self {
        ProfileId(format!("temp_{}", Uuid::new_v4().simple()))
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Authoritative browser identity: (TabId, ProfileId, Option<CefBrowserId>).
///
/// Invariant: CefBrowserId is assigned strictly when real CEF allocates a browser.
/// CDP TargetId and SessionId are associations, never browser identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserIdentity {
    pub tab_id: TabId,
    pub profile_id: ProfileId,
    pub cef_browser_id: Option<i32>,
}

impl BrowserIdentity {
    pub fn new(tab_id: TabId, profile_id: ProfileId) -> Self {
        Self {
            tab_id,
            profile_id,
            cef_browser_id: None,
        }
    }

    pub fn with_cef_browser(tab_id: TabId, profile_id: ProfileId, cef_browser_id: i32) -> Self {
        Self {
            tab_id,
            profile_id,
            cef_browser_id: Some(cef_browser_id),
        }
    }

    pub fn is_bound_to_cef(&self) -> bool {
        self.cef_browser_id.is_some()
    }
}

/// CDP target association established once CDP discovers the page target (Phase 4).
/// This is an association with BrowserIdentity, not an expanded identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CdpBinding {
    pub target_id: String,
}

impl CdpBinding {
    pub fn new(target_id: impl Into<String>) -> Self {
        Self {
            target_id: target_id.into(),
        }
    }
}

/// CDP session attachment representing an active multiplexed client session (Phase 4).
/// Sessions are ephemeral connection handles and do NOT form part of the browser's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CdpSession {
    pub session_id: String,
}

impl CdpSession {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }
}

/// High-level lifecycle stage of a browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabLifecycle {
    Created,
    Active,
    Closing,
    Closed,
}

/// Monotonic per-tab navigation attempt generation identifier.
/// Prevents callback overlap races (e.g. rapid navigate(A) followed by navigate(B)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NavigationId(pub u64);

impl std::fmt::Display for NavigationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Origin or trigger source of a navigation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationSource {
    Programmatic,
    UserGesture,
    HistoryBack,
    HistoryForward,
    Reload,
    Redirect,
    Restore,
    Unknown,
}

/// Specific reason for navigation cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationCancelCause {
    UserStop,
    Superseded,
    RendererCrashed,
    BrowserClosing,
    CefAborted,
}

/// Comprehensive navigation record storing correlation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationRecord {
    pub navigation_id: NavigationId,
    pub source: NavigationSource,
    pub requested_url: String,
    pub committed_url: Option<String>,
    pub final_url: Option<String>,
    pub cef_request_id: Option<u64>,
    pub started_at_ms: u64,
    pub committed_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
}

impl NavigationRecord {
    pub fn new(
        navigation_id: NavigationId,
        source: NavigationSource,
        requested_url: String,
        started_at_ms: u64,
    ) -> Self {
        Self {
            navigation_id,
            source,
            requested_url,
            committed_url: None,
            final_url: None,
            cef_request_id: None,
            started_at_ms,
            committed_at_ms: None,
            finished_at_ms: None,
        }
    }
}

/// Navigation status of the tab's primary frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationState {
    Idle,
    Loading {
        id: NavigationId,
        requested_url: String,
        effective_url: Option<String>,
        source: NavigationSource,
        cef_request_id: Option<u64>,
        started_at_ms: u64,
    },
    Committed {
        id: NavigationId,
        url: String,
        source: NavigationSource,
        cef_request_id: Option<u64>,
    },
    Completed {
        id: NavigationId,
        url: String,
        http_status: i32,
    },
    Failed {
        id: NavigationId,
        url: String,
        error_code: i32,
        reason: String,
    },
    Cancelled {
        id: NavigationId,
        url: String,
        cause: NavigationCancelCause,
    },
    SameDocumentNavigated {
        url: String,
    },
}

impl NavigationState {
    pub fn is_loading(&self) -> bool {
        matches!(self, NavigationState::Loading { .. } | NavigationState::Committed { .. })
    }

    pub fn navigation_id(&self) -> Option<NavigationId> {
        match self {
            NavigationState::Loading { id, .. }
            | NavigationState::Committed { id, .. }
            | NavigationState::Completed { id, .. }
            | NavigationState::Failed { id, .. }
            | NavigationState::Cancelled { id, .. } => Some(*id),
            NavigationState::Idle | NavigationState::SameDocumentNavigated { .. } => None,
        }
    }
}

/// CEF process termination status mapped from the real cef-rs 152 `TerminationStatus` type.
///
/// Note: `cef::TerminationStatus` in cef-rs is a struct with associated constants,
/// not a Rust enum. The named termination status values below correspond to the
/// well-defined CEF 152 constants:
///   `ProcessCrashed`, `ProcessOom`, `ProcessWasKilled`, `AbnormalTermination`,
///   `LaunchFailed`, `IntegrityFailure`.
/// An `Unknown(u32)` catch-all preserves any future CEF additions without silent reinterpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CefTerminationStatus {
    AbnormalTermination,
    ProcessWasKilled,
    ProcessCrashed,
    ProcessOom,
    /// Renderer process failed to start. Named CEF 152 termination status value.
    LaunchFailed,
    /// Renderer terminated by OS integrity/security enforcement. Named CEF 152 termination status value.
    IntegrityFailure,
    /// Raw value not matching any named CEF 152 termination status.
    /// Carries the raw value for forensic logging; must NOT be reinterpreted as any
    /// named security event.
    Unknown(u32),
}

/// Disaggregated renderer termination status.
///
/// - `Crashed`:          Process crashed (e.g. segfault, unhandled exception).
/// - `Oom`:              Renderer killed by OS due to memory exhaustion.
/// - `Killed`:           Renderer forcibly killed by the host or OS.
/// - `Abnormal`:         Abnormal exit not falling into the above categories.
/// - `LaunchFailed`:     Renderer process failed to start (CEF `LaunchFailed`).
/// - `IntegrityFailure`: Renderer terminated by OS integrity/security enforcement
///                       (CEF `IntegrityFailure`). `raw_cef_status` carries the
///                       original discriminant for forensic logging.
/// - `Unknown(u32)`:     Future or unrecognised raw CEF discriminant. MUST NOT be
///                       reinterpreted as a different named security event; telemetry
///                       must surface the raw value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererTerminationStatus {
    Crashed,
    Oom,
    Killed,
    Abnormal,
    LaunchFailed,
    /// Renderer terminated by OS integrity/security enforcement.
    /// Check `RendererCrashDiagnostics::raw_cef_status` for the exact discriminant.
    IntegrityFailure,
    /// Unrecognised raw discriminant from a future CEF version.
    /// Carries the raw value; never silently mapped to another security classification.
    Unknown(u32),
}

impl From<CefTerminationStatus> for RendererTerminationStatus {
    fn from(status: CefTerminationStatus) -> Self {
        match status {
            CefTerminationStatus::ProcessCrashed       => RendererTerminationStatus::Crashed,
            CefTerminationStatus::ProcessOom           => RendererTerminationStatus::Oom,
            CefTerminationStatus::ProcessWasKilled     => RendererTerminationStatus::Killed,
            CefTerminationStatus::AbnormalTermination  => RendererTerminationStatus::Abnormal,
            CefTerminationStatus::LaunchFailed         => RendererTerminationStatus::LaunchFailed,
            CefTerminationStatus::IntegrityFailure     => RendererTerminationStatus::IntegrityFailure,
            // Preserve the raw discriminant. Do NOT silently rename unknown OS/process
            // termination events to a different security classification.
            CefTerminationStatus::Unknown(raw)         => RendererTerminationStatus::Unknown(raw),
        }
    }
}

/// Diagnostic metadata captured when a renderer process terminates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendererCrashDiagnostics {
    pub termination_status: RendererTerminationStatus,
    pub raw_cef_status: CefTerminationStatus,
    pub observed_at_ms: u64,
}

/// Opaque surface identity binding a CEF windowed surface (HWND / Cocoa NSView child) to a tab.
///
/// Deliberately a UUID-backed newtype, not a raw `usize`, to prevent accidental aliasing
/// with array indices or other opaque handles. HWND/DPI/bounds are stored separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserSurfaceId(pub Uuid);

impl BrowserSurfaceId {
    pub fn new() -> Self {
        BrowserSurfaceId(Uuid::new_v4())
    }
}

impl Default for BrowserSurfaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BrowserSurfaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "surface:{}", self.0.simple())
    }
}

/// Process and responsiveness health status of the tab.
///
/// ## State Transitions
/// - `Healthy`  → `Unresponsive` : CEF hung-renderer detection fires.
/// - `Healthy`  → `RendererTerminated` : CEF `OnRenderProcessTerminated` fires;
///   the host increments `Tab::renderer_epoch` at this point.
/// - `RendererTerminated` → `Recovering` : Host explicitly initiates recovery and
///   captures the current `Tab::renderer_epoch` as `recovery_epoch`.
/// - `Recovering` → `Healthy` : CEF `OnRenderViewReady` fires.
///   **Important:** CEF does NOT provide an epoch argument in `OnRenderViewReady`.
///   The host validates:
///     (a) the tab's current health is `Recovering`, AND
///     (b) `Tab::renderer_epoch` still matches the `recovery_epoch` stored in
///         the `Recovering` variant (proving no newer crash cycle superseded this one).
///   If either check fails, the callback is silently discarded.
///   `renderer_epoch` rejects readiness when a newer crash/recovery generation has
///   superseded the current recovery. It does not identify the renderer instance
///   associated with an individual `OnRenderViewReady` callback.
///   True renderer-instance correlation requires a renderer-side handshake.
///   KAGE MUST NOT self-transition to `Healthy` on a timer or assumption.
/// - `Unresponsive` → `Healthy` : CEF reports renderer is responsive again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabHealth {
    Healthy,
    Unresponsive,
    RendererTerminated {
        status: RendererTerminationStatus,
        diagnostics: RendererCrashDiagnostics,
    },
    Recovering {
        /// Epoch value captured from `Tab::renderer_epoch` when this recovery was initiated.
        /// `renderer_epoch` rejects readiness when a newer crash/recovery generation has
        /// superseded the current recovery. It does not identify the renderer instance
        /// associated with an individual `OnRenderViewReady` callback.
        /// True renderer-instance correlation requires a renderer-side handshake.
        recovery_epoch: u64,
    },
}

impl TabHealth {
    pub fn is_healthy(&self) -> bool {
        matches!(self, TabHealth::Healthy)
    }

    pub fn is_crashed(&self) -> bool {
        matches!(self, TabHealth::RendererTerminated { .. })
    }

    pub fn is_recovering(&self) -> bool {
        matches!(self, TabHealth::Recovering { .. })
    }
}

/// Serializable snapshot of tab metadata across all orthogonal axes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSummary {
    pub identity: BrowserIdentity,
    pub cdp: Option<CdpBinding>,
    /// Opaque surface binding decoupled from CEF identity (HWND/DPI stored separately).
    pub surface_id: Option<BrowserSurfaceId>,
    pub lifecycle: TabLifecycle,
    pub navigation: NavigationState,
    pub health: TabHealth,
    pub url: String,
    pub title: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl TabSummary {
    pub fn id(&self) -> TabId {
        self.identity.tab_id
    }

    pub fn profile_id(&self) -> &ProfileId {
        &self.identity.profile_id
    }

    pub fn cef_browser_id(&self) -> Option<i32> {
        self.identity.cef_browser_id
    }
}

/// Internal thread-safe representation of a single browser tab.
pub struct Tab {
    pub id: TabId,
    pub profile_id: ProfileId,
    pub identity: Arc<RwLock<BrowserIdentity>>,
    pub cdp: Arc<RwLock<Option<CdpBinding>>>,
    /// Opaque surface binding decoupled from CEF identity. Assigned when a native
    /// windowed child surface (HWND / Cocoa NSView) is created; `None` until then.
    pub surface_id: Arc<RwLock<Option<BrowserSurfaceId>>>,
    pub lifecycle: Arc<RwLock<TabLifecycle>>,
    pub navigation: Arc<RwLock<NavigationState>>,
    pub health: Arc<RwLock<TabHealth>>,
    pub url: Arc<RwLock<String>>,
    pub title: Arc<RwLock<String>>,
    pub can_go_back: Arc<AtomicBool>,
    pub can_go_forward: Arc<AtomicBool>,
    pub active_navigation_record: Arc<RwLock<Option<NavigationRecord>>>,
    /// Host-side renderer recovery generation counter.
    ///
    /// This is NOT a value that CEF reports. `OnRenderViewReady` carries no epoch argument.
    /// It is a host-maintained monotonic counter that the host increments each time the
    /// renderer terminates. The current value is captured into `TabHealth::Recovering {
    /// recovery_epoch }` when recovery begins.
    ///
    /// When `OnRenderViewReady` fires, the host validates:
    ///   - tab health is currently `Recovering`, AND
    ///   - the stored `recovery_epoch` == current `renderer_epoch.load()`
    ///
    /// `renderer_epoch` rejects readiness when a newer crash/recovery generation has
    /// superseded the current recovery. It does not identify the renderer instance
    /// associated with an individual `OnRenderViewReady` callback.
    /// True renderer-instance correlation requires a renderer-side handshake.
    pub renderer_epoch: Arc<AtomicU64>,
}

impl Tab {
    pub fn new(id: TabId, profile_id: ProfileId, initial_url: impl Into<String>) -> Self {
        let initial_url = initial_url.into();
        let identity = BrowserIdentity::new(id, profile_id.clone());

        Self {
            id,
            profile_id,
            identity: Arc::new(RwLock::new(identity)),
            cdp: Arc::new(RwLock::new(None)),
            surface_id: Arc::new(RwLock::new(None)),
            lifecycle: Arc::new(RwLock::new(TabLifecycle::Created)),
            navigation: Arc::new(RwLock::new(NavigationState::Idle)),
            health: Arc::new(RwLock::new(TabHealth::Healthy)),
            url: Arc::new(RwLock::new(initial_url)),
            title: Arc::new(RwLock::new("New Tab".to_string())),
            can_go_back: Arc::new(AtomicBool::new(false)),
            can_go_forward: Arc::new(AtomicBool::new(false)),
            active_navigation_record: Arc::new(RwLock::new(None)),
            renderer_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Snapshot current tab status across all orthogonal axes.
    pub async fn summary(&self) -> TabSummary {
        let identity = self.identity.read().await.clone();
        let cdp = self.cdp.read().await.clone();
        let surface_id = *self.surface_id.read().await;
        let lifecycle = *self.lifecycle.read().await;
        let navigation = self.navigation.read().await.clone();
        let health = self.health.read().await.clone();
        let url = self.url.read().await.clone();
        let title = self.title.read().await.clone();
        let can_go_back = self.can_go_back.load(Ordering::Relaxed);
        let can_go_forward = self.can_go_forward.load(Ordering::Relaxed);

        TabSummary {
            identity,
            cdp,
            surface_id,
            lifecycle,
            navigation,
            health,
            url,
            title,
            can_go_back,
            can_go_forward,
        }
    }
}
