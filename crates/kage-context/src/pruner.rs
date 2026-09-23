//! 4-Stage DOM Pruning Pipeline (KAGE-CTX-002).
//!
//! Converts raw, large DOM subtrees into concise, high-signal, token-budgeted
//! representations for model reasoning:
//!
//! - Stage 1: Active Element Focus & Subtree Selection
//! - Stage 2: Structural Stripping (<script>, <style>, <svg> paths, base64)
//! - Stage 3: Accessibility Projection & Developer Attributes (id, class, aria-*, role)
//! - Stage 4: Repetition Collapsing (> 2 repeating sibling items collapsed)

use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use crate::telemetry::DomTreeStore;

/// Configuration options for the DOM pruner.
#[derive(Debug, Clone)]
pub struct DomPrunerConfig {
    /// Maximum depth from the root to traverse.
    pub max_depth: usize,
    /// Threshold of repeating sibling tags before collapsing (default 2).
    pub repetition_threshold: usize,
    /// Whether to strip inline scripts/styles (always true in production).
    pub strip_scripts_and_styles: bool,
    /// Retained attribute whitelist for Stage 3.
    pub allowed_attributes: HashSet<String>,
}

impl Default for DomPrunerConfig {
    fn default() -> Self {
        let mut allowed = HashSet::new();
        for attr in &[
            "id",
            "class",
            "name",
            "role",
            "type",
            "href",
            "placeholder",
            "value",
            "title",
            "data-testid",
            "data-cy",
            "aria-label",
            "aria-hidden",
            "aria-expanded",
            "aria-selected",
            "aria-checked",
            "aria-disabled",
        ] {
            allowed.insert(attr.to_string());
        }

        DomPrunerConfig {
            max_depth: 8,
            repetition_threshold: 2,
            strip_scripts_and_styles: true,
            allowed_attributes: allowed,
        }
    }
}

/// Stage-by-stage pruning metrics and verifiable report for audit gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomPruningReport {
    pub raw_node_count: usize,
    pub stage1_focused_node: Option<i64>,
    pub stage2_stripped_tags: usize,
    pub stage3_retained_attributes: usize,
    pub stage4_collapsed_siblings: usize,
    pub raw_estimated_tokens: usize,
    pub pruned_tokens: usize,
    pub pruned_dom: String,
}

#[derive(Default)]
struct PruningStats {
    stage2_stripped: usize,
    stage3_attrs: usize,
    stage4_collapsed: usize,
}

/// 4-Stage DOM Pruner.
pub struct DomPruner {
    config: DomPrunerConfig,
}

impl DomPruner {
    pub fn new(config: DomPrunerConfig) -> Self {
        DomPruner { config }
    }

    pub fn default_pruner() -> Self {
        Self::new(DomPrunerConfig::default())
    }

    /// Prune the given DOM tree into a concise XML/HTML representation.
    pub fn prune(&self, store: &DomTreeStore, focus_node_id: Option<i64>) -> String {
        self.prune_with_report(store, focus_node_id).pruned_dom
    }

    /// Prune the given DOM tree and return full stage-by-stage audit metrics.
    pub fn prune_with_report(&self, store: &DomTreeStore, focus_node_id: Option<i64>) -> DomPruningReport {
        let root_id = match store.root_id() {
            Some(id) => id,
            None => {
                return DomPruningReport {
                    raw_node_count: 0,
                    stage1_focused_node: focus_node_id,
                    stage2_stripped_tags: 0,
                    stage3_retained_attributes: 0,
                    stage4_collapsed_siblings: 0,
                    raw_estimated_tokens: 0,
                    pruned_tokens: 0,
                    pruned_dom: "<empty_dom />".to_string(),
                };
            }
        };

        let mut stats = PruningStats::default();
        let mut output = String::new();
        self.render_node(store, root_id, 0, focus_node_id, &mut output, &mut stats);

        let raw_node_count = store.node_count();
        let raw_estimated_tokens = raw_node_count * 15;
        let pruned_tokens = crate::budget::TokenBudget::estimate_tokens(&output);

        DomPruningReport {
            raw_node_count,
            stage1_focused_node: focus_node_id,
            stage2_stripped_tags: stats.stage2_stripped,
            stage3_retained_attributes: stats.stage3_attrs,
            stage4_collapsed_siblings: stats.stage4_collapsed,
            raw_estimated_tokens,
            pruned_tokens,
            pruned_dom: output,
        }
    }

