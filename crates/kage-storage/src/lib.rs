//! `kage-storage` — SQLite persistence layer for KAGE.
//!
//! # Two databases (KAGE-DATA-001)
//!
//! * **`kage_data.db`** — mutable application state: workspaces, tab history, settings.
//! * **`security_audit.db`** — append-only, SHA-256 hash-chained audit trail.
//!
//! Both databases use fully parameterized `rusqlite` queries.  No string
//! interpolation is used in SQL statements — ever.

pub mod schema;
pub mod audit;
pub mod workspace;

pub use audit::{AuditDb, AuditRecord, AuditError};
pub use workspace::{WorkspaceDb, WorkspaceRecord, WorkspaceError};
