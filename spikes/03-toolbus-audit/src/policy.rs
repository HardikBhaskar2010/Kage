use crate::tool::{KageTool, PermissionTier, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallerType {
    AiAgent,
    DevToolsPanel,
    Plugin,
    UserDirect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerContext {
    pub caller_id: String,
    pub caller_type: CallerType,
    pub session_id: String,
    pub workspace_id: String,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    AllowSessionGranted,
    RequiresUserPrompt { prompt_message: String },
    Denied { reason: String },
}

pub struct PolicyEngine {
    // Session-scoped grants: set of (session_id, tool_name)
    session_grants: RwLock<HashSet<(String, String)>>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            session_grants: RwLock::new(HashSet::new()),
        }
    }

    pub fn grant_session_permission(&self, session_id: &str, tool_name: &str) {
        let mut grants = self.session_grants.write().unwrap();
        grants.insert((session_id.to_string(), tool_name.to_string()));
    }

    pub fn revoke_session_grants(&self, session_id: &str) {
        let mut grants = self.session_grants.write().unwrap();
        grants.retain(|(s, _)| s != session_id);
    }

    /// Evaluates the policy taking declared tier as INPUT alongside caller, session, and approval token.
    pub fn evaluate(
        &self,
        tool: &dyn KageTool,
        caller: &CallerContext,
        user_approval_token: Option<&str>,
    ) -> Result<PermissionDecision, ToolError> {
        let tier = tool.tier();
        let tool_name = tool.name();

        // Tier 4 is unconditionally blocked for AI agents and plugins
        if tier == PermissionTier::Tier4DangerousBlocked {
            return Ok(PermissionDecision::Denied {
                reason: format!(
                    "Tool '{}' is Tier 4 (Dangerous / System) and is strictly blocked by KAGE Security Model",
                    tool_name
                ),
            });
        }

        match tier {
            PermissionTier::Tier1ReadOnlyPassive => {
                // Tier 1 is auto-allowed for all callers
                Ok(PermissionDecision::Allow)
            }
            PermissionTier::Tier2StateMutating => {
                // Tier 2: check if session has already been granted
                let grants = self.session_grants.read().unwrap();
                if grants.contains(&(caller.session_id.clone(), tool_name.to_string())) {
                    Ok(PermissionDecision::AllowSessionGranted)
                } else if caller.caller_type == CallerType::UserDirect {
                    Ok(PermissionDecision::Allow)
                } else {
                    // Requires session grant
                    Ok(PermissionDecision::RequiresUserPrompt {
                        prompt_message: format!(
                            "Grant session permission for '{}' to perform state-mutating actions in session '{}'?",
                            tool_name, caller.session_id
                        ),
                    })
                }
            }
            PermissionTier::Tier3ExternalHighRisk => {
                // Tier 3 requires explicit per-action confirmation token
                if let Some(token) = user_approval_token {
                    if token.starts_with("APPROVED:") && token.contains(tool_name) {
                        Ok(PermissionDecision::Allow)
                    } else {
                        Ok(PermissionDecision::Denied {
                            reason: "Invalid or mismatched user approval token for Tier 3 action".to_string(),
                        })
                    }
                } else {
                    Ok(PermissionDecision::RequiresUserPrompt {
                        prompt_message: format!(
                            "Confirm high-risk action '{}' requested by '{}'",
                            tool_name, caller.caller_id
                        ),
                    })
                }
            }
            PermissionTier::Tier4DangerousBlocked => unreachable!(),
        }
    }
}
