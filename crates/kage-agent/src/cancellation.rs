//! Root cancellation and STOP control plane for KAGE autonomous agent.
//!
//! Enforces **INV-09**: Once STOP is signaled, active operations are aborted
//! and no subsequent agent steps or ToolBus dispatches can be scheduled.

use tokio_util::sync::CancellationToken;

/// Root cancellation manager for an autonomous agent execution lifecycle.
#[derive(Debug, Clone)]
pub struct AgentCancellation {
    root: CancellationToken,
}

impl Default for AgentCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentCancellation {
    /// Create a new active agent cancellation root.
    pub fn new() -> Self {
        Self {
            root: CancellationToken::new(),
        }
    }

    /// Trigger user-initiated STOP.
    ///
    /// Aborts any in-flight execution and prevents any subsequent planner steps.
    pub fn stop(&self) {
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
