//! Bounded telemetry ring buffers and DOM tree storage for live CDP streams (Phase 5).
//!
//! Enforces:
//! - Bounded capacity: 75 console entries and 150 network transactions per tab.
//! - Consecutive error / log deduplication with `[xN]` counters.
//! - Secret sanitization on console strings and response previews.
//! - Binary media payload stripping (`image/*`, `font/*`, etc.).
//! - In-memory DOM cache with incremental mutation application.

use std::collections::{HashMap, VecDeque};
use kage_core::SecretSanitizer;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of console entries retained per tab buffer.
pub const CONSOLE_RING_CAPACITY: usize = 75;

/// Maximum number of network transactions retained per tab buffer.
pub const NETWORK_RING_CAPACITY: usize = 150;

/// Maximum length (in bytes) of network payload previews.
pub const MAX_PREVIEW_BYTES: usize = 512;

// ---------------------------------------------------------------------------
// Console Telemetry
// ---------------------------------------------------------------------------

/// Severity log levels for console messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLogLevel {
    Verbose,
    Info,
    Warning,
    Error,
}

impl ConsoleLogLevel {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "error" | "assert" => ConsoleLogLevel::Error,
            "warning" | "warn" => ConsoleLogLevel::Warning,
            "verbose" | "debug" | "trace" => ConsoleLogLevel::Verbose,
            _ => ConsoleLogLevel::Info,
        }
    }

    pub fn as_ref_str(&self) -> &'static str {
        match self {
            ConsoleLogLevel::Error => "ERROR",
            ConsoleLogLevel::Warning => "WARN",
            ConsoleLogLevel::Info => "INFO",
            ConsoleLogLevel::Verbose => "DEBUG",
        }
    }
}

/// A sanitized console log entry with recurring deduplication count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsoleEntry {
    pub id: u64,
    pub timestamp_ms: u64,
    pub level: ConsoleLogLevel,
    pub source: String,
    pub text: String,
    pub count: u32,
    pub stack_trace: Option<String>,
}

/// Bounded ring buffer for live console events with recurring deduplication.
pub struct ConsoleRingBuffer {
    capacity: usize,
    entries: VecDeque<ConsoleEntry>,
    next_id: u64,
    sanitizer: SecretSanitizer,
}

impl ConsoleRingBuffer {
    pub fn new(capacity: usize) -> Self {
        ConsoleRingBuffer {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            next_id: 1,
            sanitizer: SecretSanitizer::new(),
        }
    }

    /// Push a new console log. If it matches the most recent entry in level,
    /// source, and text, increments the deduplication counter instead of allocating a new entry.
    pub fn push(
        &mut self,
        level: ConsoleLogLevel,
        source: impl Into<String>,
        raw_text: &str,
        stack_trace: Option<String>,
        timestamp_ms: u64,
    ) {
        let source_str = source.into();
        let sanitized_text = self.sanitizer.sanitize_string(raw_text);
        let sanitized_stack = stack_trace.map(|st| self.sanitizer.sanitize_string(&st));

        // Deduplication check against the most recent entry
        if let Some(last) = self.entries.back_mut() {
            if last.level == level && last.source == source_str && last.text == sanitized_text {
                last.count = last.count.saturating_add(1);
                last.timestamp_ms = timestamp_ms;
                return;
            }
        }

        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        self.entries.push_back(ConsoleEntry {
            id,
            timestamp_ms,
            level,
            source: source_str,
            text: sanitized_text,
            count: 1,
            stack_trace: sanitized_stack,
        });
    }

    /// Returns a copy of all retained entries in chronological order.
    pub fn entries(&self) -> Vec<ConsoleEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Returns only error entries (for high-priority AI context inclusion).
    pub fn errors(&self) -> Vec<ConsoleEntry> {
        self.entries
            .iter()
            .filter(|e| e.level == ConsoleLogLevel::Error)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for ConsoleRingBuffer {
    fn default() -> Self {
        Self::new(CONSOLE_RING_CAPACITY)
    }
}

// ---------------------------------------------------------------------------
// Network Telemetry
// ---------------------------------------------------------------------------

/// A sanitized network transaction record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkEntry {
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub status_code: Option<i32>,
    pub mime_type: Option<String>,
    pub duration_ms: Option<f64>,
    pub failed: bool,
    pub failure_reason: Option<String>,
    pub preview: Option<String>,
    pub start_timestamp_ms: u64,
    pub end_timestamp_ms: Option<u64>,
}

/// Bounded ring buffer for live network requests.
pub struct NetworkRingBuffer {
    capacity: usize,
    entries: VecDeque<NetworkEntry>,
    lookup: HashMap<String, usize>, // maps request_id -> entries index
    sanitizer: SecretSanitizer,
}

impl NetworkRingBuffer {
    pub fn new(capacity: usize) -> Self {
        NetworkRingBuffer {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            lookup: HashMap::new(),
            sanitizer: SecretSanitizer::new(),
        }
    }

