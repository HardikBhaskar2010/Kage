//! Append-only, SHA-256 hash-chained audit database (KAGE-SEC-003).
//!
//! Every tool dispatch is recorded here with a `row_hash` computed over the
//! canonical fields and a `prev_hash` linking to the previous record — forming
//! an append-only hash chain.  Any tampering with a historical row will break
//! the chain and be detected by [`AuditDb::verify_chain`].

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::schema::apply_audit_schema;

/// Genesis hash — the `prev_hash` for the very first audit record.
const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("hash chain broken at record id={id}: expected {expected}, got {actual}")]
    ChainBroken { id: i64, expected: String, actual: String },

    #[error("schema error: {0}")]
    Schema(#[from] crate::schema::SchemaError),
}

// ---------------------------------------------------------------------------
// Record
// ---------------------------------------------------------------------------

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Tool identifier (e.g. `"dom.read_node"`).
    pub tool_id: String,
    /// Correlation ID matching the [`ToolRequest::request_id`].
    pub request_id: String,
    /// Caller identity string.
    pub caller_id: String,
    /// Numeric permission tier (1–4).
    pub tier: u8,
    /// Policy decision verb: `"allow"`, `"require_confirmation"`, or `"deny"`.
    pub decision: String,
    /// SHA-256 digest of the sanitized arguments JSON (raw args never stored).
    pub args_digest: String,
}

// ---------------------------------------------------------------------------
// AuditDb
// ---------------------------------------------------------------------------

/// Wrapper around the `security_audit.db` SQLite connection.
pub struct AuditDb {
    conn: Connection,
}

impl AuditDb {
    /// Open (or create) the audit database at `path`.
    pub fn open(path: &str) -> Result<Self, AuditError> {
        let conn = Connection::open(path)?;
        apply_audit_schema(&conn)?;
        Ok(AuditDb { conn })
    }

    /// Open an in-memory audit database (used in tests).
    pub fn open_in_memory() -> Result<Self, AuditError> {
        let conn = Connection::open_in_memory()?;
        apply_audit_schema(&conn)?;
        Ok(AuditDb { conn })
    }

    /// Append a new record, chaining it to the previous row's hash.
    pub fn append(&mut self, record: AuditRecord) -> Result<(), AuditError> {
        let prev_hash = self.head_hash()?;
        let timestamp = Utc::now().to_rfc3339();

        let row_hash = compute_row_hash(
            &timestamp,
            &record.tool_id,
            &record.request_id,
            &record.caller_id,
            record.tier,
            &record.decision,
            &record.args_digest,
            &prev_hash,
        );

        self.conn.execute(
            "INSERT INTO audit_log
               (timestamp, tool_id, request_id, caller_id, tier, decision, args_digest, prev_hash, row_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                timestamp,
                record.tool_id,
                record.request_id,
                record.caller_id,
                record.tier,
                record.decision,
                record.args_digest,
                prev_hash,
                row_hash,
            ],
        )?;
        Ok(())
    }

    /// Verify the entire hash chain from genesis to the latest record.
    ///
    /// Returns `Ok(())` if intact, or [`AuditError::ChainBroken`] on the first
    /// tampered record.
    pub fn verify_chain(&self) -> Result<(), AuditError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, tool_id, request_id, caller_id, tier,
                    decision, args_digest, prev_hash, row_hash
             FROM audit_log ORDER BY id ASC",
        )?;

        let mut expected_prev = GENESIS_HASH.to_string();

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,       // id
                row.get::<_, String>(1)?,     // timestamp
                row.get::<_, String>(2)?,     // tool_id
                row.get::<_, String>(3)?,     // request_id
                row.get::<_, String>(4)?,     // caller_id
                row.get::<_, u8>(5)?,         // tier
                row.get::<_, String>(6)?,     // decision
                row.get::<_, String>(7)?,     // args_digest
                row.get::<_, String>(8)?,     // prev_hash
                row.get::<_, String>(9)?,     // row_hash
            ))
        })?;

        for row in rows {
            let (id, ts, tool_id, req_id, caller, tier, decision, args_digest, prev_hash, stored_hash) =
                row?;

            // Check that prev_hash matches what we expect.
            if prev_hash != expected_prev {
                return Err(AuditError::ChainBroken {
                    id,
                    expected: expected_prev,
                    actual: prev_hash,
                });
            }

            // Recompute the row hash and verify it matches stored value.
            let recomputed = compute_row_hash(
                &ts, &tool_id, &req_id, &caller, tier, &decision, &args_digest, &prev_hash,
            );
            if recomputed != stored_hash {
                return Err(AuditError::ChainBroken {
                    id,
                    expected: recomputed,
                    actual: stored_hash,
                });
            }

            expected_prev = stored_hash;
        }
        Ok(())
    }

    /// SHA-256 digest of a JSON string (used to derive `args_digest`).
    pub fn digest_args(args: &serde_json::Value) -> String {
        let canonical = serde_json::to_string(args).unwrap_or_default();
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }

    // Internal: fetch the `row_hash` of the most recent record, or the genesis hash.
    fn head_hash(&self) -> Result<String, AuditError> {
        let result: rusqlite::Result<String> = self.conn.query_row(
            "SELECT row_hash FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(hash) => Ok(hash),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(GENESIS_HASH.to_string()),
            Err(e) => Err(AuditError::Db(e)),
        }
    }
}

fn compute_row_hash(
    timestamp: &str,
    tool_id: &str,
    request_id: &str,
    caller_id: &str,
    tier: u8,
    decision: &str,
    args_digest: &str,
    prev_hash: &str,
) -> String {
    let canonical = format!(
        "{timestamp}|{tool_id}|{request_id}|{caller_id}|{tier}|{decision}|{args_digest}|{prev_hash}"
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_record(id: &str) -> AuditRecord {
        AuditRecord {
            tool_id: "dom.read_node".into(),
            request_id: id.into(),
            caller_id: "ai_subsystem".into(),
            tier: 1,
            decision: "allow".into(),
            args_digest: AuditDb::digest_args(&json!({ "nodeId": 42 })),
        }
    }

    #[test]
    fn append_and_verify_single_record() {
        let mut db = AuditDb::open_in_memory().unwrap();
        db.append(sample_record("req-001")).unwrap();
        db.verify_chain().unwrap();
    }

    #[test]
    fn append_multiple_and_verify_chain() {
        let mut db = AuditDb::open_in_memory().unwrap();
        for i in 0..10 {
            db.append(sample_record(&format!("req-{i:03}"))).unwrap();
        }
        db.verify_chain().unwrap();
    }

    #[test]
    fn tampered_record_breaks_chain() {
        let mut db = AuditDb::open_in_memory().unwrap();
        db.append(sample_record("req-001")).unwrap();
        db.append(sample_record("req-002")).unwrap();

        // Tamper directly with the first record's decision field.
        db.conn
            .execute(
                "UPDATE audit_log SET decision = 'allow_tampered' WHERE id = 1",
                [],
            )
            .unwrap();

        let result = db.verify_chain();
        assert!(
            matches!(result, Err(AuditError::ChainBroken { .. })),
            "Chain verification must detect tampering"
        );
    }
}
