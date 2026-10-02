//! Execution coordinator translating plan steps into governed ToolBus dispatches.
//!
//! Enforces:
//! - **INV-01**: Zero direct CEF / CDP manipulation. All browser mutations route via ToolBus.
//! - **INV-02**: All mutating & capability actions pass ToolBus with schema validation & policy.
//! - **Lineage Invariant**: Every dispatch carries `(AgentTaskId, PlanStepId)`.

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use thiserror::Error;

use kage_core::audit::ActorType;
use kage_core::bus::{PartialPolicyContext, ToolBus};
use kage_core::tool::{ToolError, ToolRequest, ToolResponse};

use crate::plan::PlanStep;
use crate::task::AgentTask;

/// Errors arising during step execution.
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Tool execution failed: {0}")]
    Tool(#[from] ToolError),
    #[error("Execution cancelled by user or system STOP")]
    Cancelled,
    #[error("Permission denied by policy engine: {decision}")]
    PermissionDenied { decision: String },
}

/// Execution engine mediating between the autonomous plan and the governed [`ToolBus`].
#[derive(Clone)]
pub struct StepExecutor {
    tool_bus: Arc<ToolBus>,
}

impl StepExecutor {
    pub fn new(tool_bus: Arc<ToolBus>) -> Self {
        Self { tool_bus }
    }

    /// Dispatch a single [`PlanStep`] through the governed [`ToolBus`].
    pub async fn execute_step(
        &self,
        task: &AgentTask,
        step: &mut PlanStep,
        cancel: CancellationToken,
    ) -> Result<ToolResponse, ExecutionError> {
        if cancel.is_cancelled() {
            step.fail("Execution cancelled before dispatch".to_string());
            return Err(ExecutionError::Cancelled);
        }

        step.status = crate::plan::StepStatus::Executing;

        // 1. Generate unique ToolRequestId linked to PlanStep
        let request_id = format!("req_{}", uuid::Uuid::new_v4());

        // 2. Build governed ToolRequest with explicit task and step lineage
        let tool_req = ToolRequest::new(
            &step.tool_id,
            step.arguments.clone(),
            request_id,
            &step.expected_effect,
        )
        .with_lineage(step.task_id.to_string(), step.step_id.to_string());

        // 3. Assemble PolicyContext declaring Agent actor
        let policy_ctx = PartialPolicyContext {
            caller_id: "agent_runtime".to_string(),
            session_id: format!("agent_sess_{}", task.task_id),
            workspace_id: "default_workspace".to_string(),
            session_granted: false, // Default to unprivileged sandbox
            actor: Some(ActorType::Agent),
            profile_id: Some(task.profile_id.clone()),
            tab_id: task.starting_tab_id.clone(),
            target_id: None,
            origin: None,
        };

        // 4. Dispatch via central ToolBus
        match self.tool_bus.dispatch(tool_req, policy_ctx, cancel).await {
            Ok(resp) => {
                step.complete_with_result(resp.clone());
                Ok(resp)
            }
            Err(ToolError::Cancelled { .. }) => {
                step.fail("Operation cancelled by user STOP".to_string());
                step.status = crate::plan::StepStatus::Cancelled;
                Err(ExecutionError::Cancelled)
            }
            Err(ToolError::PermissionDenied { decision, .. }) => {
                let err_msg = format!("Permission denied: {}", decision);
                step.fail(err_msg);
                step.status = crate::plan::StepStatus::Denied;
                Err(ExecutionError::PermissionDenied { decision })
            }
            Err(err) => {
                let err_msg = err.to_string();
                step.fail(err_msg);
                Err(ExecutionError::Tool(err))
            }
        }
    }
}
