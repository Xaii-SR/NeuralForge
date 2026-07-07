pub mod executor;
pub mod memory;
pub mod planner;

use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Matches the blueprint's task JSON protocol exactly:
/// {"id","objective","agent","files","status","verification","rollback"}.
/// "files" is a single-element list for now (Phase 5 foundation scope is
/// one file per task); the schema/struct shape already supports a real list
/// so multi-file tasks are an additive change later, not a rework.
#[derive(Serialize, Deserialize, Clone)]
pub struct AgentTask {
    pub id: String,
    pub objective: String,
    pub agent: String,
    pub files: Vec<String>,
    pub status: String,
    pub verification: Option<String>,
    pub rollback: Option<String>,
    pub proposed_content: Option<String>,
    pub risk_summary: Option<String>,
    pub error: Option<String>,
}

pub mod status {
    pub const PLANNING: &str = "planning";
    pub const AWAITING_APPROVAL: &str = "awaiting_approval";
    pub const APPLYING: &str = "applying";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
    pub const ROLLED_BACK: &str = "rolled_back";
    pub const REJECTED: &str = "rejected";
}

pub const CODER_AGENT: &str = "coder";

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn insert_task(
    conn: &Connection,
    id: &str,
    objective: &str,
    file_path: &str,
    status: &str,
    original_content: &str,
    proposed_content: &str,
    risk_summary: &str,
) -> AppResult<()> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO agent_tasks (id, objective, agent, file_path, status, original_content, proposed_content, risk_summary, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![id, objective, CODER_AGENT, file_path, status, original_content, proposed_content, risk_summary, now],
    )
    .map_err(|e| AppError::Provider(format!("failed to create task: {e}")))?;
    Ok(())
}

pub fn update_status(conn: &Connection, id: &str, status: &str, verification: Option<&str>, error: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_tasks SET status = ?1, verification = ?2, error = ?3, updated_at = ?4 WHERE id = ?5",
        params![status, verification, error, now_secs(), id],
    )
    .map_err(|e| AppError::Provider(format!("failed to update task: {e}")))?;
    Ok(())
}

pub fn set_rollback(conn: &Connection, id: &str, rollback_note: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_tasks SET rollback = ?1, updated_at = ?2 WHERE id = ?3",
        params![rollback_note, now_secs(), id],
    )
    .map_err(|e| AppError::Provider(format!("failed to record rollback: {e}")))?;
    Ok(())
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<AgentTask> {
    let file_path: String = row.get("file_path")?;
    Ok(AgentTask {
        id: row.get("id")?,
        objective: row.get("objective")?,
        agent: row.get("agent")?,
        files: vec![file_path],
        status: row.get("status")?,
        verification: row.get("verification")?,
        rollback: row.get("rollback")?,
        proposed_content: row.get("proposed_content")?,
        risk_summary: row.get("risk_summary")?,
        error: row.get("error")?,
    })
}

pub fn get_task(conn: &Connection, id: &str) -> AppResult<AgentTask> {
    conn.query_row("SELECT * FROM agent_tasks WHERE id = ?1", params![id], row_to_task)
        .map_err(|_| AppError::NotFound(id.to_string()))
}

