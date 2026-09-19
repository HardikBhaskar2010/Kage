//! `kage-core` — Central Tool Bus, permission policy engine, and tool execution governance.
//!
//! # Architectural Contract (KAGE-ARCH-001 / KAGE-SEC-001)
//!
//! The AI subsystem **never** receives browser authority directly. Every action is dispatched
//! as a typed [`KageTool`] request through the [`ToolBus`]. The bus owns:
//!
//! * **Schema validation** — requests must conform to their registered JSON Schema.
//! * **Policy enforcement** — a 4-tier permission model with context-aware decisions.
//! * **Secret redaction** — recursive scrubbing before any context escapes to the LLM.
//! * **Cancellation** — every in-flight tool call carries a `CancellationToken`.
//! * **Audit** — every call is written to the tamper-evident `security_audit.db`.

pub mod tool;
pub mod policy;
pub mod sanitizer;
pub mod bus;
pub mod audit;

// Re-export primary API surface.
pub use tool::{KageTool, ToolRequest, ToolResponse, ToolError};
pub use policy::{PolicyEngine, PermissionTier, PolicyDecision, PolicyContext, AuditFailurePolicy};
pub use sanitizer::SecretSanitizer;
pub use bus::{ToolBus, PartialPolicyContext};
pub use audit::{AuditSink, AuditVerifier, AuditReader, CanonicalAuditRecord, CanonicalAuditEntry, ActorType, AuditStatus, AuditError as CoreAuditError};
