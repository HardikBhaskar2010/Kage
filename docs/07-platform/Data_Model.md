# KAGE Data Model Specification

| Document Metadata | Details |
|---|---|
| **Document ID** | KAGE-PLAT-001 |
| **Status** | Approved Core Specification |
| **Version** | v0.2.1 |
| **Last Updated** | 2026-09-16 |
| **Target Milestone** | v1.0.0 MVP |
| **Classification** | Persistence, Database Schemas & Storage Architecture |

---

## 1. Executive Summary & Storage Topology

KAGE maintains strict boundaries between transient web session data, relational developer artifacts, and native application preferences. Rather than scattering unindexed JSON files across the filesystem, KAGE utilizes a structured, ACID-compliant **SQLite 3** engine embedded in the Rust host process via `rusqlite`.

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

---

## 2. Storage Tier Demarcation

| Storage Engine | Physical Location | Concurrency Model | Content Scope |
|---|---|---|---|
| **KAGE Core Database** | `%APPDATA%/Kage/kage_data.db` | SQLite WAL Mode (Multi-Reader, Single-Writer) | Workspaces, tabs, tab groups, testing suites, snapshots, AI conversations, notes. |
| **Security Audit Database** | `%APPDATA%/Kage/security_audit.db` | SQLite Append-Only WAL Mode | Tool Bus execution logs, redacted parameters, permission decisions. |
| **Application Preferences** | `%APPDATA%/Kage/preferences.json` | Atomic Write (Temp File + Rename) | UI theme, keybindings, active workspace pointer, window bounds. |
| **Web Runtime Engine** | `%APPDATA%/Kage/cef-profile/` | Chromium Internal Multi-Process Storage | HTTP cache, IndexedDB, LocalStorage, WebSQL, Cookies. |
| **Credentials & Secrets** | OS Keychain (DPAPI / Keychain) | OS Hardware/Enclave Backed | LLM Provider API Keys (Anthropic, OpenAI, Gemini). |

---

## 3. Core SQLite Schema (`kage_data.db`)

### 3.1 Migration Management
KAGE uses `refinery` or a dedicated migration table to ensure zero data loss across upgrades:

```sql
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at_ms INTEGER NOT NULL,
    description TEXT NOT NULL
);
```

### 3.2 Workspaces & Tabs Schema

```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT 'folder',
    color_token TEXT NOT NULL DEFAULT '#DA627D',
    root_directory TEXT,
    created_at_ms INTEGER NOT NULL,
    last_accessed_ms INTEGER NOT NULL,
    is_isolated INTEGER NOT NULL DEFAULT 1,
    settings_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE tab_groups (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color_token TEXT NOT NULL,
    is_collapsed INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE TABLE tabs (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    group_id TEXT,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    favicon_url TEXT,
    scroll_x REAL NOT NULL DEFAULT 0,
    scroll_y REAL NOT NULL DEFAULT 0,
    is_pinned INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY(group_id) REFERENCES tab_groups(id) ON DELETE SET NULL
);
```

### 3.3 Testing Lab & Snapshots Schema

```sql
CREATE TABLE test_suites (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE TABLE test_cases (
    id TEXT PRIMARY KEY,
    suite_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target_url TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(suite_id) REFERENCES test_suites(id) ON DELETE CASCADE
);

CREATE TABLE test_steps (
    id TEXT PRIMARY KEY,
    test_case_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    action_type TEXT NOT NULL, -- 'click', 'type', 'navigate', 'assert'
    target_selector_json TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    FOREIGN KEY(test_case_id) REFERENCES test_cases(id) ON DELETE CASCADE
);

CREATE TABLE state_snapshots (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    dom_html_compressed BLOB NOT NULL, -- GZIP compressed outerHTML
    cookies_json TEXT NOT NULL,
    local_storage_json TEXT NOT NULL,
    session_storage_json TEXT NOT NULL,
    console_logs_json TEXT NOT NULL,
    screenshot_png BLOB,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);
```

### 3.4 AI Conversations & Threads Schema

```sql
CREATE TABLE ai_threads (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    title TEXT NOT NULL,
    model_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY(workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE TABLE ai_messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    role TEXT NOT NULL, -- 'user', 'assistant', 'system'
    content TEXT NOT NULL,
    context_pack_json TEXT, -- Serialized context pack snapshot used
    tool_calls_json TEXT,   -- Structured tool invocation records
    tokens_used INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0.0,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(thread_id) REFERENCES ai_threads(id) ON DELETE CASCADE
);
```

---

## 4. Security Audit Log Schema (`security_audit.db`)

The security audit log is maintained in a dedicated append-only SQLite file to ensure audit isolation and tamper-resistance:

```sql
CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    timestamp_ms INTEGER NOT NULL,
    actor TEXT NOT NULL,           -- 'user', 'ai_assistant', 'plugin:<id>'
    tool_name TEXT NOT NULL,       -- e.g. 'modify_css@1'
    tool_version TEXT NOT NULL,
    permission_tier INTEGER NOT NULL,
    target_origin TEXT NOT NULL,
    outcome TEXT NOT NULL,         -- 'approved', 'denied', 'timed_out'
    execution_time_ms REAL NOT NULL,
    redacted_arguments_json TEXT NOT NULL, -- All secrets scrubbed
    redacted_result_json TEXT NOT NULL
);

CREATE INDEX idx_audit_timestamp ON audit_log(timestamp_ms);
CREATE INDEX idx_audit_origin ON audit_log(target_origin);
```

---

## 5. Performance Optimization & Backup Strategy

1. **SQLite WAL Mode:** Databases are initialized with `PRAGMA journal_mode = WAL;` and `PRAGMA synchronous = NORMAL;` to support concurrent readers without blocking writes.
2. **Vacuum & Pruning:** Audit logs older than 90 days are archived or pruned based on user preference.
3. **Automated Atomic Backup:** SQLite's online backup API (`sqlite3_backup`) runs every 24 hours to create consistent snapshots in `%APPDATA%/Kage/backups/`.