pub fn get_task_content(conn: &Connection, id: &str) -> AppResult<(String, String)> {
    conn.query_row(
        "SELECT original_content, proposed_content FROM agent_tasks WHERE id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|_| AppError::NotFound(id.to_string()))
}

pub fn list_tasks(conn: &Connection) -> AppResult<Vec<AgentTask>> {
    let mut stmt = conn
        .prepare("SELECT * FROM agent_tasks ORDER BY created_at DESC")
        .map_err(|e| AppError::Provider(format!("failed to query tasks: {e}")))?;
    let rows = stmt
        .query_map([], row_to_task)
        .map_err(|e| AppError::Provider(format!("failed to query tasks: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read task row: {e}")))
}

#[tauri::command]
pub async fn create_and_plan_task(
    state: tauri::State<'_, crate::core::state::AppState>,
    db: tauri::State<'_, crate::database::DbState>,
    objective: String,
    file_path: String,
) -> AppResult<AgentTask> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let target = root.join(&file_path);
    let canonical_root = std::fs::canonicalize(&root)?;
    let canonical_target = std::fs::canonicalize(&target).map_err(|_| AppError::NotFound(file_path.clone()))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(format!("{file_path} is outside the workspace")));
    }

    let original_content = std::fs::read_to_string(&target)?;
    let id = uuid::Uuid::new_v4().to_string();

    // Insert the row in PLANNING state before the (potentially slow) LLM
    // call, so the task is genuinely queryable/visible while in flight -
    // not just after it succeeds. A crash or failure mid-plan still leaves
    // a real, honest record instead of the task silently never existing.
    {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        insert_task(conn, &id, &objective, &file_path, status::PLANNING, &original_content, "", "")?;
    }

    let plan_result = planner::plan_change(&objective, &file_path, &original_content).await;

    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    match plan_result {
        Ok((proposed_content, risk_summary)) => {
            conn.execute(
                "UPDATE agent_tasks SET status = ?1, proposed_content = ?2, risk_summary = ?3, updated_at = ?4 WHERE id = ?5",
                rusqlite::params![status::AWAITING_APPROVAL, proposed_content, risk_summary, now_secs(), id],
            )
            .map_err(|e| AppError::Provider(format!("failed to save plan: {e}")))?;

            tracing::info!(target: "agent", event = "task_planned", task_id = %id, agent = CODER_AGENT, file = %file_path, risk = %risk_summary);

            Ok(AgentTask {
                id,
                objective,
                agent: CODER_AGENT.to_string(),
                files: vec![file_path],
                status: status::AWAITING_APPROVAL.to_string(),
                verification: None,
                rollback: None,
                proposed_content: Some(proposed_content),
                risk_summary: Some(risk_summary),
                error: None,
            })
        }
        Err(e) => {
            update_status(conn, &id, status::FAILED, None, Some(&e.to_string()))?;
            tracing::warn!(target: "agent", event = "task_planning_failed", task_id = %id, error = %e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn approve_task(
    state: tauri::State<'_, crate::core::state::AppState>,
    db: tauri::State<'_, crate::database::DbState>,
    task_id: String,
) -> AppResult<AgentTask> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let (task, original_content, proposed_content) = {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        let task = get_task(conn, &task_id)?;
        let (original_content, proposed_content) = get_task_content(conn, &task_id)?;
        update_status(conn, &task_id, status::APPLYING, None, None)?;
        (task, original_content, proposed_content)
    };

    let file_path = task.files.first().cloned().unwrap_or_default();
    let result = executor::apply_and_verify(&root, &file_path, &original_content, &proposed_content).await?;

    let final_status = if result.rolled_back { status::ROLLED_BACK } else { status::COMPLETED };
    let error = if result.rolled_back { Some(result.verification.as_str()) } else { None };

    {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        update_status(conn, &task_id, final_status, Some(&result.verification), error)?;
        if result.rolled_back {
            set_rollback(conn, &task_id, "original content restored after failed verification")?;
        }
    }

    memory::record_task_outcome(&root, &task_id, &task.objective, &file_path, final_status, &result.verification).ok();

    tracing::info!(
        target: "agent",
        event = "task_finished",
        task_id = %task_id,
        status = final_status,
        rolled_back = result.rolled_back,
        verification = %result.verification
    );

    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().unwrap();
    get_task(conn, &task_id)
}

#[tauri::command]
pub fn reject_task(db: tauri::State<crate::database::DbState>, task_id: String) -> AppResult<()> {
    crate::database::with_conn(&db, |conn| update_status(conn, &task_id, status::REJECTED, None, None))?;
    tracing::info!(target: "agent", event = "task_rejected", task_id = %task_id);
    Ok(())
}

#[tauri::command]
pub fn list_agent_tasks(db: tauri::State<crate::database::DbState>) -> AppResult<Vec<AgentTask>> {
    crate::database::with_conn(&db, list_tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_agent_db_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    #[test]
    fn insert_get_update_roundtrip() {
        let (dir, conn) = temp_conn();

        insert_task(&conn, "task-1", "add a comment", "main.rs", status::AWAITING_APPROVAL, "old", "new", "1 line changed").unwrap();

        let task = get_task(&conn, "task-1").unwrap();
        assert_eq!(task.objective, "add a comment");
        assert_eq!(task.status, status::AWAITING_APPROVAL);
        assert_eq!(task.files, vec!["main.rs".to_string()]);
        assert_eq!(task.proposed_content.as_deref(), Some("new"));

        update_status(&conn, "task-1", status::COMPLETED, Some("cargo check passed"), None).unwrap();
        let updated = get_task(&conn, "task-1").unwrap();
        assert_eq!(updated.status, status::COMPLETED);
        assert_eq!(updated.verification.as_deref(), Some("cargo check passed"));

        set_rollback(&conn, "task-1", "restored original content").unwrap();
        let with_rollback = get_task(&conn, "task-1").unwrap();
        assert_eq!(with_rollback.rollback.as_deref(), Some("restored original content"));

        let all = list_tasks(&conn).unwrap();
        assert_eq!(all.len(), 1);

        assert!(get_task(&conn, "nonexistent").is_err());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
