//! Append-only, SHA-256 hash-chained audit database (KAGE-SEC-003, INV-05).
//!
//! Enforces Invariant 05: every tool dispatch is recorded here with a `row_hash`
//! computed over canonical fields and a `prev_hash` linking to the previous record.
//! All appends are protected by a serialized mutex lock preventing concurrent
//! hash-chain race conditions across concurrent agent tasks.
//!
//! # Tamper-Evident Ledger vs. Immutable Storage
//! The SHA-256 hash chain provides cryptographically verifiable **tamper-evidence**
//! across all preserved records: any in-place mutation, row insertion, or internal
//! reordering breaks the chain and is detected by [`AuditVerifier::verify_chain`].
//! Note: Purely local hash-chains detect tampering within the retained history, but
//! cannot prevent external tail truncation or entire database replacement without an
//! external truth anchor (e.g. OS-protected signed checkpoints or remote witness logs,
//! planned for Phase 11).

use std::sync::Arc;
use async_trait::async_trait;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;

use kage_core::audit::{
    AuditError as CoreAuditError, AuditReader, AuditSink, AuditVerifier, CanonicalAuditEntry,
    CanonicalAuditRecord,
};
use crate::schema::{apply_audit_schema, SchemaError};

/// Genesis hash — the `prev_hash` for the very first audit record (sequence = 1).
pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("hash chain broken at sequence={sequence}: expected {expected}, got {actual}")]
    ChainBroken {
        sequence: u64,
        expected: String,
        actual: String,
    },

    #[error("schema error: {0}")]
    Schema(#[from] SchemaError),
}

