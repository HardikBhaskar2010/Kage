//! Root cancellation and STOP control plane for KAGE autonomous agent.
//!
//! Enforces **INV-09**: STOP cancels the agent runtime and active tool awaiters,
//! prevents subsequent dispatches, and discards queued plan steps.
//! It does not claim rollback or preemption of already-issued browser mutations
//! or in-flight Chromium execution (no retrospective rollback illusion).
//!
//! Enforces atomic admission: sealing the [`DispatchAdmissionGate`] guarantees zero
//! requests enter ToolBus dispatch after STOP is triggered (Correction #4).

use tokio_util::sync::CancellationToken;
use kage_core::admission::DispatchAdmissionGate;

/// Root cancellation manager for an autonomous agent execution lifecycle.
#[derive(Debug, Clone)]
pub struct AgentCancellation {
    root: CancellationToken,
    admission_gate: DispatchAdmissionGate,
}

impl Default for AgentCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentCancellation {
    /// Create a new active agent cancellation root with a default admission gate.
    pub fn new() -> Self {
        Self {
            root: CancellationToken::new(),
            admission_gate: DispatchAdmissionGate::new(),
        }
    }

    /// Bind an explicit shared admission gate.
    pub fn with_admission_gate(mut self, gate: DispatchAdmissionGate) -> Self {
        self.admission_gate = gate;
        self
    }

    /// Access the underlying admission gate.
    pub fn admission_gate(&self) -> &DispatchAdmissionGate {
        &self.admission_gate
    }

    /// Trigger user-initiated STOP.
    ///
    /// # Concurrency Contract
    /// - **STOP closes future admission**: `admission_gate.seal_cancelled()` acts as the atomic
    ///   linearization point. No new `ToolRequest` may be admitted after this point.
    /// - **Pre-admitted permits**: Executions already admitted before STOP may complete or be
    ///   cancelled according to their tool's cancellation token semantics.
    /// - Cancels the agent runtime and active tool awaiters, prevents subsequent
    ///   dispatches, and discards queued plan steps. Does not claim rollback or
    ///   preemption of already-issued browser mutations or in-flight Chromium execution.
    pub fn stop(&self) {
        self.admission_gate.seal_cancelled();
        self.root.cancel();
    }

    /// Check if STOP has been triggered.
    pub fn is_stopped(&self) -> bool {
        self.root.is_cancelled()
    }

    /// Obtain the root cancellation token.
    pub fn token(&self) -> CancellationToken {
        self.root.clone()
    }

    /// Derive a child cancellation token for an individual step or tool dispatch.
    pub fn child_token(&self) -> CancellationToken {
        self.root.child_token()
    }
}
