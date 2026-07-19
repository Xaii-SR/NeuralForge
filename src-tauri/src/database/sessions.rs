//! Session persistence - v1.3.0 Phase 1 (database foundation only).
//!
//! Pure functions taking `&Connection` directly, no Tauri `State`/
//! `AppHandle` - matching this codebase's established "pure core, thin
//! Tauri wrapper" pattern (see `agent::` module's `agent_tasks` CRUD
//! functions, which this file's structure mirrors). Tauri command
//! wrappers/IPC registration are Phase 2, not this mission - nothing here
//! is `#[tauri::command]`-annotated or registered in `lib.rs`.
//!
//! Schema lives in `database::mod`'s existing `SCHEMA` constant
//! (`sessions`/`session_messages` tables), in the same per-workspace
//! `.neuralforge/index.db` every other table already uses - no new
//! database file, no new connection pattern.

use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace_path: String,
    pub title: String,
    pub provider: Option<String>,
    pub active_model: Option<String>,
    pub last_message_preview: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub status: String,
    pub timestamp: i64,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get("id")?,
        workspace_path: row.get("workspace_path")?,
        title: row.get("title")?,
        provider: row.get("provider")?,
        active_model: row.get("active_model")?,
        last_message_preview: row.get("last_message_preview")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<SessionMessage> {
    Ok(SessionMessage {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        role: row.get("role")?,
        content: row.get("content")?,
        status: row.get("status")?,
        timestamp: row.get("timestamp")?,
    })
}

/// Creates a new session row and returns it. `provider`/`model` are
/// optional (a session can exist before a provider/model is chosen).
pub fn create_session(
    conn: &Connection,
    workspace_path: &str,
    title: &str,
    provider: Option<&str>,
    model: Option<&str>,
) -> AppResult<Session> {
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        workspace_path: workspace_path.to_string(),
        title: title.to_string(),
        provider: provider.map(str::to_string),
        active_model: model.map(str::to_string),
        last_message_preview: None,
        created_at: now_secs(),
        updated_at: now_secs(),
    };

    conn.execute(
        "INSERT INTO sessions (id, workspace_path, title, provider, active_model, last_message_preview, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session.id,
            session.workspace_path,
            session.title,
            session.provider,
            session.active_model,
            session.last_message_preview,
            session.created_at,
            session.updated_at,
        ],
    )
    .map_err(|e| AppError::Provider(format!("failed to create session: {e}")))?;

    Ok(session)
}

/// Lists every session for `workspace_path`, most recently updated first.
/// A malformed/corrupted row (e.g. a column that can't deserialize into
/// `Session`'s types) is skipped and logged rather than failing the whole
/// query - one bad row must not hide every other real session from the
/// user.
pub fn list_sessions(conn: &Connection, workspace_path: &str) -> AppResult<Vec<Session>> {
    let mut stmt = conn
        .prepare("SELECT * FROM sessions WHERE workspace_path = ?1 ORDER BY updated_at DESC")
        .map_err(|e| AppError::Provider(format!("failed to query sessions: {e}")))?;

    let rows = stmt
        .query_map(params![workspace_path], row_to_session)
        .map_err(|e| AppError::Provider(format!("failed to query sessions: {e}")))?;

    let mut sessions = Vec::new();
    for row in rows {
        match row {
            Ok(session) => sessions.push(session),
            Err(e) => {
                tracing::warn!(target: "database", event = "malformed_session_row_skipped", error = %e);
            }
        }
    }
    Ok(sessions)
}

/// Appends a message to `session_id`. Does not update the parent
/// session's `updated_at`/`last_message_preview` - callers that want that
/// call `update_session_metadata` explicitly (kept separate so a caller
/// appending several messages in a batch doesn't pay redundant UPDATEs,
/// same reasoning as `agent::set_dag_membership` being its own call
/// rather than folded into task creation).
pub fn append_message(conn: &Connection, session_id: &str, role: &str, content: &str, status: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO session_messages (session_id, role, content, status, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_id, role, content, status, now_secs()],
    )
    .map_err(|e| AppError::Provider(format!("failed to append message: {e}")))?;
    Ok(())
}

/// Lists every message for `session_id`, oldest first (chronological
/// conversation order).
pub fn get_session_messages(conn: &Connection, session_id: &str) -> AppResult<Vec<SessionMessage>> {
    let mut stmt = conn
        .prepare("SELECT * FROM session_messages WHERE session_id = ?1 ORDER BY timestamp ASC, id ASC")
        .map_err(|e| AppError::Provider(format!("failed to query session messages: {e}")))?;
    let rows = stmt
        .query_map(params![session_id], row_to_message)
        .map_err(|e| AppError::Provider(format!("failed to query session messages: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read session message row: {e}")))
}

/// Updates a session's `title`/`last_message_preview` and bumps
/// `updated_at` - the "this session was just active" signal `list_sessions`
/// sorts on.
pub fn update_session_metadata(conn: &Connection, session_id: &str, title: &str, last_message_preview: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE sessions SET title = ?1, last_message_preview = ?2, updated_at = ?3 WHERE id = ?4",
        params![title, last_message_preview, now_secs(), session_id],
    )
    .map_err(|e| AppError::Provider(format!("failed to update session metadata: {e}")))?;
    Ok(())
}