    fn render_node(
        &self,
        store: &DomTreeStore,
        node_id: i64,
        depth: usize,
        focus_node_id: Option<i64>,
        out: &mut String,
        stats: &mut PruningStats,
    ) {
        if depth > self.config.max_depth {
            out.push_str("<!-- [max depth exceeded] -->\n");
            return;
        }

        let node = match store.get_node(node_id) {
            Some(n) => n,
            None => return,
        };

        let tag = node.local_name.to_lowercase();

        // Stage 2: Structural Stripping (<script>, <style>, <noscript>, <iframe>)
        if self.config.strip_scripts_and_styles {
            if tag == "script" || tag == "style" || tag == "noscript" || tag == "iframe" {
                stats.stage2_stripped += 1;
                return;
            }
        }

        // SVG path replacement (Stage 2)
        if tag == "svg" {
            stats.stage2_stripped += 1;
            let id_str = node
                .attributes
                .get("id")
                .map(|id| format!(" id=\"{id}\""))
                .unwrap_or_default();
            let class_str = node
                .attributes
                .get("class")
                .map(|c| format!(" class=\"{c}\""))
                .unwrap_or_default();
            out.push_str(&format!("{indent}<svg{id_str}{class_str} [icon]/>\n", indent = "  ".repeat(depth)));
            return;
        }

        // If text node
        if node.node_type == 3 {
            let text = node.node_value.trim();
            if !text.is_empty() {
                let clean_text: std::borrow::Cow<'_, str> = if text.starts_with("data:image/") {
                    stats.stage2_stripped += 1;
                    std::borrow::Cow::Borrowed("[base64_image]")
                } else if text.len() > 256 {
                    std::borrow::Cow::Owned(format!("{}... [truncated]", &text[..256]))
                } else {
                    std::borrow::Cow::Borrowed(text)
                };
                out.push_str(&format!("{indent}{clean_text}\n", indent = "  ".repeat(depth)));
            }
            return;
        }

        // Element node (node_type == 1) or Document (node_type == 9)
        if node.node_type == 9 {
            for &child_id in &node.children {
                self.render_node(store, child_id, depth, focus_node_id, out, stats);
            }
            return;
        }

        if node.node_type != 1 {
            return;
        }

        // Stage 1: Active Element Focus
        let is_focused = focus_node_id == Some(node_id);
        let indent = "  ".repeat(depth);

        // Stage 3: Accessibility & Developer Attributes Filtering
        let mut attrs_str = String::new();
        if is_focused {
            attrs_str.push_str(" data-kage-focused=\"true\"");
        }

