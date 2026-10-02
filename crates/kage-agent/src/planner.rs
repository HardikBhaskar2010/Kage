//! Autonomous reasoning loop, bounded planner state machine, and orchestration engine.
//!
//! Enforces:
//! - **Registry-Driven Planning**: Tools discovered solely through [`ToolCatalog`]. Zero hardcoded names.
//! - **Bounded Reasoning**: Hard step limit (`max_steps`, default 10) prevents runaway cycles.
//! - **Fail-Safe STOP (INV-09)**: Cancellation halts the loop immediately and prevents new steps.
//! - **Model Authority Invariant**: The model never executes tools directly. It proposes actions
//!   which are validated against the catalog and executed via [`StepExecutor`].

use std::sync::Arc;
use thiserror::Error;

use crate::cancellation::AgentCancellation;
use crate::context::ContextAssembler;
use crate::execution::{ExecutionError, StepExecutor};
use crate::model::{AgentModel, ModelError, ModelRequest};
use crate::plan::{Plan, PlanStep, StepStatus};
use crate::prompt_boundary::WebProvenance;
use crate::task::{AgentTask, TaskState};
use crate::tool_catalog::ToolCatalog;

/// Errors arising during planner orchestration.
#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("Agent execution stopped by user or system")]
    Stopped,
    #[error("Exceeded maximum step budget of {0}")]
    StepBudgetExceeded(usize),
    #[error("Model reasoning failure: {0}")]
    Model(#[from] ModelError),
    #[error("Step execution failure: {0}")]
    Execution(#[from] ExecutionError),
    #[error("Proposed tool '{0}' is not present in the dynamic capability catalog")]
    ToolNotFoundInCatalog(String),
}

/// Bounded autonomous reasoning agent planner.
pub struct AgentPlanner {
    model: Arc<dyn AgentModel>,
    tool_catalog: ToolCatalog,
    executor: StepExecutor,
    assembler: ContextAssembler,
    cancellation: AgentCancellation,
    max_steps: usize,
}

impl AgentPlanner {
    pub fn new(
        model: Arc<dyn AgentModel>,
        tool_catalog: ToolCatalog,
        executor: StepExecutor,
        cancellation: AgentCancellation,
    ) -> Self {
        Self {
            model,
            tool_catalog,
            executor,
            assembler: ContextAssembler::default(),
            cancellation,
            max_steps: 10,
        }
    }

    /// Override the maximum step budget (default 10).
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Override context assembler with custom budgeting parameters.
    pub fn with_context_assembler(mut self, assembler: ContextAssembler) -> Self {
        self.assembler = assembler;
        self
    }

    /// Execute the full autonomous planning and action cycle for an [`AgentTask`].
    pub async fn run_task(
        &self,
        task: &mut AgentTask,
        initial_observation: Option<(&str, &WebProvenance)>,
    ) -> Result<Plan, PlannerError> {
        task.transition(TaskState::Planning);
        let mut plan = Plan::new(task.task_id);
        let mut active_observation = initial_observation;

        while plan.steps.len() < self.max_steps {
            // 1. Check for user STOP signal (INV-09)
            if self.cancellation.is_stopped() {
                task.transition(TaskState::Stopped);
                return Err(PlannerError::Stopped);
            }

            // 2. Assemble sectioned context pack
            let context_pack = self.assembler.assemble(
                task,
                &self.tool_catalog,
                active_observation,
                &plan.steps,
            );

            // 3. Request next action proposal from model
            let model_req = ModelRequest {
                messages: context_pack.messages,
                available_tools: self.tool_catalog.list(),
                temperature: 0.0,
                max_tokens: Some(1024),
            };

            let model_resp = self
                .model
                .request(model_req, self.cancellation.child_token())
                .await?;

            // 4. Inspect model response: tool call vs final text
            if model_resp.message.tool_calls.is_empty() {
                // No tools proposed -> goal complete
                task.transition(TaskState::Completed);
                plan.completed = true;
                return Ok(plan);
            }

            // 5. Evaluate the proposed tool call
            task.transition(TaskState::Executing);
            let proposed = &model_resp.message.tool_calls[0];

            // Validate proposed tool against dynamic catalog (INV-02)
            if !self.tool_catalog.contains(&proposed.tool_name) {
                let err_msg = format!("Tool '{}' not in CapabilityRegistry", proposed.tool_name);
                let step_idx = plan.steps.len();
                let mut step = PlanStep::new(
                    task.task_id,
                    step_idx,
                    &proposed.tool_name,
                    proposed.arguments.clone(),
                    "Attempt invalid tool execution",
                );
                step.fail(err_msg);
                step.status = StepStatus::Denied;
                plan.steps.push(step);
                task.transition(TaskState::Failed);
                return Err(PlannerError::ToolNotFoundInCatalog(proposed.tool_name.clone()));
            }

            // 6. Record step in plan
            let step_idx = plan.steps.len();
            let mut step = PlanStep::new(
                task.task_id,
                step_idx,
                &proposed.tool_name,
                proposed.arguments.clone(),
                format!("Execute {}", proposed.tool_name),
            );

            // 7. Execute step via governed ToolBus
            let exec_result = self
                .executor
                .execute_step(task, &mut step, self.cancellation.child_token())
                .await;

            plan.steps.push(step);

            match exec_result {
                Ok(_response) => {
                    // Step completed; proceed to next reasoning cycle
                    active_observation = None; // Reset until fresh observation provided
                }
                Err(ExecutionError::Cancelled) => {
                    task.transition(TaskState::Stopped);
                    return Err(PlannerError::Stopped);
                }
                Err(ExecutionError::PermissionDenied { .. }) => {
                    // Blocked by policy; pause for human intervention or terminate
                    task.transition(TaskState::AwaitingApproval);
                    return Ok(plan);
                }
                Err(_err) => {
                    // Structured tool failure recorded; allow model to see failure in next cycle
                    active_observation = None;
                }
            }
        }

        // Exceeded bounded step limit
        task.transition(TaskState::Failed);
        Err(PlannerError::StepBudgetExceeded(self.max_steps))
    }
}