    /// Rebuilds lookup map indices after entries shift.
    fn reindex(&mut self) {
        self.lookup.clear();
        for (i, entry) in self.entries.iter().enumerate() {
            self.lookup.insert(entry.request_id.clone(), i);
        }
    }

    /// Ingest a `Network.requestWillBeSent` event.
    pub fn on_request_will_be_sent(
        &mut self,
        request_id: &str,
        url: &str,
        method: &str,
        timestamp_ms: u64,
    ) {
        // Redact secrets in query params or URL if any
        let sanitized_url = self.sanitizer.sanitize_string(url);

        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
            self.reindex();
        }

        let entry = NetworkEntry {
            request_id: request_id.to_string(),
            url: sanitized_url,
            method: method.to_string(),
            status_code: None,
            mime_type: None,
            duration_ms: None,
            failed: false,
            failure_reason: None,
            preview: None,
            start_timestamp_ms: timestamp_ms,
            end_timestamp_ms: None,
        };

        let idx = self.entries.len();
        self.entries.push_back(entry);
        self.lookup.insert(request_id.to_string(), idx);
    }

    /// Ingest a `Network.responseReceived` event.
    pub fn on_response_received(
        &mut self,
        request_id: &str,
        status_code: i32,
        mime_type: Option<&str>,
        timestamp_ms: u64,
    ) {
        if let Some(&idx) = self.lookup.get(request_id) {
            if let Some(entry) = self.entries.get_mut(idx) {
                entry.status_code = Some(status_code);
                entry.mime_type = mime_type.map(|m| m.to_string());
                entry.end_timestamp_ms = Some(timestamp_ms);
                if timestamp_ms >= entry.start_timestamp_ms {
                    entry.duration_ms = Some((timestamp_ms - entry.start_timestamp_ms) as f64);
                }
            }
        }
    }

    /// Ingest a `Network.loadingFinished` event.
    pub fn on_loading_finished(&mut self, request_id: &str, timestamp_ms: u64) {
        if let Some(&idx) = self.lookup.get(request_id) {
            if let Some(entry) = self.entries.get_mut(idx) {
                entry.end_timestamp_ms = Some(timestamp_ms);
                if timestamp_ms >= entry.start_timestamp_ms {
                    entry.duration_ms = Some((timestamp_ms - entry.start_timestamp_ms) as f64);
                }
            }
        }
    }

    /// Ingest a `Network.loadingFailed` event.
    pub fn on_loading_failed(&mut self, request_id: &str, error_text: &str, timestamp_ms: u64) {
        let sanitized_error = self.sanitizer.sanitize_string(error_text);
        if let Some(&idx) = self.lookup.get(request_id) {
            if let Some(entry) = self.entries.get_mut(idx) {
                entry.failed = true;
                entry.failure_reason = Some(sanitized_error);
                entry.end_timestamp_ms = Some(timestamp_ms);
                if timestamp_ms >= entry.start_timestamp_ms {
                    entry.duration_ms = Some((timestamp_ms - entry.start_timestamp_ms) as f64);
                }
            }
        }
    }

    /// Attach a response preview (JSON or text), stripping binary media and truncating to 512 bytes.
    pub fn set_response_preview(&mut self, request_id: &str, raw_preview: &str) {
        if let Some(&idx) = self.lookup.get(request_id) {
            if let Some(entry) = self.entries.get_mut(idx) {
                // Check if mime_type is binary media
                if let Some(ref mime) = entry.mime_type {
                    let m = mime.to_lowercase();
                    if m.starts_with("image/")
                        || m.starts_with("font/")
                        || m.starts_with("audio/")
                        || m.starts_with("video/")
                        || m.contains("octet-stream")
                    {
                        entry.preview = Some("[binary media stripped]".to_string());
                        return;
                    }
                }

                let sanitized = self.sanitizer.sanitize_string(raw_preview);
                let truncated = if sanitized.len() > MAX_PREVIEW_BYTES {
                    format!("{}... [truncated]", &sanitized[..MAX_PREVIEW_BYTES])
                } else {
                    sanitized
                };
                entry.preview = Some(truncated);
            }
        }
    }

