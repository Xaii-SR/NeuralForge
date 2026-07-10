pub mod indexer;
pub mod resolver;
pub mod search;

use crate::core::errors::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

#[derive(Default)]
pub struct DbState {
    pub conn: Mutex<Option<Connection>>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    content_hash TEXT NOT NULL,
    indexed_at INTEGER NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    modified_at INTEGER NOT NULL DEFAULT 0,
    language TEXT NOT NULL DEFAULT '',
    line_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB
);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    content = 'chunks',
    content_rowid = 'id',
    tokenize = 'porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.id, old.content);
END;

CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.id, old.content);
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS response_cache (
    prompt_hash TEXT NOT NULL,
    model TEXT NOT NULL,
    response TEXT NOT NULL,
    success_rating INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (prompt_hash, model)
);

CREATE TABLE IF NOT EXISTS agent_tasks (
    id TEXT PRIMARY KEY,
    objective TEXT NOT NULL,
    agent TEXT NOT NULL,
    task_type TEXT NOT NULL DEFAULT 'edit_file',
    file_path TEXT NOT NULL,
    status TEXT NOT NULL,
    original_content TEXT,
    proposed_content TEXT,
    risk_summary TEXT,
    verification TEXT,
    rollback TEXT,
    error TEXT,
    requirement_id TEXT,
    correlation_id TEXT,
    dag_id TEXT,
    depends_on TEXT,
    worker_id TEXT,
    retry_of TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS task_dags (
    id TEXT PRIMARY KEY,
    requirement_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    correlation_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS requirements (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    intent TEXT NOT NULL,
    acceptance_criteria TEXT NOT NULL,
    status TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS requirement_history (
    id INTEGER PRIMARY KEY,
    requirement_id TEXT NOT NULL REFERENCES requirements(id),
    version INTEGER NOT NULL,
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    intent TEXT NOT NULL,
    acceptance_criteria TEXT NOT NULL,
    changed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ledger_entries (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    correlation_id TEXT,
    requirement_id TEXT,
    task_id TEXT,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    prev_hash TEXT NOT NULL,
    entry_hash TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS evidence (
    id TEXT PRIMARY KEY,
    insertion_sequence INTEGER NOT NULL,
    task_id TEXT NOT NULL,
    correlation_id TEXT,
    kind TEXT NOT NULL,
    content TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS evidence_sequence (
    next_sequence INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS promotion_requests (
    id TEXT PRIMARY KEY,
    -- NULL means "promotion was requested with no evidence at all"; the
    -- row exists so the refusal is auditable. Non-null values reference a
    -- real evidence row (FK-enforced).
    evidence_id TEXT REFERENCES evidence(id),
    task_id TEXT NOT NULL,
    status TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    promoted_at INTEGER
);



CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL,
    language TEXT NOT NULL,
    module_path TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    visibility TEXT,
    signature TEXT,
    documentation TEXT,
    symbol_hash TEXT,
    import_source TEXT
);

CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_qualified ON symbols(qualified_name);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_symbols_module ON symbols(module_path);

CREATE TABLE IF NOT EXISTS worker_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    capabilities TEXT NOT NULL,
    reliability_score REAL NOT NULL DEFAULT 1.0,
    tasks_completed INTEGER NOT NULL DEFAULT 0,
    tasks_failed INTEGER NOT NULL DEFAULT 0
);
"#;

pub fn open_for_workspace(workspace_root: &Path) -> AppResult<Connection> {
    let db_dir = workspace_root.join(".neuralforge");
    std::fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("index.db");

    let conn = Connection::open(&db_path)
        .map_err(|e| AppError::Provider(format!("failed to open index.db: {e}")))?;
    // Audit remediation (post-Sprint-7): FK enforcement was previously
    // inherited from the bundled SQLite's compile-time default - real, but
    // accidental, and one dependency upgrade away from silently vanishing.
    // Declared explicitly so REFERENCES clauses (evidence.id from
    // promotion_requests, files.id from chunks, etc.) are guaranteed.
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| AppError::Provider(format!("failed to enable foreign keys: {e}")))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| AppError::Provider(format!("failed to init schema: {e}")))?;

    // Additive columns for DBs created before these features existed. The
    // CREATE TABLE above already includes them for brand-new DBs, so these
    // error with "duplicate column" there - that's expected, not a bug.
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN task_type TEXT NOT NULL DEFAULT 'edit_file'", []);
    // Sprint 1 (Requirement Intelligence): tasks link back to the
    // requirement that gated them. Nullable because pre-Sprint-1 task rows
    // (and run_code tasks, which stay ungated this sprint) have none.
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN requirement_id TEXT", []);
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN correlation_id TEXT", []);
    // Sprint 3 (Task DAG Planning): DAG membership for multi-task
    // decomposition. NULL for single-task flow rows - that path never
    // sets them.
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN dag_id TEXT", []);
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN depends_on TEXT", []);
    // Sprint 5 (Worker Intelligence): which worker profile a task was
    // assigned to. NULL for everything historical and for the single
    // built-in Coder flow - reliability derivation simply sees no rows.
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN worker_id TEXT", []);
    // Sprint 8 (Autonomous Reliability): retry lineage. A retry is a NEW
    // task row pointing at the attempt it replaces; attempt counting walks
    // this chain, so there is no counter column to drift.
    let _ = conn.execute("ALTER TABLE agent_tasks ADD COLUMN retry_of TEXT", []);
    // Sprint 12 (Context Engine): file metadata columns for existing databases
    let _ = conn.execute("ALTER TABLE files ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN modified_at INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN language TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE files ADD COLUMN line_count INTEGER NOT NULL DEFAULT 0", []);

    Ok(conn)
}

/// Sprint 7 hardening: runs `f` as ONE SQLite transaction. Multi-statement
/// governance sequences (status + ledger + evidence + promotion) must be
/// atomic - a process kill mid-sequence must leave either all rows or none,
/// never e.g. a COMPLETED task with no evidence. Takes &Connection rather
/// than &mut because the app-wide Mutex already serializes every caller;
/// BEGIN IMMEDIATE fails loudly if a transaction were somehow already open.
pub fn in_transaction<T>(conn: &Connection, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| crate::core::errors::AppError::Provider(format!("failed to begin transaction: {e}")))?;
    match f(conn) {
        Ok(value) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| crate::core::errors::AppError::Provider(format!("failed to commit transaction: {e}")))?;
            Ok(value)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

pub fn with_conn<T>(db: &State<DbState>, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
    f(conn)
}

#[tauri::command]
pub fn index_workspace(
    state: State<crate::core::state::AppState>,
    db: State<DbState>,
) -> AppResult<indexer::IndexStats> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let stats = with_conn(&db, |conn| indexer::index_workspace(conn, &root))?;
    tracing::info!(
        target: "database",
        event = "workspace_indexed",
        files_indexed = stats.files_indexed,
        chunks_created = stats.chunks_created
    );
    Ok(stats)
}

#[tauri::command]
pub fn search_workspace(db: State<DbState>, query: String) -> AppResult<Vec<search::SearchResult>> {
    with_conn(&db, |conn| search::keyword_search(conn, &query, 20))
}

/// Cursor-style "find the file the user meant" without requiring an exact
/// path. Used by both chat context-building and agent task creation - see
/// resolver::resolve_file_reference for the ranking rules.
#[tauri::command]
pub fn resolve_file_reference(db: State<DbState>, query: String) -> AppResult<resolver::ResolutionResult> {
    with_conn(&db, |conn| resolver::resolve_file_reference(conn, &query))
}

/// Sprint 7 hardening tests: migration safety and transaction atomicity.
#[cfg(test)]
mod hardening_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_db_hardening_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Migration safety, both directions the spec asks for: (a) a fresh
    /// empty DB initializes cleanly (every temp_conn test already proves
    /// this, but assert it explicitly), and (b) reopening a PRE-POPULATED
    /// DB re-runs the full schema + every additive ALTER and must leave
    /// every existing row untouched - counted per table before and after,
    /// with the hash chain still verifying.
    #[test]
    fn migrations_preserve_existing_data_across_reopen() {
        let dir = temp_dir();
        let tables = [
            "requirements", "agent_tasks", "ledger_entries", "evidence",
            "promotion_requests", "task_dags", "worker_profiles",
        ];

        // (a) fresh DB: all Sprint 1-5 tables exist and are empty.
        let before_counts: Vec<i64> = {
            let conn = open_for_workspace(&dir).unwrap();
            for t in &tables {
                let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap();
                assert_eq!(n, 0, "fresh DB must start empty in {t}");
            }

            // Populate through the REAL APIs so rows look like production data.
            let req = crate::governance::requirements::create(
                &conn, "Migration fixture", "a requirement that must survive reopening",
                vec!["survives".to_string()], "test-user",
            ).unwrap();
            crate::agent::insert_task(&conn, "mig-task", "obj", crate::agent::task_type::EDIT_FILE, "f.rs",
                crate::agent::status::COMPLETED, "old", "new", "low", Some(&req.id), Some(&req.correlation_id)).unwrap();
            crate::governance::evidence::record(&conn, "mig-task", Some(&req.correlation_id),
                crate::governance::evidence::kind::VERIFICATION, "cargo check passed", true).unwrap();
            crate::governance::promotion::request_promotion(&conn, "mig-task", Some(&req.correlation_id)).unwrap();
            crate::intelligence::registry::upsert(&conn, &crate::intelligence::registry::WorkerProfile {
                id: "mig-worker".to_string(), name: "W".to_string(), capabilities: vec!["coding".to_string()],
                reliability_score: 1.0, tasks_completed: 0, tasks_failed: 0,
            }).unwrap();
            conn.execute("INSERT INTO task_dags (id, requirement_id, version, created_at, correlation_id) VALUES ('mig-dag', ?1, 1, 0, ?2)",
                rusqlite::params![req.id, req.correlation_id]).unwrap();

            let counts: Vec<i64> = tables.iter()
                .map(|t| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap())
                .collect();
            assert!(counts.iter().all(|&n| n > 0), "every table must have fixture data: {counts:?}");
            counts
            // conn dropped here - simulates app shutdown.
        };

        // (b) reopen: schema + all ALTER migrations run again against real data.
        let conn = open_for_workspace(&dir).unwrap();
        let after_counts: Vec<i64> = tables.iter()
            .map(|t| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap())
            .collect();
        assert_eq!(before_counts, after_counts, "no migration may drop or truncate anything");

        // Row content survived, not just row counts.
        let title: String = conn.query_row("SELECT title FROM requirements", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "Migration fixture");
        let chain = crate::governance::ledger::verify_chain(&conn).unwrap();
        assert!(chain.valid, "hash chain must survive a reopen: {:?}", chain.problem);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The atomicity primitive itself: an error inside in_transaction
    /// rolls back EVERYTHING written inside it.
    #[test]
    fn in_transaction_rolls_back_all_writes_on_error() {
        let dir = temp_dir();
        let conn = open_for_workspace(&dir).unwrap();

        let result: AppResult<()> = in_transaction(&conn, |conn| {
            conn.execute("INSERT INTO worker_profiles (id, name, capabilities) VALUES ('doomed', 'D', '[]')", []).unwrap();
            crate::governance::ledger::append(conn, crate::governance::ledger::LedgerEvent::TaskCreated, None, None, None, serde_json::json!({})).unwrap();
            Err(crate::core::errors::AppError::Provider("simulated mid-sequence crash".to_string()))
        });
        assert!(result.is_err());

        let workers: i64 = conn.query_row("SELECT COUNT(*) FROM worker_profiles", [], |r| r.get(0)).unwrap();
        let entries: i64 = conn.query_row("SELECT COUNT(*) FROM ledger_entries", [], |r| r.get(0)).unwrap();
        assert_eq!(workers, 0, "worker insert must have rolled back");
        assert_eq!(entries, 0, "ledger append must have rolled back with it");

        // And the connection is reusable afterwards (no dangling transaction).
        in_transaction(&conn, |conn| {
            conn.execute("INSERT INTO worker_profiles (id, name, capabilities) VALUES ('kept', 'K', '[]')", []).unwrap();
            Ok(())
        }).unwrap();
        let workers: i64 = conn.query_row("SELECT COUNT(*) FROM worker_profiles", [], |r| r.get(0)).unwrap();
        assert_eq!(workers, 1);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Audit remediation 3: foreign-key enforcement is now DECLARED, not
    /// inherited from the bundled SQLite's compile-time default. Pins the
    /// pragma and proves it bites, and that the Sprint 4 nullable
    /// evidence_id design still behaves identically under it.
    #[test]
    fn foreign_keys_are_explicitly_enforced() {
        let dir = temp_dir();
        let conn = open_for_workspace(&dir).unwrap();

        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 1, "foreign_keys pragma must be ON");

        // It bites: a promotion referencing nonexistent evidence is rejected.
        let violation = conn.execute(
            "INSERT INTO promotion_requests (id, evidence_id, task_id, status, requested_at) VALUES ('p1', 'no-such-evidence', 't1', 'blocked', 0)",
            [],
        );
        assert!(violation.is_err(), "dangling evidence_id must violate the FK");

        // Sprint 4's nullable design is unchanged: NULL evidence_id (the
        // audited no-evidence refusal) still inserts fine.
        let req = crate::governance::promotion::request_promotion(&conn, "task-without-evidence", None).unwrap();
        assert_eq!(req.status, crate::governance::promotion::status::BLOCKED);
        assert!(req.evidence_id.is_empty());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Journal guarantees are actually in effect: SQLite reports a real
    /// journal mode and synchronous!=OFF, so a mid-transaction kill is
    /// covered by SQLite's own atomic-commit machinery.
    #[test]
    fn sqlite_journal_guarantees_are_in_effect() {
        let dir = temp_dir();
        let conn = open_for_workspace(&dir).unwrap();
        let journal: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert!(!journal.eq_ignore_ascii_case("off"), "journaling must not be disabled, got {journal}");
        let sync: i64 = conn.query_row("PRAGMA synchronous", [], |r| r.get(0)).unwrap();
        assert!(sync >= 1, "synchronous must be NORMAL or FULL, got {sync}");
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