impl From<AuditError> for CoreAuditError {
    fn from(err: AuditError) -> Self {
        match err {
            AuditError::ChainBroken { sequence, expected, actual } => {
                CoreAuditError::ChainBroken { sequence, expected, actual }
            }
            other => CoreAuditError::Storage(other.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// AuditDb
// ---------------------------------------------------------------------------

/// Thread-safe wrapper around the `security_audit.db` SQLite connection.
///
/// Employs serialized append transactions via an internal mutex to guarantee
/// strictly monotonic sequence numbers and eliminate hash-chain race conditions.
#[derive(Clone)]
pub struct AuditDb {
    conn: Arc<Mutex<Connection>>,
}

impl AuditDb {
    /// Open (or create) the audit database at `path`.
    pub fn open(path: &str) -> Result<Self, AuditError> {
        let conn = Connection::open(path)?;
        apply_audit_schema(&conn)?;
        Ok(AuditDb {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory audit database (used in tests).
    pub fn open_in_memory() -> Result<Self, AuditError> {
        let conn = Connection::open_in_memory()?;
        apply_audit_schema(&conn)?;
        Ok(AuditDb {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Compute canonical SHA-256 row hash over all provenance fields.
    pub fn compute_canonical_row_hash(
        sequence: u64,
        timestamp: &str,
        request_id: &str,
        parent_request_id: Option<&str>,
        caller_id: &str,
        actor: &str,
        tool_id: &str,
        capability: &str,
        profile_id: Option<&str>,
        tab_id: Option<&str>,
        target_id: Option<&str>,
        session_id: Option<&str>,
        host_instance_id: Option<&str>,
        origin: Option<&str>,
        tier: u8,
        decision: &str,
        confirmation_id: Option<&str>,
        args_digest: &str,
        result_digest: Option<&str>,
        status: &str,
        duration_ms: u64,
        error_code: Option<&str>,
        prev_hash: &str,
    ) -> String {
        let canonical = format!(
            "{sequence}|{timestamp}|{request_id}|{}|{caller_id}|{actor}|{tool_id}|{capability}|{}|{}|{}|{}|{}|{}|{tier}|{decision}|{}|{args_digest}|{}|{status}|{duration_ms}|{}|{prev_hash}",
            parent_request_id.unwrap_or(""),
            profile_id.unwrap_or(""),
            tab_id.unwrap_or(""),
            target_id.unwrap_or(""),
            session_id.unwrap_or(""),
            host_instance_id.unwrap_or(""),
            origin.unwrap_or(""),
            confirmation_id.unwrap_or(""),
            result_digest.unwrap_or(""),
            error_code.unwrap_or("")
        );
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }

    /// Inherent convenience delegator to [`AuditVerifier::verify_chain`].
    pub async fn verify_chain(&self) -> Result<(), CoreAuditError> {
        AuditVerifier::verify_chain(self).await
    }

    /// Inherent convenience delegator to [`AuditReader::get_recent_records`].
    pub async fn get_recent_records(&self, limit: usize) -> Result<Vec<CanonicalAuditEntry>, CoreAuditError> {
        AuditReader::get_recent_records(self, limit).await
    }

    /// Inherent convenience delegator to [`AuditReader::reconcile_unresolved_intents`].
    pub async fn reconcile_unresolved_intents(&self) -> Result<Vec<String>, CoreAuditError> {
        AuditReader::reconcile_unresolved_intents(self).await
    }
}

#[async_trait]
impl AuditSink for AuditDb {
    /// Serialized atomic append: acquires mutex, determines next sequence and tail hash,
    /// computes row hash, and inserts the record.
    async fn append(&self, record: CanonicalAuditRecord) -> Result<u64, CoreAuditError> {
        let conn = self.conn.lock().await;

        // 1. Obtain current tail within serialized lock
        let (last_seq, last_hash): (u64, String) = match conn.query_row(
            "SELECT sequence, row_hash FROM audit_log ORDER BY sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ) {
            Ok((seq, hash)) => (seq, hash),
            Err(rusqlite::Error::QueryReturnedNoRows) => (0, GENESIS_HASH.to_string()),
            Err(e) => return Err(CoreAuditError::Storage(e.to_string())),
        };

        let sequence = last_seq + 1;
        let prev_hash = last_hash;

        // 2. Compute canonical cryptographic row hash
        let row_hash = AuditDb::compute_canonical_row_hash(
            sequence,
            &record.timestamp,
            &record.request_id,
            record.parent_request_id.as_deref(),
            &record.caller,
            &record.actor.to_string(),
            &record.tool_id,
            &record.capability,
            record.profile_id.as_deref(),
            record.tab_id.as_deref(),
            record.target_id.as_deref(),
            record.session_id.as_deref(),
            record.host_instance_id.as_deref(),
            record.origin.as_deref(),
            record.tier,
            &record.policy_decision,
            record.confirmation_id.as_deref(),
            &record.args_digest,
            record.result_digest.as_deref(),
            &record.status.to_string(),
            record.duration_ms,
            record.error_code.as_deref(),
            &prev_hash,
        );

        // 3. Insert and commit atomic record
        conn.execute(
            "INSERT INTO audit_log (
                sequence, timestamp, request_id, parent_request_id, caller_id,
                actor, tool_id, capability, profile_id, tab_id, target_id, session_id, host_instance_id, origin,
                tier, decision, confirmation_id, args_digest, result_digest,
                status, duration_ms, error_code, prev_hash, row_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                sequence,
                record.timestamp,
                record.request_id,
                record.parent_request_id,
                record.caller,
                record.actor.to_string(),
                record.tool_id,
                record.capability,
                record.profile_id,
                record.tab_id,
                record.target_id,
                record.session_id,
                record.host_instance_id,
                record.origin,
                record.tier,
                record.policy_decision,
                record.confirmation_id,
                record.args_digest,
                record.result_digest,
                record.status.to_string(),
                record.duration_ms,
                record.error_code,
                prev_hash,
                row_hash,
            ],
        ).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        Ok(sequence)
    }
}

#[async_trait]
impl AuditVerifier for AuditDb {
    /// Verify the complete cryptographic hash chain from genesis to the latest record.
    ///
    /// Validates strictly monotonic sequence numbers, `prev_hash` linkage, and row hashes.
    async fn verify_chain(&self) -> Result<(), CoreAuditError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, sequence, timestamp, request_id, parent_request_id,
                    caller_id, actor, tool_id, capability, profile_id, tab_id,
                    target_id, session_id, host_instance_id, origin,
                    tier, decision, confirmation_id, args_digest, result_digest,
                    status, duration_ms, error_code, prev_hash, row_hash
             FROM audit_log ORDER BY sequence ASC",
        ).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        let mut expected_prev = GENESIS_HASH.to_string();
        let mut expected_seq: u64 = 1;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,                 // id
                row.get::<_, u64>(1)?,                 // sequence
                row.get::<_, String>(2)?,              // timestamp
                row.get::<_, String>(3)?,              // request_id
                row.get::<_, Option<String>>(4)?,      // parent_request_id
                row.get::<_, String>(5)?,              // caller_id
                row.get::<_, String>(6)?,              // actor
                row.get::<_, String>(7)?,              // tool_id
                row.get::<_, String>(8)?,              // capability
                row.get::<_, Option<String>>(9)?,      // profile_id
                row.get::<_, Option<String>>(10)?,     // tab_id
                row.get::<_, Option<String>>(11)?,     // target_id
                row.get::<_, Option<String>>(12)?,     // session_id
                row.get::<_, Option<String>>(13)?,     // host_instance_id
                row.get::<_, Option<String>>(14)?,     // origin
                row.get::<_, u8>(15)?,                 // tier
                row.get::<_, String>(16)?,             // decision
                row.get::<_, Option<String>>(17)?,     // confirmation_id
                row.get::<_, String>(18)?,             // args_digest
                row.get::<_, Option<String>>(19)?,     // result_digest
                row.get::<_, String>(20)?,             // status
                row.get::<_, u64>(21)?,                // duration_ms
                row.get::<_, Option<String>>(22)?,     // error_code
                row.get::<_, String>(23)?,             // prev_hash
                row.get::<_, String>(24)?,             // row_hash
            ))
        }).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        for row in rows {
            let (
                _id,
                sequence,
                timestamp,
                request_id,
                parent_request_id,
                caller_id,
                actor,
                tool_id,
                capability,
                profile_id,
                tab_id,
                target_id,
                session_id,
                host_instance_id,
                origin,
                tier,
                decision,
                confirmation_id,
                args_digest,
                result_digest,
                status,
                duration_ms,
                error_code,
                prev_hash,
                stored_hash,
            ) = row.map_err(|e| CoreAuditError::Storage(e.to_string()))?;

            // 1. Verify sequence monotonicity
            if sequence != expected_seq {
                return Err(CoreAuditError::ChainBroken {
                    sequence,
                    expected: format!("sequence {expected_seq}"),
                    actual: format!("sequence {sequence}"),
                });
            }

            // 2. Verify prev_hash matches prior row's row_hash
            if prev_hash != expected_prev {
                return Err(CoreAuditError::ChainBroken {
                    sequence,
                    expected: expected_prev,
                    actual: prev_hash,
                });
            }

            // 3. Recompute canonical row hash
            let recomputed = AuditDb::compute_canonical_row_hash(
                sequence,
                &timestamp,
                &request_id,
                parent_request_id.as_deref(),
                &caller_id,
                &actor,
                &tool_id,
                &capability,
                profile_id.as_deref(),
                tab_id.as_deref(),
                target_id.as_deref(),
                session_id.as_deref(),
                host_instance_id.as_deref(),
                origin.as_deref(),
                tier,
                &decision,
                confirmation_id.as_deref(),
                &args_digest,
                result_digest.as_deref(),
                &status,
                duration_ms,
                error_code.as_deref(),
                &prev_hash,
            );

            if recomputed != stored_hash {
                return Err(CoreAuditError::ChainBroken {
                    sequence,
                    expected: recomputed,
                    actual: stored_hash,
                });
            }

            expected_prev = stored_hash;
            expected_seq += 1;
        }

        Ok(())
    }
}

#[async_trait]
impl AuditReader for AuditDb {
    /// Retrieve recent audit log entries ordered by sequence DESC.
    async fn get_recent_records(&self, limit: usize) -> Result<Vec<CanonicalAuditEntry>, CoreAuditError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT id, sequence, timestamp, request_id, parent_request_id,
                    caller_id, actor, tool_id, capability, profile_id, tab_id,
                    target_id, session_id, host_instance_id, origin,
                    tier, decision, confirmation_id, args_digest, result_digest,
                    status, duration_ms, error_code, prev_hash, row_hash
             FROM audit_log ORDER BY sequence DESC LIMIT ?1",
        ).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(CanonicalAuditEntry {
                id: row.get(0)?,
                sequence: row.get(1)?,
                timestamp: row.get(2)?,
                request_id: row.get(3)?,
                parent_request_id: row.get(4)?,
                caller: row.get(5)?,
                actor: row.get(6)?,
                tool_id: row.get(7)?,
                capability: row.get(8)?,
                profile_id: row.get(9)?,
                tab_id: row.get(10)?,
                target_id: row.get(11)?,
                session_id: row.get(12)?,
                host_instance_id: row.get(13)?,
                origin: row.get(14)?,
                tier: row.get(15)?,
                policy_decision: row.get(16)?,
                confirmation_id: row.get(17)?,
                args_digest: row.get(18)?,
                result_digest: row.get(19)?,
                status: row.get(20)?,
                duration_ms: row.get(21)?,
                error_code: row.get(22)?,
                prev_hash: row.get(23)?,
                row_hash: row.get(24)?,
            })
        }).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| CoreAuditError::Storage(e.to_string()))?);
        }
        Ok(entries)
    }

    /// Reconcile orphaned/non-terminal intent records on startup.
    ///
    /// Finds any records with `status = 'started'` that lack a corresponding terminal
    /// record (e.g. from process crashes or power-cuts), appends an `Unresolved` incident
    /// record with the matching `request_id`, and returns the list of reconciled request IDs.
    async fn reconcile_unresolved_intents(&self) -> Result<Vec<String>, CoreAuditError> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT DISTINCT request_id, caller_id, actor, tool_id, capability, profile_id, tab_id, target_id, session_id, host_instance_id, origin, tier, args_digest
             FROM audit_log a
             WHERE status = 'started'
               AND NOT EXISTS (
                   SELECT 1 FROM audit_log b
                   WHERE b.request_id = a.request_id
                     AND b.status IN ('success', 'error', 'cancelled', 'denied', 'failed_closed', 'unresolved')
               )
             ORDER BY sequence ASC",
        ).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        let orphaned_rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,             // request_id
                row.get::<_, String>(1)?,             // caller_id
                row.get::<_, String>(2)?,             // actor
                row.get::<_, String>(3)?,             // tool_id
                row.get::<_, String>(4)?,             // capability
                row.get::<_, Option<String>>(5)?,     // profile_id
                row.get::<_, Option<String>>(6)?,     // tab_id
                row.get::<_, Option<String>>(7)?,     // target_id
                row.get::<_, Option<String>>(8)?,     // session_id
                row.get::<_, Option<String>>(9)?,     // host_instance_id
                row.get::<_, Option<String>>(10)?,    // origin
                row.get::<_, u8>(11)?,                // tier
                row.get::<_, String>(12)?,            // args_digest
            ))
        }).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

        let mut orphaned = Vec::new();
        for row in orphaned_rows {
            orphaned.push(row.map_err(|e| CoreAuditError::Storage(e.to_string()))?);
        }

        let mut reconciled = Vec::new();
        for (request_id, caller_id, actor, tool_id, capability, profile_id, tab_id, target_id, session_id, host_instance_id, origin, tier, args_digest) in orphaned {
            // Get current tail within lock
            let (last_seq, last_hash): (u64, String) = match conn.query_row(
                "SELECT sequence, row_hash FROM audit_log ORDER BY sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ) {
                Ok((seq, hash)) => (seq, hash),
                Err(rusqlite::Error::QueryReturnedNoRows) => (0, GENESIS_HASH.to_string()),
                Err(e) => return Err(CoreAuditError::Storage(e.to_string())),
            };

            let sequence = last_seq + 1;
            let timestamp = chrono::Utc::now().to_rfc3339();
            let prev_hash = last_hash;
            let status = "unresolved";
            let decision = "reconciled_orphan";
            let error_code = "ORPHANED_INTENT_RECONCILED";

            let row_hash = AuditDb::compute_canonical_row_hash(
                sequence,
                &timestamp,
                &request_id,
                None,
                &caller_id,
                &actor,
                &tool_id,
                &capability,
                profile_id.as_deref(),
                tab_id.as_deref(),
                target_id.as_deref(),
                session_id.as_deref(),
                host_instance_id.as_deref(),
                origin.as_deref(),
                tier,
                decision,
                None,
                &args_digest,
                None,
                status,
                0,
                Some(error_code),
                &prev_hash,
            );

            conn.execute(
                "INSERT INTO audit_log (
                    sequence, timestamp, request_id, parent_request_id, caller_id,
                    actor, tool_id, capability, profile_id, tab_id, target_id, session_id, host_instance_id, origin,
                    tier, decision, confirmation_id, args_digest, result_digest,
                    status, duration_ms, error_code, prev_hash, row_hash
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
                params![
                    sequence,
                    timestamp,
                    request_id,
                    None::<String>,
                    caller_id,
                    actor,
                    tool_id,
                    capability,
                    profile_id,
                    tab_id,
                    target_id,
                    session_id,
                    host_instance_id,
                    origin,
                    tier,
                    decision,
                    None::<String>,
                    args_digest,
                    None::<String>,
                    status,
                    0u64,
                    Some(error_code),
                    prev_hash,
                    row_hash,
                ],
            ).map_err(|e| CoreAuditError::Storage(e.to_string()))?;

            reconciled.push(request_id);
        }

        Ok(reconciled)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kage_core::audit::{ActorType, AuditStatus};

    fn sample_record(id: &str) -> CanonicalAuditRecord {
        CanonicalAuditRecord {
            sequence: None,
            timestamp: "2026-09-18T20:00:00Z".into(),
            request_id: id.into(),
            parent_request_id: None,
            caller: "ai_subsystem".into(),
            actor: ActorType::Agent,
            tool_id: "dom.read_node".into(),
            capability: "ReadOnly".into(),
            profile_id: Some("default".into()),
            tab_id: Some("tab_1".into()),
            target_id: Some("target_01".into()),
            session_id: Some("session_01".into()),
            origin: Some("https://example.com".into()),
            tier: 1,
            policy_decision: "allow".into(),
            confirmation_id: None,
            args_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            result_digest: Some("ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb".into()),
            status: AuditStatus::Success,
            duration_ms: 12,
            error_code: None,
            host_instance_id: Some("inst-test-001".into()),
            prev_hash: None,
        }
    }

    #[tokio::test]
    async fn append_and_verify_single_record() {
        let db = AuditDb::open_in_memory().unwrap();
        let seq = db.append(sample_record("req-001")).await.unwrap();
        assert_eq!(seq, 1);
        db.verify_chain().await.unwrap();
    }

    #[tokio::test]
    async fn append_multiple_and_verify_chain() {
        let db = AuditDb::open_in_memory().unwrap();
        for i in 1..=10 {
            let seq = db.append(sample_record(&format!("req-{i:03}"))).await.unwrap();
            assert_eq!(seq, i);
        }
        db.verify_chain().await.unwrap();

        let recent = db.get_recent_records(5).await.unwrap();
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].sequence, 10);
        assert_eq!(recent[4].sequence, 6);
    }

    #[tokio::test]
    async fn tampered_record_breaks_chain() {
        let db = AuditDb::open_in_memory().unwrap();
        db.append(sample_record("req-001")).await.unwrap();
        db.append(sample_record("req-002")).await.unwrap();

        // Tamper directly with the first record's decision field.
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "UPDATE audit_log SET decision = 'tampered_decision' WHERE sequence = 1",
                [],
            )
            .unwrap();
        }

        let result = db.verify_chain().await;
        assert!(
            matches!(result, Err(CoreAuditError::ChainBroken { sequence: 1, .. })),
            "Chain verification must detect tampering in record sequence=1"
        );
    }

    #[tokio::test]
    async fn reconcile_orphaned_intent_creates_terminal_record_and_preserves_chain() {
        let db = AuditDb::open_in_memory().unwrap();

        // 1. Append a normal complete transaction (Intent + Success)
        let mut rec1 = sample_record("req-normal");
        rec1.status = AuditStatus::Started;
        db.append(rec1).await.unwrap();

        let mut rec1_done = sample_record("req-normal");
        rec1_done.status = AuditStatus::Success;
        db.append(rec1_done).await.unwrap();

        // 2. Simulate a crash / broken shutdown: Intent logged, but process died before completion
        let mut rec2_orphaned = sample_record("req-orphaned-crash");
        rec2_orphaned.status = AuditStatus::Started;
        db.append(rec2_orphaned).await.unwrap();

        // 3. Run startup reconciliation
        let reconciled = db.reconcile_unresolved_intents().await.unwrap();
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0], "req-orphaned-crash");

        // 4. Verify the chain is 100% cryptographically intact
        db.verify_chain().await.unwrap();

        // 5. Inspect the records
        let records = db.get_recent_records(10).await.unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].sequence, 4);
        assert_eq!(records[0].request_id, "req-orphaned-crash");
        assert_eq!(records[0].status, "unresolved");
        assert_eq!(records[0].error_code.as_deref(), Some("ORPHANED_INTENT_RECONCILED"));

        // 6. Assert idempotency: subsequent run finds 0 orphaned records
        let rerun = db.reconcile_unresolved_intents().await.unwrap();
        assert_eq!(rerun.len(), 0);
    }
}