        for (k, v) in &node.attributes {
            if self.config.allowed_attributes.contains(k) || k.starts_with("aria-") || k.starts_with("data-") {
                stats.stage3_attrs += 1;
                let val: std::borrow::Cow<'_, str> = if v.starts_with("data:image/") {
                    stats.stage2_stripped += 1;
                    std::borrow::Cow::Borrowed("[base64_image]")
                } else if v.len() > 120 {
                    std::borrow::Cow::Owned(format!("{}...", &v[..120]))
                } else {
                    std::borrow::Cow::Borrowed(v.as_str())
                };
                attrs_str.push_str(&format!(" {k}=\"{val}\""));
            }
        }

        if node.children.is_empty() {
            out.push_str(&format!("{indent}<{tag}{attrs_str} />\n"));
            return;
        }

        out.push_str(&format!("{indent}<{tag}{attrs_str}>\n"));

        // Stage 4: Repetition Collapsing of immediate children
        self.render_children_with_repetition_collapsing(store, &node.children, depth + 1, focus_node_id, out, stats);

        out.push_str(&format!("{indent}</{tag}>\n"));
    }

    /// Renders children while collapsing contiguous identical sibling tags beyond threshold.
    fn render_children_with_repetition_collapsing(
        &self,
        store: &DomTreeStore,
        children: &[i64],
        depth: usize,
        focus_node_id: Option<i64>,
        out: &mut String,
        stats: &mut PruningStats,
    ) {
        let mut i = 0;
        let indent = "  ".repeat(depth);

        while i < children.len() {
            let child_id = children[i];
            let tag = store
                .get_node(child_id)
                .map(|n| n.local_name.to_lowercase())
                .unwrap_or_default();

            let mut run_len = 1;
            while i + run_len < children.len() {
                let next_tag = store
                    .get_node(children[i + run_len])
                    .map(|n| n.local_name.to_lowercase())
                    .unwrap_or_default();
                if next_tag == tag && !tag.is_empty() && tag != "div" && tag != "span" {
                    run_len += 1;
                } else {
                    break;
                }
            }

            if run_len > self.config.repetition_threshold && (tag == "tr" || tag == "li" || tag == "option" || tag == "p") {
                // Render first few up to threshold
                for k in 0..self.config.repetition_threshold {
                    self.render_node(store, children[i + k], depth, focus_node_id, out, stats);
                }
                let omitted = run_len - self.config.repetition_threshold;
                stats.stage4_collapsed += omitted;
                out.push_str(&format!("{indent}<!-- [... {omitted} similar <{tag}> items collapsed ...] -->\n"));
                i += run_len;
            } else {
                self.render_node(store, child_id, depth, focus_node_id, out, stats);
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_stage2_script_and_svg_stripping() {
        let mut store = DomTreeStore::new();
        let doc = json!({
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
                            "nodeName": "SCRIPT",
                            "localName": "script",
                            "children": [
                                { "nodeId": 4, "nodeType": 3, "nodeName": "#text", "nodeValue": "console.log('secret');" }
                            ]
                        },
                        {
                            "nodeId": 5,
                            "nodeType": 1,
                            "nodeName": "SVG",
                            "localName": "svg",
                            "attributes": ["id", "icon-1", "class", "feather"],
                            "children": [
                                { "nodeId": 6, "nodeType": 1, "nodeName": "PATH", "localName": "path", "attributes": ["d", "M10 20..."] }
                            ]
                        }
                    ]
                }
            ]
        });

        store.set_document(&doc);
        let pruner = DomPruner::default_pruner();
        let report = pruner.prune_with_report(&store, None);

        assert!(report.stage2_stripped_tags >= 2);
        assert!(!report.pruned_dom.contains("script"));
        assert!(!report.pruned_dom.contains("console.log"));
        assert!(report.pruned_dom.contains("<svg id=\"icon-1\" class=\"feather\" [icon]/>"));
        assert!(!report.pruned_dom.contains("M10 20..."));
    }

    #[test]
    fn test_stage4_repetition_collapsing() {
        let mut store = DomTreeStore::new();
        let mut list_items = Vec::new();
        for i in 10..=30 {
            list_items.push(json!({
                "nodeId": i,
                "nodeType": 1,
                "nodeName": "LI",
                "localName": "li",
                "children": [
                    { "nodeId": i + 100, "nodeType": 3, "nodeName": "#text", "nodeValue": format!("Item {i}") }
                ]
            }));
        }

        let doc = json!({
            "nodeId": 1,
            "nodeType": 9,
            "nodeName": "#document",
            "children": [
                {
                    "nodeId": 2,
                    "nodeType": 1,
                    "nodeName": "UL",
                    "localName": "ul",
                    "children": list_items
                }
            ]
        });

        store.set_document(&doc);
        let pruner = DomPruner::default_pruner();
        let report = pruner.prune_with_report(&store, None);

        assert_eq!(report.stage4_collapsed_siblings, 19);
        assert!(report.pruned_dom.contains("Item 10"));
        assert!(report.pruned_dom.contains("Item 11"));
        assert!(!report.pruned_dom.contains("Item 25"));
        assert!(report.pruned_dom.contains("<!-- [... 19 similar <li> items collapsed ...] -->"));
    }
}
