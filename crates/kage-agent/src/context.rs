//! Sectioned context budget engine and prompt pack assembler.
//!
//! Formats input context into strict structural tiers:
//! ```text
//! 1. CONTROL / SYSTEM RULES (Authoritative)
//! 2. USER GOAL
//! 3. AVAILABLE GOVERNED TOOLS
//! 4. CURRENT BROWSER OBSERVATION (Untrusted Web Content Envelopes)
//! 5. RECENT ACTIONS & OBSERVED RESULTS
//! 6. RECOVERY / ERROR SIGNALS
//! ```

use crate::model::ChatMessage;
use crate::plan::PlanStep;
use crate::prompt_boundary::{PromptBoundary, WebProvenance};
use crate::task::AgentTask;
use crate::tool_catalog::ToolCatalog;
use serde::{Deserialize, Serialize};

/// Token allocation envelope across distinct context sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Total maximum token ceiling across all sections.
    pub total_ceiling: usize,
    /// Maximum tokens reserved for system instructions and authority boundaries.
    pub system_budget: usize,
    /// Maximum tokens reserved for the user goal.
    pub goal_budget: usize,
    /// Maximum tokens reserved for tool schema descriptions.
    pub tools_budget: usize,
    /// Maximum tokens reserved for recent action history and results.
    pub history_budget: usize,
    /// Maximum tokens reserved for active browser observation data.
    pub observation_budget: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            total_ceiling: 4_000,
            system_budget: 500,
            goal_budget: 300,
            tools_budget: 1_200,
            history_budget: 800,
            observation_budget: 1_200,
        }
    }
}

/// Assembled context prompt pack ready for model consumption.
#[derive(Debug, Clone)]
pub struct ContextPack {
    pub messages: Vec<ChatMessage>,
    pub estimated_tokens: usize,
}

/// Context assembly engine enforcing strict token budgeting and prompt boundaries.
#[derive(Debug, Clone)]
pub struct ContextAssembler {
    boundary: PromptBoundary,
    budget: ContextBudget,
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new(ContextBudget::default())
    }
}

impl ContextAssembler {
    pub fn new(budget: ContextBudget) -> Self {
        Self {
            boundary: PromptBoundary::new(),
            budget,
        }
    }

    /// Assemble full context messages for the next planning step.
    pub fn assemble(
        &self,
        task: &AgentTask,
        _catalog: &ToolCatalog,
        raw_observation: Option<(&str, &WebProvenance)>,
        recent_steps: &[PlanStep],
    ) -> ContextPack {
        let mut messages = Vec::new();

        // 1. SYSTEM CONTROL SECTION
        let system_text = format!(
            "{}\n\n\
            ROLE & MISSION:\n\
            You are KAGE Autonomous Browser Agent executing a plan for profile '{}'.\n\
            You reason step-by-step and propose actions by calling available tools.\n\
            When the user's goal is achieved or no further action is needed, respond with text explaining the result.",
            PromptBoundary::system_instruction_preamble(),
            task.profile_id
        );
        messages.push(ChatMessage::system(system_text));

        // 2. USER GOAL
        messages.push(ChatMessage::user(format!("USER GOAL:\n{}", task.goal)));

        // 3. RECENT ACTION HISTORY
        if !recent_steps.is_empty() {
            let mut history_str = String::from("RECENT ACTIONS TAKEN:\n");
            for s in recent_steps.iter().rev().take(5).rev() {
                history_str.push_str(&format!(
                    "- Step #{}: tool='{}', status={:?}\n",
                    s.step_index, s.tool_id, s.status
                ));
                if let Some(res) = &s.observed_result {
                    if let Some(effect) = &res.effect_summary {
                        history_str.push_str(&format!("  Effect: {}\n", effect));
                    }
                    if let Some(failure) = &res.failure {
                        history_str.push_str(&format!("  Error: {}\n", failure));
                    }
                }
            }
            messages.push(ChatMessage::user(history_str));
        }

        // 4. BROWSER OBSERVATION (ENCLOSED IN UNTRUSTED BOUNDARY)
        if let Some((content, provenance)) = raw_observation {
            // Truncate raw observation if it exceeds budgeted character limit
            let max_chars = self.budget.observation_budget * 4; // ~4 chars per token
            let truncated = if content.len() > max_chars {
                &content[..max_chars]
            } else {
                content
            };
            let wrapped = self.boundary.wrap_untrusted_content(truncated, provenance);
            messages.push(ChatMessage::user(format!(
                "CURRENT BROWSER OBSERVATION:\n{}",
                wrapped
            )));
        }

        // Token estimation (~4 characters per token heuristic)
        let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        let estimated_tokens = total_chars / 4;

        ContextPack {
            messages,
            estimated_tokens,
        }
    }
}
