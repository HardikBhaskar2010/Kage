use chrono::Utc;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub id: i64,
    pub timestamp: String,
    pub tool_name: String,
    pub tier: i32,
    pub caller: String,
    pub sanitized_args: String,
    pub status: String,
    pub duration_ms: i64,
    pub payload_hash: String,
    pub previous_record_hash: String,
    pub chain_hash: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerificationResult {
    Pass { total_records_verified: usize },
    IntegrityFailure { record_id: i64, reason: String },
}

pub struct AuditLogger {
    conn: Mutex<Connection>,
    last_chain_hash: Mutex<String>,
}

impl AuditLogger {
    pub fn new_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    pub fn new_at_path<P: AsRef<Path>>(path: P) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS security_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                tier INTEGER NOT NULL,
                caller TEXT NOT NULL,
                sanitized_args TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                payload_hash TEXT NOT NULL,
                previous_record_hash TEXT NOT NULL,
                chain_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_audit_tool ON security_audit(tool_name);
            CREATE INDEX IF NOT EXISTS idx_audit_status ON security_audit(status);",
        )?;

        // Find last chain hash if table already contains entries
        let last_hash: String = conn
            .query_row(
                "SELECT chain_hash FROM security_audit ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| GENESIS_HASH.to_string());

        Ok(Self {
            conn: Mutex::new(conn),
            last_chain_hash: Mutex::new(last_hash),
        })
    }

    pub fn record(
        &self,
        tool_name: &str,
        tier: i32,
        caller: &str,
        sanitized_args: &str,
        status: &str,
        duration_ms: i64,
    ) -> Result<AuditRecord, rusqlite::Error> {
        let timestamp = Utc::now().to_rfc3339();

        // 1. Calculate payload hash
        let payload_hash = Self::calculate_hash(sanitized_args.as_bytes());

        // 2. Lock chain state to ensure unbroken serial link
        let mut last_hash_guard = self.last_chain_hash.lock().unwrap();
        let prev_hash = last_hash_guard.clone();

        // 3. Compute chain hash: SHA256(timestamp || tool_name || status || payload_hash || prev_hash)
        let mut hasher = Sha256::new();
        hasher.update(timestamp.as_bytes());
        hasher.update(tool_name.as_bytes());
        hasher.update(status.as_bytes());
        hasher.update(payload_hash.as_bytes());
        hasher.update(prev_hash.as_bytes());
        let chain_hash = format!("{:x}", hasher.finalize());

        // 4. Insert record
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO security_audit (
                timestamp, tool_name, tier, caller, sanitized_args, status, duration_ms,
                payload_hash, previous_record_hash, chain_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                timestamp,
                tool_name,
                tier,
                caller,
                sanitized_args,
                status,
                duration_ms,
                payload_hash,
                prev_hash,
                chain_hash
            ],
        )?;

        let id = conn.last_insert_rowid();
        *last_hash_guard = chain_hash.clone();

        Ok(AuditRecord {
            id,
            timestamp,
            tool_name: tool_name.to_string(),
            tier,
            caller: caller.to_string(),
            sanitized_args: sanitized_args.to_string(),
            status: status.to_string(),
            duration_ms,
            payload_hash,
            previous_record_hash: prev_hash,
            chain_hash,
        })
    }

    pub fn get_all_records(&self) -> Result<Vec<AuditRecord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, tool_name, tier, caller, sanitized_args, status, duration_ms,
                    payload_hash, previous_record_hash, chain_hash
             FROM security_audit ORDER BY id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(AuditRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                tool_name: row.get(2)?,
                tier: row.get(3)?,
                caller: row.get(4)?,
                sanitized_args: row.get(5)?,
                status: row.get(6)?,
                duration_ms: row.get(7)?,
                payload_hash: row.get(8)?,
                previous_record_hash: row.get(9)?,
                chain_hash: row.get(10)?,
            })
        })?;

        let mut records = Vec::new();
        for r in rows {
            records.push(r?);
        }
        Ok(records)
    }

    pub fn verify_integrity(&self) -> Result<VerificationResult, rusqlite::Error> {
        let records = self.get_all_records()?;
        let mut expected_prev_hash = GENESIS_HASH.to_string();

        for record in &records {
            // Check previous hash link
            if record.previous_record_hash != expected_prev_hash {
                return Ok(VerificationResult::IntegrityFailure {
                    record_id: record.id,
                    reason: format!(
                        "Broken chain link: expected previous hash '{}', found '{}'",
                        expected_prev_hash, record.previous_record_hash
                    ),
                });
            }

            // Recompute payload hash
            let computed_payload_hash = Self::calculate_hash(record.sanitized_args.as_bytes());
            if record.payload_hash != computed_payload_hash {
                return Ok(VerificationResult::IntegrityFailure {
                    record_id: record.id,
                    reason: format!(
                        "Payload tampering detected: stored '{}', computed '{}'",
                        record.payload_hash, computed_payload_hash
                    ),
                });
            }

            // Recompute chain hash
            let mut hasher = Sha256::new();
            hasher.update(record.timestamp.as_bytes());
            hasher.update(record.tool_name.as_bytes());
            hasher.update(record.status.as_bytes());
            hasher.update(computed_payload_hash.as_bytes());
            hasher.update(expected_prev_hash.as_bytes());
            let computed_chain_hash = format!("{:x}", hasher.finalize());

            if record.chain_hash != computed_chain_hash {
                return Ok(VerificationResult::IntegrityFailure {
                    record_id: record.id,
                    reason: format!(
                        "Chain hash mismatch: stored '{}', computed '{}'",
                        record.chain_hash, computed_chain_hash
                    ),
                });
            }

            expected_prev_hash = record.chain_hash.clone();
        }

        Ok(VerificationResult::Pass {
            total_records_verified: records.len(),
        })
    }

    fn calculate_hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }
}
