//! Context pack assembler — builds the final LLM context snapshot from DOM, Console,
//! and Network signals, in priority order, within the dynamic token budget.
//!
//! # Priority order (KAGE-CTX-001)
//!
//! 1. Active focused node & pruned DOM structure (highest relevance)
//! 2. Recent console errors (stack traces & error messages)
//! 3. Failed or recent network transactions (status, url, payload preview)
//! 4. Untrusted XML framing (<untrusted_web_content> packaging)

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::budget::TokenBudget;
use crate::pruner::{DomPruner, DomPrunerConfig};
use crate::scrubber::ContextScrubber;
use crate::telemetry::{ConsoleEntry, ConsoleRingBuffer, DomTreeStore, NetworkEntry, NetworkRingBuffer};

// ---------------------------------------------------------------------------
// Constants for Dynamic Token Budgeting (KAGE-CTX-003 §4)
// ---------------------------------------------------------------------------

pub const BUDGET_MICRO_INSPECT: usize = 1_500;
pub const BUDGET_DEFAULT: usize = 4_000;
pub const BUDGET_ERROR_DIAGNOSIS: usize = 6_500;
pub const BUDGET_FULL_PAGE: usize = 16_000;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ContextPackError {
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Input signals (Legacy / manual)
// ---------------------------------------------------------------------------

/// Raw signals captured before context assembly.
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
// Output Context Envelopes
// ---------------------------------------------------------------------------

/// The assembled, budget-bounded context sent to the AI subsystem.
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

/// Structured observation model synthesized from live telemetry buffers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserObservation {
    pub origin: String,
    pub url: String,
    pub title: String,
    pub timestamp_ms: u64,
    pub dom_summary: String,
    pub console_errors: Vec<ConsoleEntry>,
    pub recent_network: Vec<NetworkEntry>,
    pub formatted_untrusted_content: String,
    pub tokens_used: usize,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Assembler
// ---------------------------------------------------------------------------

pub struct ContextPackAssembler {
    scrubber: ContextScrubber,
    pruner: DomPruner,
    budget_tokens: usize,
}

impl ContextPackAssembler {
    pub fn new(budget_tokens: usize) -> Self {
        ContextPackAssembler {
            scrubber: ContextScrubber::new(),
            pruner: DomPruner::default_pruner(),
            budget_tokens,
        }
    }

    pub fn with_pruner_config(mut self, config: DomPrunerConfig) -> Self {
        self.pruner = DomPruner::new(config);
        self
    }

    pub fn default_budget() -> Self {
        Self::new(BUDGET_DEFAULT)
    }

    pub fn micro_inspect_budget() -> Self {
        Self::new(BUDGET_MICRO_INSPECT)
    }

    pub fn error_diagnosis_budget() -> Self {
        Self::new(BUDGET_ERROR_DIAGNOSIS)
    }

    /// Assemble a high-fidelity [`BrowserObservation`] directly from live telemetry ring buffers.
    pub fn assemble_from_telemetry(
        &self,
        origin: &str,
        url: &str,
        title: &str,
        timestamp_ms: u64,
        dom_store: &DomTreeStore,
        focus_node_id: Option<i64>,
        console_buffer: &ConsoleRingBuffer,
        network_buffer: &NetworkRingBuffer,
    ) -> Result<BrowserObservation, ContextPackError> {
        let mut budget = TokenBudget::new(self.budget_tokens);
        let mut truncated = false;

        // 1. Prune DOM tree
        let raw_pruned_dom = self.pruner.prune(dom_store, focus_node_id);
        let dom_summary = if budget.try_consume(&raw_pruned_dom) {
            raw_pruned_dom
        } else {
            truncated = true;
            let fallback = "<dom_truncated />".to_string();
            budget.try_consume(&fallback);
            fallback
        };

        // 2. High-priority console errors
        let mut console_errors = Vec::new();
        let errors = console_buffer.errors();
        for err in errors {
            let serialized = serde_json::to_string(&err)?;
            if budget.try_consume(&serialized) {
                console_errors.push(err);
            } else {
                truncated = true;
                break;
            }
        }

        // 3. Failed and recent network transactions
        let mut recent_network = Vec::new();
        let failed = network_buffer.failed_requests();
        for req in failed {
            let serialized = serde_json::to_string(&req)?;
            if budget.try_consume(&serialized) {
                recent_network.push(req);
            } else {
                truncated = true;
                break;
            }
        }

        // 4. Construct unified <untrusted_web_content> XML envelope
        let mut xml_inner = String::new();
        xml_inner.push_str(&format!("<page_title>{}</page_title>\n", self.scrubber.scrub_str(title)));
        
        xml_inner.push_str("<dom_hierarchy>\n");
        xml_inner.push_str(&dom_summary);
        xml_inner.push_str("\n</dom_hierarchy>\n");

        if !console_errors.is_empty() {
            xml_inner.push_str(&format!("<recent_console_errors count=\"{}\">\n", console_errors.len()));
            for err in &console_errors {
                let count_str = if err.count > 1 { format!(" [x{}]", err.count) } else { String::new() };
                xml_inner.push_str(&format!("  [{}]{} {}\n", err.level.as_ref_str(), count_str, err.text));
            }
            xml_inner.push_str("</recent_console_errors>\n");
        }

        if !recent_network.is_empty() {
            xml_inner.push_str(&format!("<recent_failed_network count=\"{}\">\n", recent_network.len()));
            for req in &recent_network {
                let status = req.status_code.map(|s| s.to_string()).unwrap_or_else(|| "ERR".to_string());
                let dur = req.duration_ms.map(|d| format!(" ({d:.0}ms)")).unwrap_or_default();
                xml_inner.push_str(&format!("  [{} {}] {}{}\n", req.method, status, req.url, dur));
            }
            xml_inner.push_str("</recent_failed_network>\n");
        }

        let formatted_untrusted_content = self.scrubber.wrap_untrusted_content(
            origin,
            url,
            timestamp_ms,
            &xml_inner,
        );

        Ok(BrowserObservation {
            origin: origin.to_string(),
            url: url.to_string(),
            title: title.to_string(),
            timestamp_ms,
            dom_summary,
            console_errors,
            recent_network,
            formatted_untrusted_content,
            tokens_used: self.budget_tokens - budget.remaining(),
            truncated,
        })
    }

    /// Assemble a legacy [`ContextPack`] from the provided raw signals.
    pub fn assemble(&self, signals: ContextSignals) -> Result<ContextPack, ContextPackError> {
        let mut budget = TokenBudget::new(self.budget_tokens);
        let mut sections = Vec::new();
        let mut truncated = false;

        let mut try_add = |label: &str, value: serde_json::Value, sections: &mut Vec<ContextSection>, truncated: &mut bool| {
            let content = self.scrubber.scrub_and_wrap(value);
            if budget.try_consume(&content) {
                sections.push(ContextSection { label: label.to_string(), content });
            } else {
                *truncated = true;
            }
        };

        if let Some(node) = signals.focused_node {
            try_add("focused_node", node, &mut sections, &mut truncated);
        }

        for (i, el) in signals.interactive_elements.into_iter().enumerate() {
            try_add(&format!("interactive_element[{i}]"), el, &mut sections, &mut truncated);
        }

        for (i, msg) in signals.console_messages.into_iter().enumerate() {
            try_add(&format!("console_message[{i}]"), msg, &mut sections, &mut truncated);
        }

        for (i, node) in signals.visible_nodes.into_iter().enumerate() {
            try_add(&format!("visible_node[{i}]"), node, &mut sections, &mut truncated);
        }

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

impl ConsoleEntry {
    pub fn as_ref_str(&self) -> &str {
        match self.level {
            crate::telemetry::ConsoleLogLevel::Error => "ERROR",
            crate::telemetry::ConsoleLogLevel::Warning => "WARN",
            crate::telemetry::ConsoleLogLevel::Info => "INFO",
            crate::telemetry::ConsoleLogLevel::Verbose => "DEBUG",
        }
    }
}

impl crate::telemetry::ConsoleLogLevel {
    pub fn as_ref_str(&self) -> &str {
        match self {
            crate::telemetry::ConsoleLogLevel::Error => "ERROR",
            crate::telemetry::ConsoleLogLevel::Warning => "WARN",
            crate::telemetry::ConsoleLogLevel::Info => "INFO",
            crate::telemetry::ConsoleLogLevel::Verbose => "DEBUG",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::telemetry::ConsoleLogLevel;

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
    fn test_assemble_from_telemetry_buffers() {
        let assembler = ContextPackAssembler::default_budget();
        let mut dom_store = DomTreeStore::new();
        dom_store.set_document(&json!({
            "nodeId": 1,
            "nodeType": 9,
            "nodeName": "#document",
            "children": [
                {
                    "nodeId": 2,
                    "nodeType": 1,
                    "nodeName": "HTML",
                    "localName": "html",
                    "children": [
                        {
                            "nodeId": 3,
                            "nodeType": 1,
                            "nodeName": "BODY",
                            "localName": "body",
                            "children": [
                                {
                                    "nodeId": 4,
                                    "nodeType": 1,
                                    "nodeName": "BUTTON",
                                    "localName": "button",
                                    "attributes": ["id", "submit-btn", "class", "btn-primary"],
                                    "children": [
                                        { "nodeId": 5, "nodeType": 3, "nodeName": "#text", "nodeValue": "Pay Now" }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }));

        let mut console_buffer = ConsoleRingBuffer::new(10);
        console_buffer.push(ConsoleLogLevel::Error, "checkout", "Payment gateway 500", None, 100);

        let mut network_buffer = NetworkRingBuffer::new(10);
        network_buffer.on_request_will_be_sent("r1", "https://api.example.com/pay", "POST", 100);
        network_buffer.on_response_received("r1", 500, Some("application/json"), 180);

        let obs = assembler.assemble_from_telemetry(
            "https://example.com",
            "https://example.com/checkout",
            "Checkout Page",
            1000,
            &dom_store,
            Some(4),
            &console_buffer,
            &network_buffer,
        ).unwrap();

        assert_eq!(obs.origin, "https://example.com");
        assert_eq!(obs.url, "https://example.com/checkout");
        assert!(obs.formatted_untrusted_content.contains("<untrusted_web_content"));
        assert!(obs.formatted_untrusted_content.contains("data-kage-focused=\"true\""));
        assert!(obs.formatted_untrusted_content.contains("Payment gateway 500"));
        assert!(obs.formatted_untrusted_content.contains("POST 500"));
        assert!(!obs.truncated);
    }
}
