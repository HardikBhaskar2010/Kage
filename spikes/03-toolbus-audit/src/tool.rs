use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionTier {
    Tier1ReadOnlyPassive,
    Tier2StateMutating,
    Tier3ExternalHighRisk,
    Tier4DangerousBlocked,
}

impl std::fmt::Display for PermissionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionTier::Tier1ReadOnlyPassive => write!(f, "Tier 1 (Read-Only Passive)"),
            PermissionTier::Tier2StateMutating => write!(f, "Tier 2 (State-Mutating)"),
            PermissionTier::Tier3ExternalHighRisk => write!(f, "Tier 3 (External High-Risk)"),
            PermissionTier::Tier4DangerousBlocked => write!(f, "Tier 4 (Dangerous Blocked)"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Validation failed for tool '{tool}': {reason}")]
    ValidationFailed { tool: String, reason: String },

    #[error("Permission denied for tool '{tool}' ({tier}): {reason}")]
    PermissionDenied {
        tool: String,
        tier: PermissionTier,
        reason: String,
    },

    #[error("Execution cancelled")]
    Cancelled,

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait KageTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn tier(&self) -> PermissionTier;
    fn schema(&self) -> Value;
    async fn execute(&self, args: Value, cancel: CancellationToken) -> Result<Value, ToolError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn KageTool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn KageTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn KageTool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<(&str, PermissionTier, &'static str)> {
        self.tools
            .values()
            .map(|t| (t.name(), t.tier(), t.description()))
            .collect()
    }
}

/// Lightweight, deterministic JSON schema validator for tool input validation
pub fn validate_schema(schema: &Value, args: &Value) -> Result<(), String> {
    if !args.is_object() {
        return Err("Input arguments must be a JSON object".to_string());
    }

    let args_map = args.as_object().unwrap();

    // Check required properties
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(field) = req.as_str() {
                if !args_map.contains_key(field) {
                    return Err(format!("Missing required parameter: '{}'", field));
                }
            }
        }
    }

    // Check property types
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, prop_spec) in properties {
            if let Some(arg_val) = args_map.get(prop_name) {
                if let Some(expected_type) = prop_spec.get("type").and_then(|t| t.as_str()) {
                    let type_matches = match expected_type {
                        "string" => arg_val.is_string(),
                        "number" => arg_val.is_number(),
                        "integer" => arg_val.is_i64() || arg_val.is_u64(),
                        "boolean" => arg_val.is_boolean(),
                        "array" => arg_val.is_array(),
                        "object" => arg_val.is_object(),
                        _ => true,
                    };

                    if !type_matches {
                        return Err(format!(
                            "Parameter '{}' expected type '{}', got '{:?}'",
                            prop_name, expected_type, arg_val
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}
