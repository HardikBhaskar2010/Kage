//! Dynamic Capability Registry with Declarative Verification & Idempotency Metadata.
//!
//! Bridges KageTools, profile permission policies, and the autonomous agent planner:
//! ```text
//!                 Capability Registry
//!                          │
//!                          ├── tool_id & version
//!                          ├── schema (typed inputs & outputs)
//!                          ├── permission tier & required capabilities
//!                          ├── idempotency classification:
//!                          │     ├── ReadOnly       (safe to re-execute anytime)
//!                          │     ├── Idempotent     (re-executing yields identical state)
//!                          │     └── NonIdempotent  (state-mutating)
//!                          ├── retry_policy:
//!                          │     ├── Never          (non-idempotent mutations)
//!                          │     ├── SafeRetry      (read-only or idempotent operations)
//!                          │     └── VerifyBeforeRetry (must verify state before retrying)
//!                          ├── timeout & cancellation semantics
//!                          ├── observation metadata
//!                          └── declarative verification contract:
//!                                ├── page.click      -> [element_exists, opt_nav_started, opt_dom_mutation]
//!                                ├── browser.navigate -> [NavigationCommitted, NavigationCompleted, url_matches]
//!                                ├── page.fill       -> [element_value_matches, input_dispatched]
//!                                └── download.start  -> [download_item_created, path_bound]
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::policy::PermissionTier;

/// Idempotency classification of a governed browser action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyClassification {
    /// Safe to re-execute anytime (reads, observations, queries).
    ReadOnly,
    /// Re-executing yields identical state (navigation to URL, setting input field, closing tab).
    Idempotent,
    /// State-mutating operation (form submission, payment, clicking dynamic toggles, file download).
    NonIdempotent,
}

impl Default for IdempotencyClassification {
    fn default() -> Self {
        Self::NonIdempotent
    }
}

/// Recovery and retry policy governing tool failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryPolicy {
    /// Never automatically retry (non-idempotent mutations, payments, submissions).
    Never,
    /// Safe to retry upon transient failure (read-only or idempotent operations).
    SafeRetry,
    /// Must verify actual page/application state before deciding to retry.
    VerifyBeforeRetry,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::Never
    }
}

/// Declarative postcondition verification contract evaluated by the Deterministic Verifier (INV-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarativeContract {
    ElementExists { selector: String },
    ElementValueMatches { selector: String, expected: String },
    NavigationCommitted,
    NavigationCompleted,
    UrlMatches { pattern: String },
    DomMutationObserved,
    DownloadItemCreated,
    PathBound,
    TabCreated,
    ProfileBound,
    Custom(String),
}

/// Functional category for grouping and discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Browser,
    Tab,
    PageInteraction,
    PageObservation,
    Download,
    Developer,
}

/// Rich metadata registered for each governed tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub tool_id: String,
    pub version: String,
    pub description: String,
    pub category: ToolCategory,
    pub tier: PermissionTier,
    pub schema: serde_json::Value,
    pub idempotency: IdempotencyClassification,
    pub retry_policy: RetryPolicy,
    pub timeout_ms: u64,
    pub declarative_contracts: Vec<DeclarativeContract>,
    /// When true, this tool is strictly restricted to interactive developer REPL/DevTools
    /// and cannot be queried or executed by autonomous agent planners without Tier 3 escalation.
    pub is_developer_only: bool,
}

impl ToolMetadata {
    pub fn new(
        tool_id: impl Into<String>,
        category: ToolCategory,
        tier: PermissionTier,
        schema: serde_json::Value,
    ) -> Self {
        Self {
            tool_id: tool_id.into(),
            version: "1.0.0".to_string(),
            description: String::new(),
            category,
            tier,
            schema,
            idempotency: IdempotencyClassification::NonIdempotent,
            retry_policy: RetryPolicy::Never,
            timeout_ms: 10_000,
            declarative_contracts: Vec::new(),
            is_developer_only: false,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_idempotency(mut self, idempotency: IdempotencyClassification) -> Self {
        self.idempotency = idempotency;
        self
    }

    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry_policy = retry;
        self
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_contract(mut self, contract: DeclarativeContract) -> Self {
        self.declarative_contracts.push(contract);
        self
    }

    pub fn with_contracts(mut self, contracts: Vec<DeclarativeContract>) -> Self {
        self.declarative_contracts = contracts;
        self
    }

    pub fn developer_only(mut self) -> Self {
        self.is_developer_only = true;
        self.category = ToolCategory::Developer;
        self
    }
}

/// Dynamic, thread-safe capability registry for all registered browser tools.
#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    tools: Arc<RwLock<HashMap<String, ToolMetadata>>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update metadata for a tool.
    pub async fn register(&self, metadata: ToolMetadata) {
        let mut tools = self.tools.write().await;
        tools.insert(metadata.tool_id.clone(), metadata);
    }

    /// Retrieve metadata for a specific tool ID.
    pub async fn get(&self, tool_id: &str) -> Option<ToolMetadata> {
        let tools = self.tools.read().await;
        tools.get(tool_id).cloned()
    }

    /// Return all registered tools.
    pub async fn list_all(&self) -> Vec<ToolMetadata> {
        let tools = self.tools.read().await;
        let mut list: Vec<ToolMetadata> = tools.values().cloned().collect();
        list.sort_by(|a, b| a.tool_id.cmp(&b.tool_id));
        list
    }

    /// Query tools available for an autonomous agent planner.
    ///
    /// If `allow_developer_capabilities` is false (default), all developer-only tools
    /// (such as `devtools.runtime.evaluate`) are excluded from agent discovery,
    /// enforcing the strict prohibition of `eval_js` as a generic fallback.
    pub async fn query_for_agent(&self, allow_developer_capabilities: bool) -> Vec<ToolMetadata> {
        let tools = self.tools.read().await;
        let mut list: Vec<ToolMetadata> = tools
            .values()
            .filter(|meta| allow_developer_capabilities || !meta.is_developer_only)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.tool_id.cmp(&b.tool_id));
        list
    }

    /// Filter tools by maximum granted permission tier.
    pub async fn filter_by_max_tier(&self, max_tier: PermissionTier) -> Vec<ToolMetadata> {
        let tools = self.tools.read().await;
        let mut list: Vec<ToolMetadata> = tools
            .values()
            .filter(|meta| (meta.tier as u8) <= (max_tier as u8))
            .cloned()
            .collect();
        list.sort_by(|a, b| a.tool_id.cmp(&b.tool_id));
        list
    }

    /// Filter tools by functional category.
    pub async fn filter_by_category(&self, category: ToolCategory) -> Vec<ToolMetadata> {
        let tools = self.tools.read().await;
        let mut list: Vec<ToolMetadata> = tools
            .values()
            .filter(|meta| meta.category == category)
            .cloned()
            .collect();
        list.sort_by(|a, b| a.tool_id.cmp(&b.tool_id));
        list
    }
}
