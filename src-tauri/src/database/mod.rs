pub mod indexer;
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
    indexed_at INTEGER NOT NULL
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
    file_path TEXT NOT NULL,
    status TEXT NOT NULL,
    original_content TEXT,
    proposed_content TEXT,
    risk_summary TEXT,
    verification TEXT,
    rollback TEXT,
    error TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

pub fn open_for_workspace(workspace_root: &Path) -> AppResult<Connection> {
    let db_dir = workspace_root.join(".neuralforge");
    std::fs::create_dir_all(&db_dir)?;
    let db_path = db_dir.join("index.db");

    let conn = Connection::open(&db_path)
        .map_err(|e| AppError::Provider(format!("failed to open index.db: {e}")))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| AppError::Provider(format!("failed to init schema: {e}")))?;

    Ok(conn)
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
