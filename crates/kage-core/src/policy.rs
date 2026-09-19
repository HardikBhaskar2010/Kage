//! 4-tier permission policy engine (KAGE-SEC-001).
//!
//! The policy engine is the **authoritative** decision point for all tool dispatch.
//! It receives a rich [`PolicyContext`] — caller identity, session, workspace, and
//! requested tool — and emits a typed [`PolicyDecision`].  The [`ToolBus`] is
//! responsible for translating `Decision::RequireConfirmation` into a UI modal and
//! `Decision::Deny` into a [`ToolError::PermissionDenied`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Permission tiers
// ---------------------------------------------------------------------------

/// The four permission tiers defined in `docs/08-security/Security_Model.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionTier {
    /// **Tier 1 — Read-Only / Passive**
    ///
    /// Auto-allowed without prompting. Example: inspect DOM, read console logs.
    ReadOnly = 1,

    /// **Tier 2 — State-Mutating / Low Risk**
    ///
    /// Session-scoped approval. Example: click element, fill input field.
    StateMutating = 2,

    /// **Tier 3 — External / High Risk**
    ///
    /// Per-action modal confirmation required. Example: network replay, file export.
    External = 3,

    /// **Tier 4 — Dangerous / Blocked**
    ///
    /// Strictly forbidden for the AI subsystem and unprivileged plugins.
    /// Example: bypass certificate errors, modify browser binaries.
    Dangerous = 4,
}

impl std::fmt::Display for PermissionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionTier::ReadOnly => write!(f, "Tier1/ReadOnly"),
            PermissionTier::StateMutating => write!(f, "Tier2/StateMutating"),
            PermissionTier::External => write!(f, "Tier3/External"),
            PermissionTier::Dangerous => write!(f, "Tier4/Dangerous"),
        }
    }
}

/// Failure policy when audit ledger append fails (KAGE-SEC-003, INV-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditFailurePolicy {
    /// Mutating / privileged actions MUST fail closed: No audit, no privileged action.
    FailClosed,
    /// Non-sensitive read telemetry may degrade gracefully with warning.
    DegradeGraceful,
}

impl PermissionTier {
    /// Determines whether an audit write failure must abort execution (INV-05).
    pub fn audit_failure_policy(&self) -> AuditFailurePolicy {
        match self {
            PermissionTier::ReadOnly => AuditFailurePolicy::DegradeGraceful,
            PermissionTier::StateMutating
            | PermissionTier::External
            | PermissionTier::Dangerous => AuditFailurePolicy::FailClosed,
        }
    }
}

// ---------------------------------------------------------------------------
// Policy context — inputs to the decision function
// ---------------------------------------------------------------------------

/// All context that the policy engine receives when adjudicating a tool call.
///
/// The engine is a *context-aware* decision function: `PermissionTier` is an input
/// (the minimum tier the tool requires), not the output. The engine may further
/// restrict based on session grants, workspace rules, and caller identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// Stable identifier of the requesting caller (e.g. `"ai_subsystem"`, `"plugin:devtools"`).
    pub caller_id: String,
    /// Opaque session handle scoping previously granted Tier-2 approvals.
    pub session_id: String,
    /// Active workspace identifier (used for workspace-level allow/deny lists).
    pub workspace_id: String,
    /// The tool being requested.
    pub tool_id: String,
    /// Minimum tier the tool declares it requires.
    pub required_tier: PermissionTier,
    /// Whether the user has pre-granted this tool for the current session.
    pub session_granted: bool,
}

// ---------------------------------------------------------------------------
// Policy decision
// ---------------------------------------------------------------------------

/// The output of the policy engine — what the [`ToolBus`] must do next.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Proceed immediately — no confirmation needed.
    Allow,
    /// Pause dispatch; surface a confirmation modal to the user.
    /// If the user approves, the bus may retry with `session_granted = true`.
    RequireConfirmation { reason: String },
    /// Hard deny — do not execute, surface error to caller.
    Deny { reason: String },
}

// ---------------------------------------------------------------------------
// PolicyEngine error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("policy configuration invalid: {0}")]
    ConfigInvalid(String),
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

/// Stateless, context-aware permission adjudicator.
///
/// The default implementation enforces the spec-defined rules for each tier.
/// Replace or extend via trait-object injection in tests or future plugin hooks.
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn new() -> Self {
        PolicyEngine
    }

    /// Adjudicate a tool call given its full [`PolicyContext`].
    pub fn adjudicate(&self, ctx: &PolicyContext) -> PolicyDecision {
        match ctx.required_tier {
            // Tier 1 — always allowed for any registered caller.
            PermissionTier::ReadOnly => PolicyDecision::Allow,

            // Tier 2 — allowed if the session has previously granted this tool.
            PermissionTier::StateMutating => {
                if ctx.session_granted {
                    PolicyDecision::Allow
                } else {
                    PolicyDecision::RequireConfirmation {
                        reason: format!(
                            "Tool '{}' may mutate browser state. Approve for this session?",
                            ctx.tool_id
                        ),
                    }
                }
            }

            // Tier 3 — always requires explicit per-action confirmation.
            PermissionTier::External => PolicyDecision::RequireConfirmation {
                reason: format!(
                    "Tool '{}' performs an external or high-risk action (e.g. network, file I/O). \
                     Confirm this specific action?",
                    ctx.tool_id
                ),
            },

            // Tier 4 — hard block.
            PermissionTier::Dangerous => PolicyDecision::Deny {
                reason: format!(
                    "Tool '{}' is classified Tier 4 (Dangerous) and is permanently blocked \
                     for AI callers and unprivileged plugins.",
                    ctx.tool_id
                ),
            },
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(tier: PermissionTier, granted: bool) -> PolicyContext {
        PolicyContext {
            caller_id: "ai_subsystem".into(),
            session_id: "sess-test".into(),
            workspace_id: "ws-default".into(),
            tool_id: "test.tool".into(),
            required_tier: tier,
            session_granted: granted,
        }
    }

    #[test]
    fn tier1_always_allowed() {
        let engine = PolicyEngine::new();
        assert!(matches!(
            engine.adjudicate(&ctx(PermissionTier::ReadOnly, false)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn tier2_requires_session_grant() {
        let engine = PolicyEngine::new();
        assert!(matches!(
            engine.adjudicate(&ctx(PermissionTier::StateMutating, false)),
            PolicyDecision::RequireConfirmation { .. }
        ));
        assert!(matches!(
            engine.adjudicate(&ctx(PermissionTier::StateMutating, true)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn tier3_always_confirms() {
        let engine = PolicyEngine::new();
        assert!(matches!(
            engine.adjudicate(&ctx(PermissionTier::External, true)),
            PolicyDecision::RequireConfirmation { .. }
        ));
    }

    #[test]
    fn tier4_always_denied() {
        let engine = PolicyEngine::new();
        assert!(matches!(
            engine.adjudicate(&ctx(PermissionTier::Dangerous, true)),
            PolicyDecision::Deny { .. }
        ));
    }
}
