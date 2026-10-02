//! Immutable action lineage and plan representation.
//!
//! Enforces unified cross-plane lineage:
//! ```text
//! AgentTaskId (Plan Scope)
//!      ↓
//! PlanStepId (Step Scope)
//!      ↓
//! ToolRequestId (Caller Request)
//!      ↓
//! ToolExecutionId (Bus Execution Instance)
//!      ↓
//! AuditRecordId (Immutable Ledger Commit)
//!      ↓
//! VerificationId (Deterministic Postcondition Check)
//! ```

use chrono::{DateTime, Utc};
use kage_core::lineage::{AgentTaskId, PlanStepId};
use kage_core::tool::ToolResponse;
use serde::{Deserialize, Serialize};

/// Execution status of a discrete plan step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Executing,
    Success,
    Failed,
    Skipped,
    Cancelled,
    Denied,
}

/// Comprehensive cross-plane action lineage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLineage {
    pub task_id: AgentTaskId,
    pub step_id: PlanStepId,
    pub tool_request_id: Option<String>,
    pub tool_execution_id: Option<String>,
    pub audit_record_id: Option<u64>,
}

impl ActionLineage {
    pub fn new(task_id: AgentTaskId, step_id: PlanStepId) -> Self {
        Self {
            task_id,
            step_id,
            tool_request_id: None,
            tool_execution_id: None,
            audit_record_id: None,
        }
    }
}

/// Discrete action step in an autonomous plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Unique immutable step identifier.
    pub step_id: PlanStepId,
    /// Parent task scope.
    pub task_id: AgentTaskId,
    /// Sequential index within the plan.
    pub step_index: usize,
    /// Canonical governed tool identifier (e.g. `page.click`).
    pub tool_id: String,
    /// Validated input arguments matching tool schema.
    pub arguments: serde_json::Value,
    /// High-level expected effect or postcondition requirement (consumed in M10).
    pub expected_effect: String,
    /// Current step status.
    pub status: StepStatus,
    /// Immutable lineage linkage.
    pub lineage: ActionLineage,
    /// Observed tool response after execution.
    pub observed_result: Option<ToolResponse>,
    /// Step creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Step completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
}

impl PlanStep {
    pub fn new(
        task_id: AgentTaskId,
        step_index: usize,
        tool_id: impl Into<String>,
        arguments: serde_json::Value,
        expected_effect: impl Into<String>,
    ) -> Self {
        let step_id = PlanStepId::new();
        Self {
            step_id,
            task_id,
            step_index,
            tool_id: tool_id.into(),
            arguments,
            expected_effect: expected_effect.into(),
            status: StepStatus::Pending,
            lineage: ActionLineage::new(task_id, step_id),
            observed_result: None,
            created_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Record successful completion of the step with result and lineage IDs.
    pub fn complete_with_result(&mut self, response: ToolResponse) {
        self.status = StepStatus::Success;
        self.lineage.tool_request_id = Some(response.request_id.clone());
        self.lineage.tool_execution_id = Some(response.execution_id.clone());
        self.observed_result = Some(response);
        self.completed_at = Some(Utc::now());
    }

    /// Record step failure.
    pub fn fail(&mut self, reason: String) {
        self.status = StepStatus::Failed;
        self.completed_at = Some(Utc::now());
        if let Some(res) = &mut self.observed_result {
            res.failure = Some(reason);
        }
    }
}

/// Dynamic plan consisting of sequenced or branched plan steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub plan_id: String,
    pub task_id: AgentTaskId,
    pub steps: Vec<PlanStep>,
    pub current_step_index: usize,
    pub completed: bool,
}

impl Plan {
    pub fn new(task_id: AgentTaskId) -> Self {
        Self {
            plan_id: format!("plan_{}", uuid::Uuid::new_v4()),
            task_id,
            steps: Vec::new(),
            current_step_index: 0,
            completed: false,
        }
    }

    /// Append a new step to the plan.
    pub fn add_step(&mut self, tool_id: impl Into<String>, arguments: serde_json::Value, expected_effect: impl Into<String>) -> &PlanStep {
        let index = self.steps.len();
        let step = PlanStep::new(self.task_id, index, tool_id, arguments, expected_effect);
        self.steps.push(step);
        self.steps.last().unwrap()
    }

    /// Return the currently active plan step, if any.
    pub fn current_step(&self) -> Option<&PlanStep> {
        self.steps.get(self.current_step_index)
    }

    /// Return mutable reference to the currently active step.
    pub fn current_step_mut(&mut self) -> Option<&mut PlanStep> {
        self.steps.get_mut(self.current_step_index)
    }

    /// Advance to the next plan step.
    pub fn advance(&mut self) {
        self.current_step_index += 1;
        if self.current_step_index >= self.steps.len() {
            self.completed = true;
        }
    }
}
