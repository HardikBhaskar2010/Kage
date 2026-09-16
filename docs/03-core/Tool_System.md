# KAGE Tool System Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-CORE-002 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Agent Infrastructure & Automation Bus |

---

## 1. Executive Summary & Architectural Role

The **KAGE Tool System** is the execution engine that powers browser automation, developer inspections, and AI agent workflows. Rather than allowing scripts, plugins, or AI models to execute unstructured code or raw CDP commands directly, KAGE exposes a typed, permission-governed, auditable **Tool Bus**.

```
╔═════════════════════════════════════════════════════════════════════════════════════╗
║                            PRIME ARCHITECTURAL INVARIANT                            ║
║                                                                                     ║
║        AI NEVER GETS BROWSER AUTHORITY DIRECTLY. EVERY AI ACTION BECOMES            ║
║        A TYPED TOOL BUS REQUEST, AND THE TOOL BUS—NOT THE MODEL, PROMPT,            ║
║        PLUGIN, OR UI—OWNS VALIDATION, AUTHORIZATION, EXECUTION,                     ║
║        CANCELLATION, AND AUDITING.                                                  ║
╚═════════════════════════════════════════════════════════════════════════════════════╝
```

Every action—whether requested by the AI developer assistant, the Micro Inspect overlay, or a recorded test flow—must be dispatched through the Tool Bus.

```
┌────────────────────────────────────────────────────────┐
│                        CALLERS                         │
│  AI Assistant  │  Micro Inspect  │  Testing Lab Record │
└───────────────────────────┬────────────────────────────┘
                            │ Tool Invocation Request
                            ▼
┌────────────────────────────────────────────────────────┐
│                     KAGE TOOL BUS                      │
│                                                        │
│  1. Versioned Tool Lookup (name@version)               │
│  2. JSON Schema Parameter Validation                   │
│  3. Origin & Session Target Binding                    │
│  4. Policy-Based Permission Decision Engine            │
│  5. User Confirmation Binding (Exact Args Match)       │
│  6. Asynchronous Execution with Timeout & Cancellation │
│  7. Secret Redaction Pipeline                          │
│  8. Security Audit Log Append (SQLite)                 │
│  9. Typed Result Serialization                         │
└───────────────────────────┬────────────────────────────┘
                            │ Target Operation
                            ▼
┌────────────────────────────────────────────────────────┐
│                    EXECUTION DRIVERS                   │
│  CDP Client  │  CEF Native API  │  Tauri Host Services │
└────────────────────────────────────────────────────────┘
```

---

## 2. Tool Definition, Versioning & Permission Contracts