    /// Returns a copy of all retained network transactions.
    pub fn entries(&self) -> Vec<NetworkEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Returns only failed requests (HTTP 4xx/5xx or transport failures).
    pub fn failed_requests(&self) -> Vec<NetworkEntry> {
        self.entries
            .iter()
            .filter(|e| e.failed || e.status_code.map_or(false, |s| s >= 400))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.lookup.clear();
    }
}

impl Default for NetworkRingBuffer {
    fn default() -> Self {
        Self::new(NETWORK_RING_CAPACITY)
    }
}

// ---------------------------------------------------------------------------
// DOM Telemetry Cache
// ---------------------------------------------------------------------------

/// Structured representation of a DOM node in the live cache.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomNode {
    pub node_id: i64,
    pub parent_id: Option<i64>,
    pub backend_node_id: Option<i64>,
    pub node_type: i32,
    pub node_name: String,
    pub local_name: String,
    pub node_value: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<i64>,
}

/// In-memory DOM cache with incremental mutation support.
#[derive(Debug, Clone, Default)]
pub struct DomTreeStore {
    root_id: Option<i64>,
    nodes: HashMap<i64, DomNode>,
    sanitizer: SecretSanitizer,
}

impl DomTreeStore {
    pub fn new() -> Self {
        DomTreeStore {
            root_id: None,
            nodes: HashMap::new(),
            sanitizer: SecretSanitizer::new(),
        }
    }

    /// Ingest a complete `DOM.getDocument` response root.
    pub fn set_document(&mut self, root_val: &serde_json::Value) {
        self.nodes.clear();
        if let Some(root_node) = self.parse_node(root_val, None) {
            self.root_id = Some(root_node.node_id);
        }
    }

