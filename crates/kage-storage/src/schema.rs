//! SQLite schema bootstrap for both KAGE databases.
//!
//! Call [`apply_kage_data_schema`] after opening `kage_data.db` and
//! [`apply_audit_schema`] after opening `security_audit.db`.

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("schema migration failed: {0}")]
    Migration(#[from] rusqlite::Error),
}

/// Bootstrap `kage_data.db` with all application tables.
pub fn apply_kage_data_schema(conn: &Connection) -> Result<(), SchemaError> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS workspaces (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            metadata    TEXT NOT NULL DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS tabs (
            id           TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
            url          TEXT NOT NULL,
            title        TEXT NOT NULL DEFAULT '',
            opened_at    TEXT NOT NULL,
            closed_at    TEXT,
            is_active    INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;
    Ok(())
}

/// Bootstrap `security_audit.db` with the append-only audit table.
///
/// The `prev_hash` column forms the SHA-256 hash chain; the genesis row has
/// `prev_hash = '0000...0000'` (64 zeros).
pub fn apply_audit_schema(conn: &Connection) -> Result<(), SchemaError> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS audit_log (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            sequence          INTEGER NOT NULL UNIQUE,
            timestamp         TEXT    NOT NULL,
            request_id        TEXT    NOT NULL,  -- Non-unique: allows two-stage intent (Started) and completion records
            parent_request_id TEXT,
            caller_id         TEXT    NOT NULL,
            actor             TEXT    NOT NULL,
            tool_id           TEXT    NOT NULL,
            capability        TEXT    NOT NULL,
            profile_id        TEXT,
            tab_id            TEXT,
            target_id         TEXT,
            session_id        TEXT,
            host_instance_id  TEXT,
            origin            TEXT,
            tier              INTEGER NOT NULL,
            decision          TEXT    NOT NULL,
            confirmation_id   TEXT,
            args_digest       TEXT    NOT NULL,  -- SHA-256 of sanitized args JSON
            result_digest     TEXT,              -- SHA-256 of sanitized output JSON
            status            TEXT    NOT NULL,  -- started, success, denied, failed_closed, cancelled, error
            duration_ms       INTEGER NOT NULL,
            error_code        TEXT,
            prev_hash         TEXT    NOT NULL,  -- SHA-256 of previous row (hash chain)
            row_hash          TEXT    NOT NULL   -- SHA-256 of this row's canonical fields
        );

        CREATE INDEX IF NOT EXISTS idx_audit_sequence ON audit_log (sequence);
        CREATE INDEX IF NOT EXISTS idx_audit_request_id ON audit_log (request_id);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log (timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_tool_id ON audit_log (tool_id);
        ",
    )?;
    Ok(())
}
