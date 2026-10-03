//! Autonomous agent task lifecycle and immutable identity representation.

use chrono::{DateTime, Utc};
use kage_core::lineage::AgentTaskId;
use serde::{Deserialize, Serialize};

/// High-level lifecycle state for an autonomous agent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Task has been instantiated but planning has not started.
    Created,
    /// Agent is currently observing context and reasoning about a plan.
    Planning,
    /// Agent is executing plan steps via the governed ToolBus.
    Executing,
    /// Agent is paused awaiting user confirmation or policy escalation.
    AwaitingApproval,
    /// Task has been paused for manual human takeover (Escape Hatch).
    HumanTakeover,
    /// Task has paused awaiting user guidance on a non-idempotent action or unknown outcome.
    AwaitingGuidance,
    /// Task successfully achieved its completion condition.
    Completed,
    /// Task was cancelled / stopped by the user or system.
    Stopped,
    /// Task encountered an unrecoverable failure.
    Failed,
}

/// Fully-qualified agent task scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// Unique immutable task identity.
    pub task_id: AgentTaskId,
    /// Human-described user goal.
    pub goal: String,
    /// Profile under which the agent executes (default: agent_sandbox).
    pub profile_id: String,
    /// Active browser tab targeted by the task, if known.
    pub starting_tab_id: Option<String>,
    /// Current execution state.
    pub state: TaskState,
    /// Whether session-scoped mutation permissions have been granted to this task.
    pub session_granted: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last state update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Optional arbitrary metadata (e.g. workspace_id, session_id).
    pub metadata: serde_json::Value,
}

impl AgentTask {
    /// Instantiate a new autonomous agent task with a fresh [`AgentTaskId`].
    pub fn new(goal: impl Into<String>, profile_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            task_id: AgentTaskId::new(),
            goal: goal.into(),
            profile_id: profile_id.into(),
            starting_tab_id: None,
            session_granted: false,
            state: TaskState::Created,
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        }
    }

    /// Explicitly grant or revoke session-scoped mutation permissions for this task.
    pub fn with_session_grant(mut self, granted: bool) -> Self {
        self.session_granted = granted;
        self
    }

    /// Bind a starting tab to the task.
    pub fn with_tab(mut self, tab_id: impl Into<String>) -> Self {
        self.starting_tab_id = Some(tab_id.into());
        self
    }

    /// Attach metadata to the task.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Transition task state.
    pub fn transition(&mut self, next: TaskState) {
        self.state = next;
        self.updated_at = Utc::now();
    }
}
