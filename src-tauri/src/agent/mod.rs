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
    pub task_type: String,
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

pub mod task_type {
    pub const EDIT_FILE: &str = "edit_file";
    pub const RUN_CODE: &str = "run_code";
}

pub const CODER_AGENT: &str = "coder";

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

pub fn insert_task(
    conn: &Connection,
    id: &str,
    objective: &str,
    task_type: &str,
    file_path: &str,
    status: &str,
    original_content: &str,
    proposed_content: &str,
    risk_summary: &str,
) -> AppResult<()> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO agent_tasks (id, objective, agent, task_type, file_path, status, original_content, proposed_content, risk_summary, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![id, objective, CODER_AGENT, task_type, file_path, status, original_content, proposed_content, risk_summary, now],
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
    let task: String = row.get("task_type")?;
    let files = if task == task_type::RUN_CODE { vec![] } else { vec![file_path] };
    Ok(AgentTask {
        id: row.get("id")?,
        objective: row.get("objective")?,
        agent: row.get("agent")?,
        task_type: task,
        files,
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
        insert_task(conn, &id, &objective, task_type::EDIT_FILE, &file_path, status::PLANNING, &original_content, "", "")?;
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
                task_type: task_type::EDIT_FILE.to_string(),
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

/// Same Simulation Mode / human-approval flow as create_and_plan_task, but
/// for "write and run a script" objectives instead of "edit this file".
/// Deliberately does NOT require a workspace to be open - code execution
/// happens in the extension's own isolated directory, not the workspace.
#[tauri::command]
pub async fn create_and_plan_code_task(db: tauri::State<'_, crate::database::DbState>, objective: String) -> AppResult<AgentTask> {
    let id = uuid::Uuid::new_v4().to_string();

    {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        insert_task(conn, &id, &objective, task_type::RUN_CODE, "", status::PLANNING, "", "", "")?;
    }

    let plan_result = planner::plan_code(&objective).await;

    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    match plan_result {
        Ok((code, risk_summary)) => {
            conn.execute(
                "UPDATE agent_tasks SET status = ?1, proposed_content = ?2, risk_summary = ?3, updated_at = ?4 WHERE id = ?5",
                rusqlite::params![status::AWAITING_APPROVAL, code, risk_summary, now_secs(), id],
            )
            .map_err(|e| AppError::Provider(format!("failed to save plan: {e}")))?;

            tracing::info!(target: "agent", event = "code_task_planned", task_id = %id, risk = %risk_summary);

            Ok(AgentTask {
                id,
                objective,
                agent: CODER_AGENT.to_string(),
                task_type: task_type::RUN_CODE.to_string(),
                files: vec![],
                status: status::AWAITING_APPROVAL.to_string(),
                verification: None,
                rollback: None,
                proposed_content: Some(code),
                risk_summary: Some(risk_summary),
                error: None,
            })
        }
        Err(e) => {
            update_status(conn, &id, status::FAILED, None, Some(&e.to_string()))?;
            tracing::warn!(target: "agent", event = "code_task_planning_failed", task_id = %id, error = %e);
            Err(e)
        }
    }
}

fn format_extension_output(output: &serde_json::Value) -> String {
    match output {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn approve_task(
    state: tauri::State<'_, crate::core::state::AppState>,
    db: tauri::State<'_, crate::database::DbState>,
    task_id: String,
) -> AppResult<AgentTask> {
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

    // (final_status, verification text, error, rollback note, workspace root if this
    // was a file edit - only file edits get a memory record, since run_code tasks
    // never touch the workspace).
    let (final_status, verification, error, rollback_note, memory_root): (
        &str,
        String,
        Option<String>,
        Option<String>,
        Option<std::path::PathBuf>,
    ) = if task.task_type == task_type::RUN_CODE {
        match executor::run_code_via_extension(&proposed_content).await {
            Ok(result) if result.success => (status::COMPLETED, format_extension_output(&result.output), None, None, None),
            Ok(result) => (status::FAILED, format_extension_output(&result.output), result.error, None, None),
            Err(e) => (status::FAILED, String::new(), Some(e.to_string()), None, None),
        }
    } else {
        let root = state
            .workspace_root
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        let file_path = task.files.first().cloned().unwrap_or_default();
        let result = executor::apply_and_verify(&root, &file_path, &original_content, &proposed_content).await?;
        let final_status = if result.rolled_back { status::ROLLED_BACK } else { status::COMPLETED };
        let error = if result.rolled_back { Some(result.verification.clone()) } else { None };
        let rollback_note = if result.rolled_back { Some("original content restored after failed verification".to_string()) } else { None };
        (final_status, result.verification, error, rollback_note, Some(root))
    };

    {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        update_status(conn, &task_id, final_status, Some(&verification), error.as_deref())?;
        if let Some(note) = &rollback_note {
            set_rollback(conn, &task_id, note)?;
        }
    }

    if let Some(root) = &memory_root {
        let file_path = task.files.first().cloned().unwrap_or_default();
        memory::record_task_outcome(root, &task_id, &task.objective, &file_path, final_status, &verification).ok();
    }

    tracing::info!(
        target: "agent",
        event = "task_finished",
        task_id = %task_id,
        task_type = %task.task_type,
        status = final_status,
        verification = %verification
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

        insert_task(&conn, "task-1", "add a comment", task_type::EDIT_FILE, "main.rs", status::AWAITING_APPROVAL, "old", "new", "1 line changed").unwrap();

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

    #[test]
    fn run_code_task_reports_no_files_since_it_never_touches_the_workspace() {
        let (dir, conn) = temp_conn();

        insert_task(&conn, "code-task-1", "print 42", task_type::RUN_CODE, "", status::AWAITING_APPROVAL, "", "print(42)", "low risk").unwrap();

        let task = get_task(&conn, "code-task-1").unwrap();
        assert_eq!(task.task_type, task_type::RUN_CODE);
        assert!(task.files.is_empty(), "run_code tasks should not report a workspace file");
        assert_eq!(task.proposed_content.as_deref(), Some("print(42)"));

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end gate test for "Run Python code via agent": drives the same
    /// planning -> awaiting_approval -> applying -> completed state machine
    /// approve_task uses, but calls the underlying DB/executor functions
    /// directly (a live tauri::State can't be constructed outside a running
    /// app - see MockRuntime notes elsewhere in this codebase). Points
    /// HOME/USERPROFILE at a scratch dir so it runs against a throwaway
    /// python-repl install, not the developer's real extensions folder, and
    /// genuinely spawns python.exe rather than mocking the extension layer.
    #[tokio::test]
    async fn gate_test_run_python_code_via_agent_task_lifecycle() {
        if std::process::Command::new("python").arg("--version").output().is_err() {
            eprintln!("skipping: python not on PATH");
            return;
        }

        let (dir, conn) = temp_conn();

        let mut home = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        home.push(format!("neuralforge_agent_gate_test_home_{nanos}"));
        std::fs::create_dir_all(&home).unwrap();

        #[cfg(windows)]
        let var = "USERPROFILE";
        #[cfg(not(windows))]
        let var = "HOME";
        let previous = std::env::var(var).ok();
        unsafe { std::env::set_var(var, &home) };

        // "Load PythonREPL extension": ensure_and_scan bundles + scans it,
        // proving it is genuinely discoverable through the loader before any
        // task references it.
        let ext_dir = crate::extensions::loader::extensions_dir().unwrap();
        crate::extensions::loader::ensure_bundled_extensions(&ext_dir).unwrap();
        let installed = crate::extensions::loader::scan(&ext_dir).unwrap();
        assert!(installed.iter().any(|e| e.manifest.name == "python-repl"), "python-repl should be discoverable after bundling");

        // Planning step (plan_code itself needs a live Ollama call, so this
        // test supplies a fixed objective/code pair the same way plan_code
        // would return one, and exercises everything downstream for real).
        let id = "code-gate-task";
        let objective = "print the sum of 40 and 2";
        let code = "print(40 + 2)";
        insert_task(&conn, id, objective, task_type::RUN_CODE, "", status::PLANNING, "", "", "").unwrap();
        conn.execute(
            "UPDATE agent_tasks SET status = ?1, proposed_content = ?2, risk_summary = ?3 WHERE id = ?4",
            rusqlite::params![status::AWAITING_APPROVAL, code, "low risk: 1 line", id],
        )
        .unwrap();

        let task = get_task(&conn, id).unwrap();
        assert_eq!(task.status, status::AWAITING_APPROVAL);

        // Approval step: run the exact same extension invocation approve_task uses.
        update_status(&conn, id, status::APPLYING, None, None).unwrap();
        let result = executor::run_code_via_extension(code).await.unwrap();
        let final_status = if result.success { status::COMPLETED } else { status::FAILED };
        update_status(&conn, id, final_status, Some(&format_extension_output(&result.output)), result.error.as_deref()).unwrap();

        let finished = get_task(&conn, id).unwrap();

        unsafe {
            if let Some(prev) = previous {
                std::env::set_var(var, prev);
            } else {
                std::env::remove_var(var);
            }
        }
        std::fs::remove_dir_all(&home).ok();
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();

        // "Verify output": the real python.exe subprocess actually computed 42.
        assert_eq!(finished.status, status::COMPLETED);
        assert_eq!(finished.verification.as_deref(), Some("42"));
    }
}