/// Deletes `session_id` and every one of its messages. Relies on the
/// schema's `ON DELETE CASCADE` (see `database::mod`'s `SCHEMA` constant)
/// for the message cleanup - not a manual second DELETE - so this stays
/// correct even if a future caller deletes a session through raw SQL
/// elsewhere. `PRAGMA foreign_keys = ON` (set once in `open_for_workspace`)
/// is what makes the cascade actually fire; without it SQLite would
/// silently ignore `ON DELETE CASCADE` and orphan the messages instead.
pub fn delete_session(conn: &Connection, session_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
        .map_err(|e| AppError::Provider(format!("failed to delete session: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> Connection {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_sessions_test_{nanos}"));
        crate::database::open_for_workspace(&dir).unwrap()
    }

    #[test]
    fn create_session_persists_and_returns_a_real_row() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "New chat", Some("ollama"), Some("qwen2.5-coder:7b")).unwrap();

        assert_eq!(session.workspace_path, "/workspace/a");
        assert_eq!(session.title, "New chat");
        assert_eq!(session.provider.as_deref(), Some("ollama"));
        assert_eq!(session.active_model.as_deref(), Some("qwen2.5-coder:7b"));
        assert!(session.last_message_preview.is_none());
        assert!(!session.id.is_empty());
    }

    #[test]
    fn create_session_allows_no_provider_or_model() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "New chat", None, None).unwrap();
        assert!(session.provider.is_none());
        assert!(session.active_model.is_none());
    }

    #[test]
    fn list_sessions_scopes_to_workspace_and_orders_by_most_recently_updated() {
        let conn = temp_conn();
        let a = create_session(&conn, "/workspace/a", "Session A", None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = create_session(&conn, "/workspace/a", "Session B", None, None).unwrap();
        create_session(&conn, "/workspace/other", "Different workspace", None, None).unwrap();

        // Touch A so it becomes the most recently updated.
        update_session_metadata(&conn, &a.id, "Session A", "hello").unwrap();

        let sessions = list_sessions(&conn, "/workspace/a").unwrap();
        assert_eq!(sessions.len(), 2, "must only return sessions for the requested workspace");
        assert_eq!(sessions[0].id, a.id, "most recently updated session must come first");
        assert_eq!(sessions[1].id, b.id);
    }

    #[test]
    fn list_sessions_skips_a_malformed_row_instead_of_failing_the_whole_query() {
        let conn = temp_conn();
        let good = create_session(&conn, "/workspace/a", "Good session", None, None).unwrap();

        // SQLite is weakly typed (no STRICT tables here) - it happily
        // stores a TEXT value in `created_at` even though the column is
        // conceptually INTEGER. rusqlite's typed `row.get::<_, i64>` then
        // fails to convert it, so row_to_session errors for exactly this
        // row - a real "malformed at the Rust level" case that isn't
        // rejected at INSERT time, simulating hand-edited/corrupted data
        // or a future schema mismatch.
        conn.execute(
            "INSERT INTO sessions (id, workspace_path, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["malformed-session", "/workspace/a", "Malformed", "not-a-number", 1i64],
        )
        .unwrap();

        let sessions = list_sessions(&conn, "/workspace/a").unwrap();
        assert_eq!(sessions.len(), 1, "the malformed row must be skipped, not crash the whole list");
        assert_eq!(sessions[0].id, good.id, "the real, well-formed session must still be returned");
    }

    #[test]
    fn append_message_and_get_session_messages_round_trip_in_order() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "Chat", None, None).unwrap();

        append_message(&conn, &session.id, "user", "hello", "completed").unwrap();
        append_message(&conn, &session.id, "assistant", "hi there", "completed").unwrap();

        let messages = get_session_messages(&conn, &session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "hi there");
        assert_eq!(messages[1].status, "completed");
    }

    #[test]
    fn get_session_messages_is_empty_for_a_session_with_no_messages() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "Chat", None, None).unwrap();
        assert!(get_session_messages(&conn, &session.id).unwrap().is_empty());
    }

    #[test]
    fn update_session_metadata_changes_title_preview_and_bumps_updated_at() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "Untitled", None, None).unwrap();
        let original_updated_at = session.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(1100));
        update_session_metadata(&conn, &session.id, "Fix auth bug", "Let's fix the login flow").unwrap();

        let sessions = list_sessions(&conn, "/workspace/a").unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "Fix auth bug");
        assert_eq!(sessions[0].last_message_preview.as_deref(), Some("Let's fix the login flow"));
        assert!(sessions[0].updated_at > original_updated_at, "updated_at must advance on metadata update");
    }

    #[test]
    fn delete_session_removes_the_session_and_all_its_messages_no_orphans() {
        let conn = temp_conn();
        let session = create_session(&conn, "/workspace/a", "Chat", None, None).unwrap();
        append_message(&conn, &session.id, "user", "hello", "completed").unwrap();
        append_message(&conn, &session.id, "assistant", "hi", "completed").unwrap();

        delete_session(&conn, &session.id).unwrap();

        assert!(list_sessions(&conn, "/workspace/a").unwrap().is_empty(), "session itself must be gone");

        // Verify no orphaned message rows survive the cascade - query the
        // table directly rather than through get_session_messages, so this
        // test still catches an orphan even if get_session_messages itself
        // had a bug that hid them.
        let orphan_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_messages WHERE session_id = ?1", params![session.id], |r| r.get(0))
            .unwrap();
        assert_eq!(orphan_count, 0, "deleting a session must not leave orphaned session_messages rows");
    }

    #[test]
    fn delete_session_on_an_unknown_id_is_not_an_error() {
        let conn = temp_conn();
        // A DELETE affecting zero rows is not a SQL error - `delete_session`
        // doesn't check row count, so this stays a simple idempotent
        // operation: "make sure it's gone" succeeds whether or not it
        // existed to begin with.
        assert!(delete_session(&conn, "does-not-exist").is_ok());
    }
}
