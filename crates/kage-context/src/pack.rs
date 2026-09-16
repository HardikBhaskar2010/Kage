//! Context pack assembler — builds the final LLM context snapshot from DOM, Console,
//! and Network signals, in priority order, within the token budget.
//!
//! # Priority order (KAGE-CTX-001)
//!
//! 1. Active focused node (highest relevance)
//! 2. Interactive elements in viewport
//! 3. Recent console errors
//! 4. Viewport-visible DOM nodes
//! 5. Network summary (recent requests)

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::budget::TokenBudget;
use crate::scrubber::ContextScrubber;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ContextPackError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Input signals
// ---------------------------------------------------------------------------

/// Raw signals captured from CDP before context assembly.
#[derive(Debug, Clone, Default)]
pub struct ContextSignals {
    /// The focused DOM node (highest priority).
    pub focused_node: Option<serde_json::Value>,
    /// Interactive elements currently in the viewport.
    pub interactive_elements: Vec<serde_json::Value>,
    /// Recent console messages (errors first).
    pub console_messages: Vec<serde_json::Value>,
    /// General visible DOM nodes (lower priority).
    pub visible_nodes: Vec<serde_json::Value>,
    /// Recent network requests (URL + status).
    pub network_summary: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Context pack
// ---------------------------------------------------------------------------

/// The assembled, budget-bounded context sent to the AI subsystem.
///
/// All fields are already sanitized and wrapped in `<webpage_data>` delimiters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    /// Serialized sections in priority order.
    pub sections: Vec<ContextSection>,
    /// Tokens consumed out of the budget.
    pub tokens_used: usize,
    /// True if any signals were truncated due to budget.
    pub truncated: bool,
}

/// A single named section within a context pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSection {
    pub label: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Assembler
// ---------------------------------------------------------------------------

pub struct ContextPackAssembler {
    scrubber: ContextScrubber,
    budget_tokens: usize,
}

impl ContextPackAssembler {
    pub fn new(budget_tokens: usize) -> Self {
        ContextPackAssembler {
            scrubber: ContextScrubber::new(),
            budget_tokens,
        }
    }

    pub fn default_budget() -> Self {
        Self::new(crate::budget::DEFAULT_BUDGET)
    }

    /// Assemble a [`ContextPack`] from the provided signals.
    pub fn assemble(&self, signals: ContextSignals) -> Result<ContextPack, ContextPackError> {
        let mut budget = TokenBudget::new(self.budget_tokens);
        let mut sections = Vec::new();
        let mut truncated = false;

        // Helper: try to add a section.
        let mut try_add = |label: &str, value: serde_json::Value, sections: &mut Vec<ContextSection>, truncated: &mut bool| {
            let content = self.scrubber.scrub_and_wrap(value);
            if budget.try_consume(&content) {
                sections.push(ContextSection { label: label.to_string(), content });
            } else {
                *truncated = true;
            }
        };

        // 1. Focused node.
        if let Some(node) = signals.focused_node {
            try_add("focused_node", node, &mut sections, &mut truncated);
        }

        // 2. Interactive elements.
        for (i, el) in signals.interactive_elements.into_iter().enumerate() {
            try_add(&format!("interactive_element[{i}]"), el, &mut sections, &mut truncated);
        }

        // 3. Console messages (errors come in first by convention from caller).
        for (i, msg) in signals.console_messages.into_iter().enumerate() {
            try_add(&format!("console_message[{i}]"), msg, &mut sections, &mut truncated);
        }

        // 4. Visible DOM nodes.
        for (i, node) in signals.visible_nodes.into_iter().enumerate() {
            try_add(&format!("visible_node[{i}]"), node, &mut sections, &mut truncated);
        }

        // 5. Network summary.
        for (i, req) in signals.network_summary.into_iter().enumerate() {
            try_add(&format!("network_request[{i}]"), req, &mut sections, &mut truncated);
        }

        Ok(ContextPack {
            tokens_used: self.budget_tokens - budget.remaining(),
            sections,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assembles_context_within_budget() {
        let assembler = ContextPackAssembler::new(4_000);
        let signals = ContextSignals {
            focused_node: Some(json!({ "tag": "input", "id": "search" })),
            console_messages: vec![json!({ "level": "error", "text": "TypeError: null" })],
            ..Default::default()
        };
        let pack = assembler.assemble(signals).unwrap();
        assert!(!pack.sections.is_empty());
        assert!(pack.tokens_used > 0);
        assert!(!pack.truncated);
    }

    #[test]
    fn truncates_when_budget_exceeded() {
        // 10-token budget — far too small for real content.
        let assembler = ContextPackAssembler::new(10);
        let signals = ContextSignals {
            focused_node: Some(json!({ "tag": "div", "content": "A".repeat(500) })),
            visible_nodes: vec![json!({ "tag": "p" }); 20],
            ..Default::default()
        };
        let pack = assembler.assemble(signals).unwrap();
        assert!(pack.truncated, "Must mark pack as truncated when budget exceeded");
    }

    #[test]
    fn secrets_are_not_in_pack() {
        let assembler = ContextPackAssembler::new(4_000);
        let signals = ContextSignals {
            focused_node: Some(json!({ "value": "Bearer tok_secret_12345" })),
            ..Default::default()
        };
        let pack = assembler.assemble(signals).unwrap();
        let full = pack.sections.iter().map(|s| s.content.as_str()).collect::<Vec<_>>().join("");
        assert!(!full.contains("tok_secret_12345"), "Bearer token must be scrubbed from pack");
    }
}
