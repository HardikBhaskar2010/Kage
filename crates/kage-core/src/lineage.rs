//! Cross-plane identifier lineage and execution status.
//!
//! Enforces unified action correlation across all KAGE subsystems:
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

use std::fmt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an end-to-end agent task or goal plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentTaskId(pub Uuid);

impl AgentTaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentTaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for AgentTaskId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Unique identifier for a single plan step within an agent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanStepId(pub Uuid);

impl PlanStepId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlanStepId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlanStepId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for PlanStepId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Unique correlation ID for a caller's tool invocation request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolRequestId(pub String);

impl ToolRequestId {
    pub fn new() -> Self {
        Self(format!("req_{}", Uuid::new_v4().simple()))
    }
}

impl Default for ToolRequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ToolRequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ToolRequestId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ToolRequestId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Unique identifier for a specific execution attempt dispatched on the ToolBus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolExecutionId(pub Uuid);

impl ToolExecutionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ToolExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ToolExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ToolExecutionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Monotonic or content-hash identifier for an entry committed to the audit ledger.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditRecordId(pub String);

impl fmt::Display for AuditRecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AuditRecordId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AuditRecordId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<u64> for AuditRecordId {
    fn from(seq: u64) -> Self {
        Self(seq.to_string())
    }
}

/// Unique identifier for a deterministic verifier postcondition check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerificationId(pub Uuid);

impl VerificationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for VerificationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VerificationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for VerificationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Terminal or in-progress execution status for a tool dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failed,
    Cancelled,
    Denied,
}

impl Default for ExecutionStatus {
    fn default() -> Self {
        Self::Success
    }
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

/// Complete cross-plane execution lineage container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLineage {
    pub task_id: Option<AgentTaskId>,
    pub step_id: Option<PlanStepId>,
    pub request_id: ToolRequestId,
    pub execution_id: ToolExecutionId,
    #[serde(default)]
    pub audit_record_id: Option<AuditRecordId>,
    #[serde(default)]
    pub verification_id: Option<VerificationId>,
}

impl ExecutionLineage {
    pub fn new(request_id: impl Into<ToolRequestId>) -> Self {
        Self {
            task_id: None,
            step_id: None,
            request_id: request_id.into(),
            execution_id: ToolExecutionId::new(),
            audit_record_id: None,
            verification_id: None,
        }
    }

    pub fn with_task_and_step(
        mut self,
        task_id: impl Into<Option<AgentTaskId>>,
        step_id: impl Into<Option<PlanStepId>>,
    ) -> Self {
        self.task_id = task_id.into();
        self.step_id = step_id.into();
        self
    }

    pub fn with_audit_record(mut self, record_id: impl Into<AuditRecordId>) -> Self {
        self.audit_record_id = Some(record_id.into());
        self
    }

    pub fn with_verification(mut self, verif_id: impl Into<VerificationId>) -> Self {
        self.verification_id = Some(verif_id.into());
        self
    }
}
