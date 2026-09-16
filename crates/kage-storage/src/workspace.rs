//! Workspace persistence (application state — `kage_data.db`).

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::schema::apply_kage_data_schema;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("workspace not found: {0}")]
    NotFound(String),

    #[error("schema error: {0}")]
    Schema(#[from] crate::schema::SchemaError),
}

/// A persisted browser workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: serde_json::Value,
}

/// Workspace CRUD operations backed by `kage_data.db`.
pub struct WorkspaceDb {
    conn: Connection,
}

impl WorkspaceDb {
    pub fn open(path: &str) -> Result<Self, WorkspaceError> {
        let conn = Connection::open(path)?;
        apply_kage_data_schema(&conn)?;
        Ok(WorkspaceDb { conn })
    }

    pub fn open_in_memory() -> Result<Self, WorkspaceError> {
        let conn = Connection::open_in_memory()?;
        apply_kage_data_schema(&conn)?;
        Ok(WorkspaceDb { conn })
    }

    pub fn create(&self, name: &str) -> Result<WorkspaceRecord, WorkspaceError> {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, now, now, "{}"],
        )?;
        self.get(&id)
    }

    pub fn get(&self, id: &str) -> Result<WorkspaceRecord, WorkspaceError> {
        let result = self.conn.query_row(
            "SELECT id, name, created_at, updated_at, metadata FROM workspaces WHERE id = ?1",
            params![id],
            |row| {
                Ok(WorkspaceRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    metadata: serde_json::from_str(
                        &row.get::<_, String>(4).unwrap_or_else(|_| "{}".into()),
                    )
                    .unwrap_or_default(),
                })
            },
        );
        match result {
            Ok(ws) => Ok(ws),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(WorkspaceError::NotFound(id.into())),
            Err(e) => Err(WorkspaceError::Db(e)),
        }
    }

    pub fn list(&self) -> Result<Vec<WorkspaceRecord>, WorkspaceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, metadata FROM workspaces ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkspaceRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                metadata: serde_json::from_str(
                    &row.get::<_, String>(4).unwrap_or_else(|_| "{}".into()),
                )
                .unwrap_or_default(),
            })
        })?;
        rows.map(|r| r.map_err(WorkspaceError::Db)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get_workspace() {
        let db = WorkspaceDb::open_in_memory().unwrap();
        let ws = db.create("My Workspace").unwrap();
        assert_eq!(ws.name, "My Workspace");

        let fetched = db.get(&ws.id).unwrap();
        assert_eq!(fetched.id, ws.id);
    }

    #[test]
    fn list_workspaces() {
        let db = WorkspaceDb::open_in_memory().unwrap();
        db.create("Alpha").unwrap();
        db.create("Beta").unwrap();
        let all = db.list().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn get_missing_workspace_errors() {
        let db = WorkspaceDb::open_in_memory().unwrap();
        let err = db.get("nonexistent-id").unwrap_err();
        assert!(matches!(err, WorkspaceError::NotFound(_)));
    }
}
