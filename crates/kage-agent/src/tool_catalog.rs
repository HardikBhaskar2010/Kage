//! Registry-driven dynamic tool discovery and schema projection for LLM orchestration.
//!
//! Enforces **INV-02**: All tool capabilities exposed to the autonomous agent planner
//! derive strictly from [`CapabilityRegistry`]. No tool commands are hardcoded in the agent.
//! Developer-only tools (such as `devtools.runtime.evaluate`) are structurally filtered out.

use std::collections::HashMap;
use kage_core::policy::PermissionTier;
use kage_core::registry::{CapabilityRegistry, IdempotencyClassification, ToolCategory, ToolMetadata};
use serde::{Deserialize, Serialize};

/// Projected tool capability description formatted for the agent model and planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Canonical governed tool identifier (e.g. `page.click`, `browser.navigate`).
    pub name: String,
    /// Semantic description of tool action and purpose.
    pub description: String,
    /// JSON schema describing the expected input parameters.
    pub input_schema: serde_json::Value,
    /// Functional grouping.
    pub category: ToolCategory,
    /// Permission tier enforced by PolicyEngine.
    pub tier: PermissionTier,
    /// Whether the action is read-only, idempotent, or non-idempotent.
    pub idempotency: IdempotencyClassification,
    /// Execution timeout in milliseconds.
    pub timeout_ms: u64,
}

impl From<&ToolMetadata> for ToolDefinition {
    fn from(meta: &ToolMetadata) -> Self {
        Self {
            name: meta.tool_id.clone(),
            description: meta.description.clone(),
            input_schema: meta.schema.clone(),
            category: meta.category,
            tier: meta.tier,
            idempotency: meta.idempotency,
            timeout_ms: meta.timeout_ms,
        }
    }
}

/// Dynamic, registry-derived tool catalog exposed to the autonomous agent runtime.
#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolCatalog {
    /// Create an empty tool catalog.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Dynamically discover and populate tools from the authoritative [`CapabilityRegistry`].
    ///
    /// For autonomous agent execution, `allow_developer` MUST be `false` to enforce
    /// the lockdown of developer-only tools (`devtools.runtime.evaluate`).
    pub async fn from_registry(registry: &CapabilityRegistry, allow_developer: bool) -> Self {
        let metadata_list = registry.query_for_agent(allow_developer).await;
        let mut tools = HashMap::new();
        for meta in metadata_list {
            tools.insert(meta.tool_id.clone(), ToolDefinition::from(&meta));
        }
        Self { tools }
    }

    /// Retrieve a projected tool definition by tool name.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Check whether a tool is present in the catalog.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// List all discovered tools.
    pub fn list(&self) -> Vec<ToolDefinition> {
        self.tools.values().cloned().collect()
    }

    /// Count of available tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Format tool definitions into OpenAI/Anthropic-compatible function schema definitions.
    pub fn to_model_tools(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect()
    }
}