    fn parse_node(
        &mut self,
        val: &serde_json::Value,
        parent_id: Option<i64>,
    ) -> Option<DomNode> {
        let node_id = val.get("nodeId")?.as_i64()?;
        let node_type = val.get("nodeType").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
        let node_name = val
            .get("nodeName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let local_name = val
            .get("localName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let raw_val = val
            .get("nodeValue")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let node_value = self.sanitizer.sanitize_string(raw_val);
        let backend_node_id = val.get("backendNodeId").and_then(|v| v.as_i64());

        // Parse attributes array: ["attr1", "val1", "attr2", "val2"]
        let mut attributes = HashMap::new();
        if let Some(attr_arr) = val.get("attributes").and_then(|v| v.as_array()) {
            for chunk in attr_arr.chunks_exact(2) {
                if let (Some(k), Some(v)) = (chunk[0].as_str(), chunk[1].as_str()) {
                    let sanitized_v = self.sanitizer.sanitize_string(v);
                    attributes.insert(k.to_string(), sanitized_v);
                }
            }
        }

        let mut children_ids = Vec::new();
        if let Some(children_arr) = val.get("children").and_then(|v| v.as_array()) {
            for child_val in children_arr {
                if let Some(child_node) = self.parse_node(child_val, Some(node_id)) {
                    children_ids.push(child_node.node_id);
                }
            }
        }

        let node = DomNode {
            node_id,
            parent_id,
            backend_node_id,
            node_type,
            node_name,
            local_name,
            node_value,
            attributes,
            children: children_ids,
        };

        self.nodes.insert(node_id, node.clone());
        Some(node)
    }

    pub fn root_id(&self) -> Option<i64> {
        self.root_id
    }

    pub fn get_node(&self, node_id: i64) -> Option<&DomNode> {
        self.nodes.get(&node_id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Apply `DOM.childNodeInserted` mutation.
    pub fn child_node_inserted(
        &mut self,
        parent_id: i64,
        previous_node_id: i64,
        node_val: &serde_json::Value,
    ) {
        if let Some(child) = self.parse_node(node_val, Some(parent_id)) {
            let child_id = child.node_id;
            if let Some(parent) = self.nodes.get_mut(&parent_id) {
                if previous_node_id > 0 {
                    if let Some(pos) = parent.children.iter().position(|&id| id == previous_node_id) {
                        parent.children.insert(pos + 1, child_id);
                    } else {
                        parent.children.push(child_id);
                    }
                } else {
                    parent.children.insert(0, child_id);
                }
            }
        }
    }

    /// Apply `DOM.childNodeRemoved` mutation.
    pub fn child_node_removed(&mut self, parent_id: i64, node_id: i64) {
        if let Some(parent) = self.nodes.get_mut(&parent_id) {
            parent.children.retain(|&id| id != node_id);
        }
        self.remove_recursive(node_id);
    }

    fn remove_recursive(&mut self, node_id: i64) {
        if let Some(node) = self.nodes.remove(&node_id) {
            for child_id in node.children {
                self.remove_recursive(child_id);
            }
        }
    }

    /// Apply `DOM.attributeModified` mutation.
    pub fn attribute_modified(&mut self, node_id: i64, name: &str, value: &str) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            let sanitized_v = self.sanitizer.sanitize_string(value);
            node.attributes.insert(name.to_string(), sanitized_v);
        }
    }

    /// Apply `DOM.attributeRemoved` mutation.
    pub fn attribute_removed(&mut self, node_id: i64, name: &str) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.attributes.remove(name);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_console_ring_deduplication_and_capacity() {
        let mut buf = ConsoleRingBuffer::new(3);

        // Deduplication test
        buf.push(ConsoleLogLevel::Error, "js", "Uncaught TypeError: null", None, 100);
        buf.push(ConsoleLogLevel::Error, "js", "Uncaught TypeError: null", None, 105);
        buf.push(ConsoleLogLevel::Error, "js", "Uncaught TypeError: null", None, 110);

        assert_eq!(buf.len(), 1);
        let entries = buf.entries();
        assert_eq!(entries[0].count, 3);
        assert_eq!(entries[0].timestamp_ms, 110);

        // Push new different logs to test bounding at 3
        buf.push(ConsoleLogLevel::Info, "app", "Message 2", None, 120);
        buf.push(ConsoleLogLevel::Warning, "css", "Message 3", None, 130);
        buf.push(ConsoleLogLevel::Verbose, "net", "Message 4", None, 140);

        assert_eq!(buf.len(), 3);
        let entries = buf.entries();
        assert_eq!(entries[0].text, "Message 2");
        assert_eq!(entries[1].text, "Message 3");
        assert_eq!(entries[2].text, "Message 4");
    }

    #[test]
    fn test_console_secret_sanitization() {
        let mut buf = ConsoleRingBuffer::new(10);
        buf.push(
            ConsoleLogLevel::Error,
            "auth",
            "Token failed: Bearer secret_token_xyz",
            None,
            100,
        );
        let entries = buf.entries();
        assert!(!entries[0].text.contains("secret_token_xyz"));
        assert!(entries[0].text.contains("[REDACTED"));
    }

    #[test]
    fn test_network_ring_lifecycle_and_binary_stripping() {
        let mut buf = NetworkRingBuffer::new(5);

        buf.on_request_will_be_sent("req-1", "https://api.com/v1/auth?token=secret123", "GET", 100);
        buf.on_response_received("req-1", 200, Some("application/json"), 150);
        buf.on_loading_finished("req-1", 160);
        buf.set_response_preview("req-1", r#"{"status": "ok", "user": "alice"}"#);

        let entries = buf.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].duration_ms, Some(60.0));
        assert_eq!(entries[0].status_code, Some(200));
        assert_eq!(entries[0].preview.as_deref(), Some(r#"{"status": "ok", "user": "alice"}"#));

        // Test binary stripping
        buf.on_request_will_be_sent("req-2", "https://api.com/avatar.png", "GET", 200);
        buf.on_response_received("req-2", 200, Some("image/png"), 220);
        buf.set_response_preview("req-2", "RAW_IMAGE_BINARY_BYTES_XYZ");

        let entries2 = buf.entries();
        assert_eq!(entries2[1].preview.as_deref(), Some("[binary media stripped]"));
    }

    #[test]
    fn test_dom_tree_store_and_mutations() {
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
                    "attributes": ["lang", "en"],
                    "children": [
                        {
                            "nodeId": 3,
                            "nodeType": 1,
                            "nodeName": "BODY",
                            "localName": "body",
                            "children": []
                        }
                    ]
                }
            ]
        });

        store.set_document(&doc);
        assert_eq!(store.node_count(), 3);
        assert_eq!(store.root_id(), Some(1));

        // Child node inserted
        let new_child = json!({
            "nodeId": 4,
            "nodeType": 1,
            "nodeName": "H1",
            "localName": "h1",
            "attributes": ["class", "title"],
            "children": []
        });
        store.child_node_inserted(3, 0, &new_child);

        let body = store.get_node(3).unwrap();
        assert_eq!(body.children, vec![4]);
        let h1 = store.get_node(4).unwrap();
        assert_eq!(h1.attributes.get("class").map(|s| s.as_str()), Some("title"));

        // Attribute modified
        store.attribute_modified(4, "class", "title active");
        let h1_updated = store.get_node(4).unwrap();
        assert_eq!(h1_updated.attributes.get("class").map(|s| s.as_str()), Some("title active"));

        // Child node removed
        store.child_node_removed(3, 4);
        assert!(store.get_node(4).is_none());
        assert_eq!(store.get_node(3).unwrap().children.len(), 0);
    }
}
