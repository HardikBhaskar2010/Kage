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
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   TEXT    NOT NULL,
            tool_id     TEXT    NOT NULL,
            request_id  TEXT    NOT NULL UNIQUE,
            caller_id   TEXT    NOT NULL,
            tier        INTEGER NOT NULL,
            decision    TEXT    NOT NULL,
            args_digest TEXT    NOT NULL,  -- SHA-256 of sanitized args JSON
            prev_hash   TEXT    NOT NULL,  -- SHA-256 of previous row (hash chain)
            row_hash    TEXT    NOT NULL   -- SHA-256 of this row's canonical fields
        );
        ",
    )?;
    Ok(())
}
