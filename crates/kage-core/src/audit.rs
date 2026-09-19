//! Decoupled `AuditSink`, `AuditVerifier`, and `AuditReader` traits (KAGE-SEC-003, INV-05).
//!
//! Enforces Invariant 05: every AI tool execution is adjudicated by the ToolBus
//! and recorded in a tamper-evident audit ledger.
//!
//! # Two-Stage Fail-Closed Audit Lifecycle
//! For mutating or privileged actions (Tier >= 2), an `AuditStatus::Started` intent
//! record MUST be committed before the tool execution occurs. If the intent write fails,
//! execution is aborted immediately without any browser side-effects.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Domain errors originating from audit sinks.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuditError {
    #[error("audit sink unavailable: {0}")]
    Unavailable(String),

    #[error("hash chain broken at sequence={sequence}: expected {expected}, got {actual}")]
    ChainBroken {
        sequence: u64,
        expected: String,
        actual: String,
    },

    #[error("concurrency collision: {0}")]
    ConcurrencyCollision(String),

    #[error("storage failure: {0}")]
    Storage(String),
}

/// Actor class originating or authorizing the tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
    System,
}

impl std::fmt::Display for ActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorType::User => write!(f, "user"),
            ActorType::Agent => write!(f, "agent"),
            ActorType::System => write!(f, "system"),
        }
    }
}

/// Execution status recorded in the audit entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    /// Intent to execute logged before privileged mutation (Fail-Closed prerequisite).
    Started,
    /// Execution completed successfully.
    Success,
    /// Rejected by policy or missing user confirmation.
    Denied,
    /// Aborted because audit commit failed.
    FailedClosed,
    /// Aborted by human "Take Control" / STOP token.
    Cancelled,
    /// Tool returned runtime error.
    Error,
    /// Orphaned intent record reconciled on startup (process crash / storage crash before completion).
    Unresolved,
}

impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditStatus::Started => write!(f, "started"),
            AuditStatus::Success => write!(f, "success"),
            AuditStatus::Denied => write!(f, "denied"),
            AuditStatus::FailedClosed => write!(f, "failed_closed"),
            AuditStatus::Cancelled => write!(f, "cancelled"),
            AuditStatus::Error => write!(f, "error"),
            AuditStatus::Unresolved => write!(f, "unresolved"),
        }
    }
}

impl AuditStatus {
    /// Returns true if this status represents a terminal execution outcome.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AuditStatus::Success
                | AuditStatus::Denied
                | AuditStatus::FailedClosed
                | AuditStatus::Cancelled
                | AuditStatus::Error
                | AuditStatus::Unresolved
        )
    }
}

/// Canonical audit record submitted to [`AuditSink::append`].
///
/// Contains rich provenance for browser control plane and agent operations.
/// Sensitive values (passwords, raw cookies, Authorization headers) must never be stored raw;
/// only cryptographic digests (`args_digest`, `result_digest`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalAuditRecord {
    /// Monotonic sequence number (assigned by AuditSink or passed sequentially).
    pub sequence: Option<u64>,
    /// ISO 8601 timestamp in UTC.
    pub timestamp: String,
    /// Unique correlation ID matching `ToolRequest::request_id`.
    pub request_id: String,
    /// Optional parent request ID for agent sub-tasks or chained plans.
    pub parent_request_id: Option<String>,
    /// Caller identifier (e.g. "ai_subsystem", "plugin:devtools").
    pub caller: String,
    /// Actor class: user, agent, or system.
    pub actor: ActorType,
    /// Tool identifier (e.g. "dom.read_node", "page.navigate").
    pub tool_id: String,
    /// Capability or permission scope required.
    pub capability: String,
    /// Isolated browser profile identifier.
    pub profile_id: Option<String>,
    /// Associated browser tab identifier.
    pub tab_id: Option<String>,
    /// Exact Chromium target identifier.
    pub target_id: Option<String>,
    /// Active CDP multiplexer session identifier.
    pub session_id: Option<String>,
    /// Web origin (e.g. "https://github.com").
    pub origin: Option<String>,
    /// Permission tier (1–4).
    pub tier: u8,
    /// Policy decision verb ("allow", "require_confirmation", "deny").
    pub policy_decision: String,
    /// Confirmation ID if explicit human approval was granted.
    pub confirmation_id: Option<String>,
    /// SHA-256 digest of the sanitized input arguments JSON.
    pub args_digest: String,
    /// SHA-256 digest of the sanitized result JSON (if execution succeeded).
    pub result_digest: Option<String>,
    /// Outcome status.
    pub status: AuditStatus,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Error code if the tool or policy failed.
    pub error_code: Option<String>,
    /// Unique KAGE desktop host process instance ID.
    pub host_instance_id: Option<String>,
    /// SHA-256 hash of previous row in hash chain (assigned by sink).
    pub prev_hash: Option<String>,
}

/// A stored audit entry retrieved from the audit sink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalAuditEntry {
    pub id: i64,
    pub sequence: u64,
    pub timestamp: String,
    pub request_id: String,
    pub parent_request_id: Option<String>,
    pub caller: String,
    pub actor: String,
    pub tool_id: String,
    pub capability: String,
    pub profile_id: Option<String>,
    pub tab_id: Option<String>,
    pub target_id: Option<String>,
    pub session_id: Option<String>,
    pub host_instance_id: Option<String>,
    pub origin: Option<String>,
    pub tier: u8,
    pub policy_decision: String,
    pub confirmation_id: Option<String>,
    pub args_digest: String,
    pub result_digest: Option<String>,
    pub status: String,
    pub duration_ms: u64,
    pub error_code: Option<String>,
    pub prev_hash: String,
    pub row_hash: String,
}

/// Compute SHA-256 hex digest for arbitrary JSON values.
pub fn digest_json(value: &serde_json::Value) -> String {
    let canonical = serde_json::to_string(value).unwrap_or_default();
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// Primary append-only audit ingestion sink.
/// Used by [`crate::bus::ToolBus`] to record intents and completions.
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Append a canonical record into the hash chain.
    /// Returns the assigned monotonic sequence number or an [`AuditError`].
    async fn append(&self, record: CanonicalAuditRecord) -> Result<u64, AuditError>;
}

/// Audit verification interface.
/// Used by DevTools security panel and CI verification gates.
#[async_trait]
pub trait AuditVerifier: Send + Sync {
    /// Verify the complete cryptographic hash chain from genesis to tail.
    async fn verify_chain(&self) -> Result<(), AuditError>;
}

/// Audit inspection and retrieval interface.
/// Used by UI logs, history readers, and forensics tooling.
#[async_trait]
pub trait AuditReader: Send + Sync {
    /// Retrieve recent audit log entries ordered by sequence DESC.
    async fn get_recent_records(&self, limit: usize) -> Result<Vec<CanonicalAuditEntry>, AuditError>;

    /// Reconcile orphaned/non-terminal intent records on startup.
    ///
    /// Finds any records with `status = 'started'` that lack a corresponding terminal
    /// record (e.g. from process crashes or power-cuts), appends an `Unresolved` incident
    /// record with the matching `request_id`, and returns the list of reconciled request IDs.
    async fn reconcile_unresolved_intents(&self) -> Result<Vec<String>, AuditError>;
}
