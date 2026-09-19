//! Browser event bus and telemetry stream.
//!
//! Provides a normalized, sequentially ordered, loss-aware event stream for
//! UI chrome synchronization, tab lifecycle tracking, and deterministic verification.
//!
//! Ordering & Provenance Contract:
//! - Event ordering is governed strictly by `sequence: u64` (atomic monotonic).
//! - Event provenance is explicit via `producer: BrowserEventProducer`.
//! - `monotonic_time_ns: u64` is used for interval and latency measurement.
//! - `wall_time_ms: u64` is used for human readability and audit trail correlation.
//! - Events are separated into `Critical` (retained in replay window) and `Telemetry` (lossy/metered).
//! - When a subscriber falls behind the replay window, `ResyncRequired` triggers an authoritative
//!   `StateSnapshot` rather than ambiguous replay failure.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::tab::{
    NavigationCancelCause, NavigationId, NavigationSource, NavigationState, ProfileId,
    RendererCrashDiagnostics, TabHealth, TabId, TabLifecycle, TabSummary,
};

/// Explicit producer origin of an event to eliminate provenance ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserEventProducer {
    /// Direct Chromium Embedded Framework callback (LoadHandler, DisplayHandler, RequestHandler).
    Cef,
    /// Central Tab and active window controller.
    BrowserControl,
    /// NavigationController state coordinator.
    NavigationController,
    /// Profile directory and RequestContext partition manager.
    ProfileManager,
    /// Native Win32 / Cocoa child surface and HWND composition manager.
    SurfaceManager,
}

/// Semantic categorization of browser events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventClass {
    /// Essential browser state transitions (lifecycle, navigation, crashes).
    /// These are retained in a ring buffer to allow gap detection and re-synchronization.
    Critical,
    /// High-volume operational stream (console, progress, metrics).
    /// Bounded and droppable; loss counters track drop counts.
    Telemetry,
}

/// Authoritative snapshot of all open tabs and active focus for full state resynchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabStateSnapshot {
    pub sequence: u64,
    pub tabs: Vec<TabSummary>,
    pub active_tab: Option<TabId>,
    pub generated_at_ms: u64,
}

/// Granular, typed event discriminator for browser control plane transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserEventKind {
    /// A new tab was created.
    TabCreated {
        profile_id: ProfileId,
        url: String,
    },
    /// Tab lifecycle transitioned (Created -> Active -> Closing -> Closed).
    TabLifecycleChanged {
        lifecycle: TabLifecycle,
    },
    /// Navigation initiated / loading started.
    NavigationStarted {
        id: NavigationId,
        requested_url: String,
        source: NavigationSource,
    },
    /// Navigation committed (OnLoadStart for main frame).
    NavigationCommitted {
        id: NavigationId,
        url: String,
        source: NavigationSource,
    },
    /// Navigation completed successfully (OnLoadEnd for main frame).
    NavigationCompleted {
        id: NavigationId,
        url: String,
        http_status: i32,
    },
    /// Navigation failed (OnLoadError for main frame).
    NavigationFailed {
        id: NavigationId,
        url: String,
        error_code: i32,
        reason: String,
    },
    /// Navigation was aborted or superseded.
    NavigationCancelled {
        id: NavigationId,
        url: String,
        cause: NavigationCancelCause,
    },
    /// In-page / same-document fragment or history state change.
    SameDocumentNavigated {
        url: String,
    },
    /// Full navigation state synchronization snapshot for a tab.
    NavigationStateChanged {
        state: NavigationState,
    },
    /// Authoritative multi-tab state snapshot emitted during initial connection or gap resync.
    StateSnapshot {
        snapshot: TabStateSnapshot,
    },
    /// Page title was updated by Chromium DOM display handler.
    TabTitleChanged {
        title: String,
    },
    /// URL address updated by Chromium display handler.
    TabAddressChanged {
        url: String,
    },
    /// Tab process and responsiveness health changed.
    TabHealthChanged {
        health: TabHealth,
    },
    /// Renderer process terminated unexpectedly (INV-11A failure signal).
    RendererProcessTerminated {
        diagnostics: RendererCrashDiagnostics,
    },
    /// Active focused tab switched.
    ActiveTabSwitched {
        previous: Option<TabId>,
        current: TabId,
    },
    /// Native viewport bounds resized.
    ViewportResized {
        width: u32,
        height: u32,
    },
    /// Tab destroyed and native resources released.
    TabClosed,
    /// Telemetry: Console message.
    ConsoleMessage {
        level: String,
        message: String,
    },
    /// Telemetry: Page load progress (0.0 - 1.0).
    LoadProgress {
        progress: f64,
    },
}

impl BrowserEventKind {
    pub fn class(&self) -> EventClass {
        match self {
            BrowserEventKind::ConsoleMessage { .. } | BrowserEventKind::LoadProgress { .. } => {
                EventClass::Telemetry
            }
            _ => EventClass::Critical,
        }
    }
}