Tools are implemented natively in Rust to guarantee memory safety, compile-time schema validation, and high-speed execution:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PermissionTier {
    Tier0ReadOnly = 0,      // Read-only inspection (inspect_dom, get_box_model)
    Tier1TransientUI = 1,    // Visible highlight, no DOM changes (highlight_element)
    Tier2PageMutation = 2,   // Modifies active page DOM/CSS (modify_css, modify_dom)
    Tier3ExternalState = 3,  // Navigates, alters cookies, writes files (navigate, clear_storage)
    Tier4SystemCritical = 4, // Deletes workspaces, clears profiles (delete_workspace)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyPreference {
    AskEveryTime,
    AskOncePerSession,
    AllowForSite(String),
    AlwaysAllow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationRequirement {
    None,
    AmbientIndicator,
    ExplicitUserConfirmation,
    StrongWarningModal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub tier: PermissionTier,
    pub policy: PolicyPreference,
    pub confirmation: ConfirmationRequirement,
    pub explanation: String,
}

#[async_trait]
pub trait KageTool: Send + Sync {
    /// Programmatic identifier (e.g. "modify_css")
    fn name(&self) -> &'static str;

    /// Semantic version of the tool implementation (e.g. "1.0.0")
    fn version(&self) -> &'static str;

    /// Monotonic schema version for backwards compatibility
    fn schema_version(&self) -> u32 { 1 }

    /// Human-readable description for UI and LLM function calling
    fn description(&self) -> &'static str;

    /// Strict JSON Schema defining input parameters
    fn parameter_schema(&self) -> serde_json::Value;

    /// Assigned base security permission tier
    fn base_permission_tier(&self) -> PermissionTier;

    /// Timeout boundary in milliseconds (default: 5,000ms)
    fn timeout_ms(&self) -> u64 { 5000 }

    /// Execution handler
    async fn execute(
        &self,
        ctx: &ToolExecutionContext,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError>;
}
```

---

## 3. Core Built-In Tool Catalog

| Tool Identifier | Tier | Driver | Purpose & Description |
|---|---|---|---|
| `inspect_dom@1` | Tier 0 | CDP `DOM` | Retrieves element tag, attributes, parent, children, and outerHTML snippet. |
| `get_box_model@1` | Tier 0 | CDP `DOM` | Retrieves physical width, height, margin, border, and padding quads. |
| `get_computed_style@1` | Tier 0 | CDP `CSS` | Retrieves resolved CSS properties and active cascade rules for a selector. |
| `get_console_errors@1` | Tier 0 | Context Engine | Retrieves the last N buffered browser console errors, warnings, and traces. |
| `get_network_requests@1` | Tier 0 | Context Engine | Retrieves recent HTTP transactions, statuses, timing, and failure payloads. |
| `take_screenshot@1` | Tier 0 | CDP `Page` | Captures high-resolution PNG of viewport or specific element bounding rect. |
| `highlight_element@1` | Tier 1 | CDP `Overlay` | Draws a visual bounding box over a node for user orientation. |
| `modify_css@1` | Tier 2 | CDP `CSS` | Injects, modifies, or deletes CSS rules targeting an element or stylesheet. |
| `modify_dom@1` | Tier 2 | CDP `DOM` | Modifies DOM attributes, inserts child elements, or replaces outerHTML. |
| `run_javascript@1` | Tier 2 | CDP `Runtime` | **Hardened JS evaluator** running in isolated world with strict sandboxing. |
| `navigate@1` | Tier 3 | CEF / CDP | Navigates the active tab to a new target URL or reloads. |
| `clear_storage@1` | Tier 3 | CEF Storage | Clears cookies, local storage, or cache partitions for active origin. |
| `record_action@1` | Tier 0 | Testing Lab | Appends a normalized user interaction to the active test session. |

---

## 4. Hardening of `run_javascript@1`

Because arbitrary JavaScript evaluation can bypass granular tool permissions, `run_javascript@1` enforces strict containment:

```
┌────────────────────────────────────────────────────────┐
│             run_javascript@1 HARDENING RULES           │
├───────────────────┬────────────────────────────────────┤
│ Execution World   │ Isolated World by default. Cannot  │
│                   │ pollute page window object.        │
├───────────────────┼────────────────────────────────────┤
│ Execution Timeout │ Strict 3,000 ms hard timeout.      │
├───────────────────┼────────────────────────────────────┤
│ Return Size Limit │ Max 64 KB stringified payload.     │
├───────────────────┼────────────────────────────────────┤
│ Origin Binding    │ Bound to active tab frame origin.  │
├───────────────────┼────────────────────────────────────┤
│ Prohibited APIs   │ window.open, navigation, downloads,│
│                   │ and clipboard writes intercepted.  │
├───────────────────┼────────────────────────────────────┤
│ Async Promises    │ Long-lived background microtasks   │
│                   │ rejected after execution timeout.  │
└───────────────────┴────────────────────────────────────┘
```

If an evaluation attempts navigation or file download, the Tool Bus traps the event and fails the execution with `E_PROHIBITED_API`.

---

## 5. Parameter Validation & JSON Schema Integration

Tools expose standard JSON Schemas to enable native function-calling with modern AI models:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ModifyCssParameters",
  "type": "object",
  "properties": {
    "tab_id": { "type": "string", "description": "Target KAGE tab identifier" },
    "selector": { "type": "string", "description": "CSS selector targeting elements to modify" },
    "declarations": {
      "type": "object",
      "additionalProperties": { "type": "string" },
      "description": "Key-value map of CSS properties (e.g. {'background-color': '#A53860'})"
    }
  },
  "required": ["tab_id", "selector", "declarations"]
}
```

Before execution, arguments are validated using the `jsonschema` crate. Schema mismatches abort instantly with `E_VALIDATION_FAILED` before touching the browser engine.

---

## 6. Policy-Based Permission Decision Engine

KAGE does not hard-code automatic approvals for state-modifying actions. The Tool Bus consults the **Policy Engine**:

```rust
pub async fn evaluate_permission(
    tool: &dyn KageTool,
    args: &serde_json::Value,
    origin: &str,
    user_policies: &PolicyStore,
) -> PermissionDecision {
    match tool.base_permission_tier() {
        PermissionTier::Tier0ReadOnly => PermissionDecision {
            tier: PermissionTier::Tier0ReadOnly,
            policy: PolicyPreference::AlwaysAllow,
            confirmation: ConfirmationRequirement::None,
            explanation: "Read-only inspection is safe and non-mutating.".into(),
        },
        PermissionTier::Tier1TransientUI => PermissionDecision {
            tier: PermissionTier::Tier1TransientUI,
            policy: PolicyPreference::AlwaysAllow,
            confirmation: ConfirmationRequirement::AmbientIndicator,
            explanation: "Visual overlay displayed in viewport.".into(),
        },
        PermissionTier::Tier2PageMutation => {
            let policy = user_policies.get_site_policy(origin, tool.name());
            match policy {
                PolicyPreference::AllowForSite(_) | PolicyPreference::AlwaysAllow => PermissionDecision {
                    tier: PermissionTier::Tier2PageMutation,
                    policy,
                    confirmation: ConfirmationRequirement::AmbientIndicator,
                    explanation: "Mutation allowed per user site policy.".into(),
                },
                _ => PermissionDecision {
                    tier: PermissionTier::Tier2PageMutation,
                    policy,
                    confirmation: ConfirmationRequirement::ExplicitUserConfirmation,
                    explanation: "Page mutation requires confirmation.".into(),
                }
            }
        },
        PermissionTier::Tier3ExternalState => PermissionDecision {
            tier: PermissionTier::Tier3ExternalState,
            policy: PolicyPreference::AskEveryTime,
            confirmation: ConfirmationRequirement::ExplicitUserConfirmation,
            explanation: "Alters persistent browser storage or navigates tab.".into(),
        },
        PermissionTier::Tier4SystemCritical => PermissionDecision {
            tier: PermissionTier::Tier4SystemCritical,
            policy: PolicyPreference::AskEveryTime,
            confirmation: ConfirmationRequirement::StrongWarningModal,
            explanation: "System-critical destructive operation.".into(),
        },
    }
}
```

### Exact Confirmation Binding
When the user approves an action in a confirmation modal:
- A cryptographic hash is calculated over `(tool_name, tool_version, origin, sorted_arguments)`.
- If the agent modifies any argument prior to execution, the confirmation hash invalidates instantly, forcing a re-prompt.

---

## 7. Security Audit Logging with Secret Redaction

Every tool execution is recorded in an append-only SQLite database (`%APPDATA%/Kage/security_audit.db`).

> [!IMPORTANT]
> **Audit Hygiene Rule:** Raw secrets (passwords, bearer tokens, private keys, auth headers) are **strictly redacted** before database insertion.

```sql
CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    actor TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_version TEXT NOT NULL,
    permission_tier INTEGER NOT NULL,
    target_origin TEXT NOT NULL,
    outcome TEXT NOT NULL,
    execution_time_ms REAL NOT NULL,
    redacted_arguments_json TEXT NOT NULL,
    redacted_result_json TEXT NOT NULL
);
```
