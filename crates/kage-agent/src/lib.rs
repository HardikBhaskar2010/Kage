//! `kage-agent` — Autonomous Agent Runtime, Registry-Driven Planner, and Model Abstraction.
//!
//! # Architecture Invariants
//!
//! - **INV-01**: Zero direct CEF / CDP manipulation. All browser mutations route via [`ToolBus`].
//! - **INV-02**: All actions pass through the governed capability system.
//! - **INV-03**: Web content is treated as untrusted data, never authority.
//! - **INV-06**: Secrets are redacted before entering the model context.
//! - **INV-09**: User STOP cancels active operations and prevents subsequent actions.

pub mod cancellation;
pub mod context;
pub mod execution;
pub mod model;
pub mod plan;
pub mod planner;
pub mod prompt_boundary;
pub mod task;
pub mod tool_catalog;

// Re-exports
pub use cancellation::AgentCancellation;
pub use context::{ContextAssembler, ContextBudget, ContextPack};
pub use execution::{ExecutionError, StepExecutor};
pub use model::{
    AgentModel, ChatMessage, ChatRole, MockAgentModel, ModelCapabilities, ModelError, ModelRequest,
    ModelResponse, ProposedToolCall, TokenUsage,
};
pub use plan::{ActionLineage, Plan, PlanStep, StepStatus};
pub use planner::{AgentPlanner, PlannerError};
pub use prompt_boundary::{PromptBoundary, WebProvenance};
pub use task::{AgentTask, TaskState};
pub use tool_catalog::{ToolCatalog, ToolDefinition};