/// Normalized, sequentially ordered browser event envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserEvent {
    /// Monotonically increasing sequence number for deterministic ordering.
    pub sequence: u64,
    /// Monotonic clock offset in nanoseconds since process launch.
    pub monotonic_time_ns: u64,
    /// Wall-clock timestamp in milliseconds since UNIX epoch.
    pub wall_time_ms: u64,
    /// Explicit producer subsystem origin.
    pub producer: BrowserEventProducer,
    /// Classification (Critical vs Telemetry).
    pub class: EventClass,
    /// Associated Tab identifier (if tab-specific).
    pub tab_id: Option<TabId>,
    /// Native CEF browser identifier (if bound).
    pub cef_browser_id: Option<i32>,
    /// CDP target identifier (if attached during Phase 4+).
    pub cdp_target_id: Option<String>,
    /// Typed event payload.
    pub kind: BrowserEventKind,
}

/// Signal returned when an event subscriber detects a sequence gap that has fallen outside the replay buffer.
/// Triggers the defined recovery contract: read authoritative TabManager state and emit StateSnapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResyncRequired {
    pub requested_sequence: u64,
    pub oldest_available_sequence: u64,
    pub current_sequence: u64,
}

/// Broadcast coordinator providing guaranteed sequential ordering, replay for critical events,
/// provenance tracking, and drop-aware telemetry metering.
#[derive(Clone)]
pub struct BrowserEventBus {
    sender: broadcast::Sender<BrowserEvent>,
    sequence_counter: Arc<AtomicU64>,
    start_instant: Instant,
    critical_replay_buffer: Arc<Mutex<VecDeque<BrowserEvent>>>,
    replay_capacity: usize,
    dropped_telemetry_count: Arc<AtomicU64>,
}

impl BrowserEventBus {
    /// Create a new event bus with specified broadcast channel capacity and critical replay window size.
    pub fn new(capacity: usize) -> Self {
        Self::with_replay_capacity(capacity, 1000)
    }

    pub fn with_replay_capacity(capacity: usize, replay_capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            sequence_counter: Arc::new(AtomicU64::new(1)),
            start_instant: Instant::now(),
            critical_replay_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(replay_capacity))),
            replay_capacity,
            dropped_telemetry_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Construct and emit a sequenced, provenance-tagged, dual-timestamped event.
    pub fn emit(
        &self,
        producer: BrowserEventProducer,
        tab_id: Option<TabId>,
        cef_browser_id: Option<i32>,
        cdp_target_id: Option<String>,
        kind: BrowserEventKind,
    ) -> BrowserEvent {
        let sequence = self.sequence_counter.fetch_add(1, Ordering::SeqCst);
        let monotonic_time_ns = self.start_instant.elapsed().as_nanos() as u64;
        let wall_time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let class = kind.class();

        let event = BrowserEvent {
            sequence,
            monotonic_time_ns,
            wall_time_ms,
            producer,
            class,
            tab_id,
            cef_browser_id,
            cdp_target_id,
            kind,
        };

        // If this is a Critical event, store in the ring replay buffer
        if class == EventClass::Critical {
            if let Ok(mut buffer) = self.critical_replay_buffer.lock() {
                if buffer.len() >= self.replay_capacity {
                    buffer.pop_front();
                }
                buffer.push_back(event.clone());
            }
        }

        // Broadcast to live subscribers; if send fails because no subscribers, that's fine
        if let Err(_send_err) = self.sender.send(event.clone()) {
            if class == EventClass::Telemetry {
                self.dropped_telemetry_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        event
    }

    /// Subscribe to live browser events.
    pub fn subscribe(&self) -> broadcast::Receiver<BrowserEvent> {
        self.sender.subscribe()
    }

    /// Read the current sequence counter value.
    pub fn current_sequence(&self) -> u64 {
        self.sequence_counter.load(Ordering::SeqCst)
    }

    /// Total count of dropped telemetry events.
    pub fn dropped_telemetry_count(&self) -> u64 {
        self.dropped_telemetry_count.load(Ordering::Relaxed)
    }

    /// Replay critical events starting from `from_sequence` (inclusive).
    /// If `from_sequence` is older than the oldest retained event, returns `Err(ResyncRequired)`.
    pub fn replay_critical_events(&self, from_sequence: u64) -> Result<Vec<BrowserEvent>, ResyncRequired> {
        let buffer = self.critical_replay_buffer.lock().map_err(|_| ResyncRequired {
            requested_sequence: from_sequence,
            oldest_available_sequence: 1,
            current_sequence: self.current_sequence(),
        })?;

        if buffer.is_empty() {
            return Ok(Vec::new());
        }

        let oldest_seq = buffer.front().map(|e| e.sequence).unwrap_or(1);
        if from_sequence < oldest_seq {
            return Err(ResyncRequired {
                requested_sequence: from_sequence,
                oldest_available_sequence: oldest_seq,
                current_sequence: self.current_sequence(),
            });
        }

        let replayed = buffer
            .iter()
            .filter(|e| e.sequence >= from_sequence)
            .cloned()
            .collect();

        Ok(replayed)
    }
}
