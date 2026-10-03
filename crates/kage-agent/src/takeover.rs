//! Human takeover escape hatch and state resynchronization engine.
//!
//! Provides bidirectional handover between autonomous execution and manual human control:
//! 1. User requests takeover (e.g. clicking "Take Control" or pressing `Esc`).
//! 2. Agent halts active step awaiter and transitions task to [`TaskState::HumanTakeover`].
//! 3. Keyboard and mouse input focus yields to the user on the native child HWND.
//! 4. Continuous telemetry streams to the UI and audit ledger while user performs manual actions.
//! 5. User clicks "Resume Agent": KAGE captures fresh telemetry and resynchronizes the plan baseline.
//!
//! # Execution Exclusivity (Correction #5)
//! Human Takeover is an execution mode and state barrier: while active, the
//! [`DispatchAdmissionGate`] blocks all autonomous tool dispatches until explicit user resumption.

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use kage_core::admission::DispatchAdmissionGate;
use crate::task::{AgentTask, TaskState};
use crate::verifier::ObservationTelemetry;

/// Contextual payload passed to the agent planner upon human takeover resumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TakeoverResumeContext {
    pub intervention_summary: Option<String>,
    pub human_modified_dom: bool,
    pub resume_observation: Option<ObservationTelemetry>,
    pub timestamp_utc: String,
}

/// Human takeover controller mediating manual browser intervention.
#[derive(Debug, Clone, Default)]
pub struct HumanTakeoverManager {
    is_takeover_active: Arc<RwLock<bool>>,
    resume_context: Arc<RwLock<Option<TakeoverResumeContext>>>,
    admission_gate: Option<DispatchAdmissionGate>,
}

impl HumanTakeoverManager {
    pub fn new() -> Self {
        Self {
            is_takeover_active: Arc::new(RwLock::new(false)),
            resume_context: Arc::new(RwLock::new(None)),
            admission_gate: None,
        }
    }

    /// Bind an explicit shared admission gate to lock the execution boundary.
    pub fn with_admission_gate(mut self, gate: DispatchAdmissionGate) -> Self {
        self.admission_gate = Some(gate);
        self
    }

    /// Check if human takeover is currently active.
    pub async fn is_active(&self) -> bool {
        *self.is_takeover_active.read().await
    }

    /// User signals takeover ("Take Control" / `Esc`).
    ///
    /// # Concurrency Contract
    /// - **Takeover closes future autonomous admission**: `admission_gate.enter_takeover()` locks
    ///   out any new autonomous agent dispatches immediately (`ToolError::HumanTakeoverActive`).
    /// - **Pre-admitted permits**: Any execution admitted before takeover is allowed to settle or
    ///   cancel according to tool semantics before user takes full manual input focus.
    /// - Transitions task to [`TaskState::HumanTakeover`] and yields native child HWND surface to user.
    pub async fn initiate_takeover(&self, task: &mut AgentTask) {
        let mut active = self.is_takeover_active.write().await;
        *active = true;

        if let Some(ref gate) = self.admission_gate {
            gate.enter_takeover();
        }

        task.transition(TaskState::HumanTakeover);
    }

    /// User signals resumption of autonomous control.
    ///
    /// Resumes the admission gate, captures fresh observation telemetry,
    /// stores the resume context, and transitions the task back to [`TaskState::Executing`].
    pub async fn resume_autonomous(
        &self,
        task: &mut AgentTask,
        summary: Option<String>,
        fresh_telemetry: Option<ObservationTelemetry>,
    ) -> TakeoverResumeContext {
        let mut active = self.is_takeover_active.write().await;
        *active = false;

        if let Some(ref gate) = self.admission_gate {
            gate.resume_open();
        }

        let ctx = TakeoverResumeContext {
            intervention_summary: summary,
            human_modified_dom: true,
            resume_observation: fresh_telemetry,
            timestamp_utc: chrono::Utc::now().to_rfc3339(),
        };

        let mut stored = self.resume_context.write().await;
        *stored = Some(ctx.clone());

        task.transition(TaskState::Executing);
        ctx
    }

    /// Consume any pending takeover resume context to inject into the model prompt.
    pub async fn take_resume_context(&self) -> Option<TakeoverResumeContext> {
        let mut stored = self.resume_context.write().await;
        stored.take()
    }
}
