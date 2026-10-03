//! Atomic admission gate for ToolBus dispatching and STOP boundaries.
//!
//! Enforces **INV-09**: Once STOP is initiated or Human Takeover is active,
//! zero requests can enter the execution queue. Eliminates check-then-act races
//! through an atomic state machine and RAII dispatch permits.
//!
//! # STOP Linearization Point & Concurrency Contract
//! 1. **STOP Linearization Point**:
//!    - Calling `seal_cancelled()` closes future admission immediately.
//!    - Executions already admitted before STOP may complete or be cancelled
//!      according to their tool's cancellation token semantics.
//!    - No new `ToolRequest` may be admitted after the STOP linearization point.
//!
//! 2. **Human Takeover Execution Barrier & Concurrency Contract**:
//!    - Entering takeover (`enter_takeover()`) closes future autonomous agent admission immediately.
//!    - Pre-takeover admitted actions are allowed to settle or cancel according to semantics.
//!    - Post-takeover new agent dispatches are rejected immediately with `ToolError::HumanTakeoverActive`.
//!    - Upon user resumption (`resume_open()`), the admission gate reopens, fresh baseline observation
//!      is captured, and autonomous execution resumes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Operational admission state of the ToolBus execution gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionState {
    /// Gateway is open; autonomous and interactive dispatches admitted.
    Open,
    /// Gateway is sealed by user or system STOP; all subsequent dispatches denied.
    Cancelled,
    /// Gateway is locked for exclusive human intervention; autonomous dispatches blocked.
    HumanTakeover,
}

/// Reasons for dispatch denial at the admission gate boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchDenial {
    /// Denied due to user or runtime STOP signal.
    Cancelled,
    /// Denied due to active human takeover on the browser surface.
    TakeoverActive,
}

/// RAII permit proving atomic admission into the execution gateway.
///
/// Automatically decrements the active in-flight counter when dropped.
#[derive(Debug)]
pub struct DispatchPermit {
    counter: Arc<AtomicUsize>,
}

impl Drop for DispatchPermit {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Atomic admission gate governing entry into the ToolBus dispatch boundary.
#[derive(Debug, Clone)]
pub struct DispatchAdmissionGate {
    state: Arc<RwLock<AdmissionState>>,
    in_flight: Arc<AtomicUsize>,
}

impl Default for DispatchAdmissionGate {
    fn default() -> Self {
        Self::new()
    }
}

impl DispatchAdmissionGate {
    /// Create a new, open admission gate.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AdmissionState::Open)),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Atomically check state and acquire an in-flight dispatch permit.
    ///
    /// Once sealed (`Cancelled` or `HumanTakeover`), all calls immediately fail with
    /// `DispatchDenial`. Any permit acquired prior to sealing represents an already-admitted
    /// dispatch that may complete or cancel according to its tool's cancellation token.
    pub fn acquire_permit(&self) -> Result<DispatchPermit, DispatchDenial> {
        let guard = self.state.read().unwrap();
        match *guard {
            AdmissionState::Open => {
                self.in_flight.fetch_add(1, Ordering::SeqCst);
                Ok(DispatchPermit {
                    counter: self.in_flight.clone(),
                })
            }
            AdmissionState::Cancelled => Err(DispatchDenial::Cancelled),
            AdmissionState::HumanTakeover => Err(DispatchDenial::TakeoverActive),
        }
    }

    /// Atomically seal the admission boundary. Zero dispatches are admitted after this returns.
    pub fn seal_cancelled(&self) {
        let mut guard = self.state.write().unwrap();
        *guard = AdmissionState::Cancelled;
    }

    /// Atomically lock the boundary for exclusive human interaction.
    pub fn enter_takeover(&self) {
        let mut guard = self.state.write().unwrap();
        *guard = AdmissionState::HumanTakeover;
    }

    /// Atomically resume Open mode.
    pub fn resume_open(&self) {
        let mut guard = self.state.write().unwrap();
        *guard = AdmissionState::Open;
    }

    /// Get current admission state.
    pub fn current_state(&self) -> AdmissionState {
        *self.state.read().unwrap()
    }

    /// Check if admission gate is open.
    pub fn is_open(&self) -> bool {
        *self.state.read().unwrap() == AdmissionState::Open
    }

    /// Check if admission gate is cancelled (STOP triggered).
    pub fn is_cancelled(&self) -> bool {
        *self.state.read().unwrap() == AdmissionState::Cancelled
    }

    /// Check if admission gate is in human takeover mode.
    pub fn is_takeover(&self) -> bool {
        *self.state.read().unwrap() == AdmissionState::HumanTakeover
    }

    /// Count of currently active in-flight tool dispatches.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }
}
