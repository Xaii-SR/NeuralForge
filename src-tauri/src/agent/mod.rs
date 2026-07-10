pub mod executor;
pub mod memory;
pub mod planner;

use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::governance::ledger::{self, LedgerEvent};

/// Matches the blueprint's task JSON protocol exactly:
/// {"id","objective","agent","files","status","verification","rollback"}.
/// "files" is a single-element list for now (Phase 5 foundation scope is
/// one file per task); the schema/struct shape already supports a real list
/// so multi-file tasks are an additive change later, not a rework.
#[derive(Serialize, Deserialize, Type, Clone)]
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
    /// Sprint 1: the requirement that gated this task. None for run_code
    /// tasks (still ungated this sprint) and pre-Sprint-1 rows.
    pub requirement_id: Option<String>,
    /// Copied from the gating requirement so the whole chain (requirement
    /// -> task -> future evidence/ledger records) shares one queryable ID.
    pub correlation_id: Option<String>,
    /// Sprint 3: DAG membership. None for single-task (Sprint 1/2) flow -
    /// that flow is unchanged and never sets these.
    pub dag_id: Option<String>,
    /// Task IDs that must be COMPLETED before this task may run.
    pub depends_on: Vec<String>,
    /// Sprint 8: the task this row is a retry of. None for first attempts.
    pub retry_of: Option<String>,
}

pub mod status {
    pub const PLANNING: &str = "planning";
    pub const AWAITING_APPROVAL: &str = "awaiting_approval";
    pub const APPLYING: &str = "applying";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
    pub const ROLLED_BACK: &str = "rolled_back";
    pub const REJECTED: &str = "rejected";
    /// Sprint 3: a dependency failed/rolled back, so this DAG node can
    /// never legally run. Terminal, like FAILED, but distinguishes "this
    /// task was never attempted" from "this task was attempted and failed".
    pub const BLOCKED: &str = "blocked";
}

pub mod task_type {
    pub const EDIT_FILE: &str = "edit_file";
    pub const RUN_CODE: &str = "run_code";
}

pub const CODER_AGENT: &str = "coder";

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

#[allow(clippy::too_many_arguments)]
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
    requirement_id: Option<&str>,
    correlation_id: Option<&str>,
) -> AppResult<()> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO agent_tasks (id, objective, agent, task_type, file_path, status, original_content, proposed_content, risk_summary, requirement_id, correlation_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![id, objective, CODER_AGENT, task_type, file_path, status, original_content, proposed_content, risk_summary, requirement_id, correlation_id, now],
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

/// Sprint 3: stamps DAG membership onto an existing task row. Separate
/// from insert_task so the Sprint 1/2 single-task insert path is
/// untouched - single tasks simply never get stamped.
pub fn set_dag_membership(conn: &Connection, task_id: &str, dag_id: &str, depends_on: &[String]) -> AppResult<()> {
    let deps_json = serde_json::to_string(depends_on).map_err(|e| AppError::Provider(format!("failed to encode depends_on: {e}")))?;
    conn.execute(
        "UPDATE agent_tasks SET dag_id = ?1, depends_on = ?2, updated_at = ?3 WHERE id = ?4",
        params![dag_id, deps_json, now_secs(), task_id],
    )
    .map_err(|e| AppError::Provider(format!("failed to set DAG membership: {e}")))?;
    Ok(())
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<AgentTask> {
    let file_path: String = row.get("file_path")?;
    let task: String = row.get("task_type")?;
    let files = if task == task_type::RUN_CODE { vec![] } else { vec![file_path] };
    let depends_on_json: Option<String> = row.get("depends_on")?;
    let depends_on = depends_on_json
        .as_deref()
        .map(|j| serde_json::from_str(j).unwrap_or_default())
        .unwrap_or_default();
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
        requirement_id: row.get("requirement_id")?,
        correlation_id: row.get("correlation_id")?,
        dag_id: row.get("dag_id")?,
        depends_on,
        retry_of: row.get("retry_of")?,
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

/// Flattens a validated requirement into the objective string the
/// (unchanged) planner consumes: intent plus an explicit acceptance-
/// criteria checklist, so the model plans against the contract instead
/// of a raw prompt.
pub fn objective_from_requirement(req: &crate::governance::requirements::RequirementContract) -> String {
    let criteria = req
        .acceptance_criteria
        .iter()
        .map(|c| format!("- {c}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n\nThe change is only acceptable if all of these criteria hold:\n{criteria}", req.intent)
}

/// Sprint 1 (Requirement Intelligence): edit_file tasks no longer accept a
/// raw prompt - they take the ID of a validated, ACTIVE requirement, and
/// the planner refuses to run without one. run_code tasks
/// (create_and_plan_code_task below) are deliberately still ungated this
/// sprint - flagged as a Sprint 2 follow-up, not silently bundled here.
#[tauri::command]
pub async fn create_and_plan_task(
    state: tauri::State<'_, crate::core::state::AppState>,
    db: tauri::State<'_, crate::database::DbState>,
    requirement_id: String,
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

    // The requirement gate runs before the task row exists and long before
    // any LLM call: a missing or retired requirement is a hard refusal
    // that leaves no trace in agent_tasks, because no work was authorized.
    // Insert the row in PLANNING state before the (potentially slow) LLM
    // call, so the task is genuinely queryable/visible while in flight -
    // a crash or failure mid-plan still leaves a real, honest record.
    let (objective, correlation_id) = {
        let guard = db.conn.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
        let requirement = crate::governance::requirements::get_active(conn, &requirement_id)?;
        let objective = objective_from_requirement(&requirement);
        insert_task(
            conn,
            &id,
            &objective,
            task_type::EDIT_FILE,
            &file_path,
            status::PLANNING,
            &original_content,
            "",
            "",
            Some(&requirement_id),
            Some(&requirement.correlation_id),
        )?;
        (objective, requirement.correlation_id)
    };

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

            tracing::info!(target: "agent", event = "task_planned", task_id = %id, requirement_id = %requirement_id, correlation_id = %correlation_id, agent = CODER_AGENT, file = %file_path, risk = %risk_summary);

            // Record ledger events for task lifecycle
            if let Some(conn) = guard.as_ref() {
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskCreated,
                    Some(correlation_id.as_str()),
                    Some(requirement_id.as_str()),
                    Some(&id),
                    serde_json::json!({
                        "objective": objective,
                        "file_path": file_path,
                        "risk_summary": risk_summary
                    }),
                );
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskPlanned,
                    Some(correlation_id.as_str()),
                    Some(requirement_id.as_str()),
                    Some(&id),
                    serde_json::json!({
                        "status": status::AWAITING_APPROVAL,
                        "planned_content_available": true
                    }),
                );
            }

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
                requirement_id: Some(requirement_id),
                correlation_id: Some(correlation_id),
                dag_id: None,
                depends_on: vec![],
                retry_of: None,
            })
        }
        Err(e) => {
            update_status(conn, &id, status::FAILED, None, Some(&e.to_string()))?;
            
            // Record ledger event for task plan failure
            let _ = ledger::append(
                conn,
                LedgerEvent::TaskPlanFailed,
                if correlation_id.is_empty() { None } else { Some(correlation_id.as_str()) },
                if requirement_id.is_empty() { None } else { Some(requirement_id.as_str()) },
                Some(&id),
                serde_json::json!({
                    "error": e.to_string()
                }),
            );
            
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
        insert_task(conn, &id, &objective, task_type::RUN_CODE, "", status::PLANNING, "", "", "", None, None)?;
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

            // Record ledger events for code task lifecycle
            if let Some(conn) = guard.as_ref() {
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskCreated,
                    None,
                    None,
                    Some(&id),
                    serde_json::json!({
                        "objective": objective,
                        "task_type": task_type::RUN_CODE
                    }),
                );
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskPlanned,
                    None,
                    None,
                    Some(&id),
                    serde_json::json!({
                        "status": status::AWAITING_APPROVAL,
                        "planned_content_available": true
                    }),
                );
            }

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
                requirement_id: None,
                correlation_id: None,
                dag_id: None,
                depends_on: vec![],
                retry_of: None,
            })
        }
        Err(e) => {
            update_status(conn, &id, status::FAILED, None, Some(&e.to_string()))?;
            
            // Record ledger event for code task plan failure
            if let Some(conn_ref) = guard.as_ref() {
                let _ = ledger::append(
                    conn_ref,
                    LedgerEvent::TaskPlanFailed,
                    None,
                    None,
                    Some(&id),
                    serde_json::json!({
                        "error": e.to_string()
                    }),
                );
            }
            
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
        
        // Record ledger event for task approval
        let _ = ledger::append(
            conn,
            LedgerEvent::TaskApproved,
            task.correlation_id.as_ref().map(|s| s.as_str()),
            task.requirement_id.as_ref().map(|s| s.as_str()),
            Some(&task_id),
            serde_json::json!({
                "task_type": task.task_type,
                "file_path": task.files.first().cloned().unwrap_or_default()
            }),
        );
        
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
        record_task_outcome_atomic(conn, &task, &task_id, final_status, &verification, error.as_deref(), rollback_note.as_deref())?;
    }

    // The agent_history.md append is a plain file write - deliberately
    // outside (after) the DB transaction, per the audit remediation.
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
    read_task_after_finish(guard.as_ref(), &task_id)
}

/// Audit remediation (post-Sprint-7): the final task read-back after
/// execution. Previously `guard.as_ref().unwrap()` - a panic if the
/// workspace was closed while the executor's long await was in flight.
/// Now a clean error, matching every other connection access in this file.
pub(crate) fn read_task_after_finish(conn: Option<&Connection>, task_id: &str) -> AppResult<AgentTask> {
    let conn = conn.ok_or_else(|| AppError::InvalidPath("workspace was closed while the task was finishing".to_string()))?;
    get_task(conn, task_id)
}

/// Audit remediation (post-Sprint-7): the ENTIRE task outcome - status,
/// rollback note, evidence (execution output for run_code; verification +
/// optional rollback evidence for file edits), promotion verdict, ledger
/// completion event, and DAG dependent-blocking - commits as ONE
/// transaction. Previously the file-edit evidence + promotion lived in a
/// second transaction; a kill between the two could leave a COMPLETED task
/// with no evidence and no promotion row. What gets written is unchanged -
/// only when it commits.
pub(crate) fn record_task_outcome_atomic(
    conn: &Connection,
    task: &AgentTask,
    task_id: &str,
    final_status: &str,
    verification: &str,
    error: Option<&str>,
    rollback_note: Option<&str>,
) -> AppResult<()> {
    use crate::governance::evidence::{self, kind};
    crate::database::in_transaction(conn, |conn| {
        update_status(conn, task_id, final_status, Some(verification), error)?;
        if let Some(note) = rollback_note {
            set_rollback(conn, task_id, note)?;
        }

        if task.task_type == task_type::RUN_CODE {
            // Execution-output evidence for code runs.
            evidence::record(
                conn,
                task_id,
                task.correlation_id.as_ref().map(|s| s.as_str()),
                kind::EXECUTION_OUTPUT,
                verification,
                error.is_none(), // success = true if no error
            )?;
        } else {
            // Verification evidence (plus rollback evidence when the
            // executor restored the original) for file edits.
            evidence::record(
                conn,
                task_id,
                task.correlation_id.as_ref().map(|s| s.as_str()),
                kind::VERIFICATION,
                verification,
                error.is_none(), // success = true if no error
            )?;
            if let Some(note) = rollback_note {
                evidence::record(
                    conn,
                    task_id,
                    task.correlation_id.as_ref().map(|s| s.as_str()),
                    kind::ROLLBACK,
                    note,
                    false, // rollback indicates failure
                )?;
            }
        }

        // Sprint 4: judge that evidence through the shared
        // PromotionController - PROMOTED for a verified pass, BLOCKED for
        // a failure/rollback - on the task's correlation chain.
        let _ = crate::governance::promotion::request_promotion(conn, task_id, task.correlation_id.as_deref());

        // Record ledger events for task completion
        match final_status {
            status::COMPLETED => {
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskCompleted,
                    task.correlation_id.as_ref().map(|s| s.as_str()),
                    task.requirement_id.as_ref().map(|s| s.as_str()),
                    Some(&task_id),
                    serde_json::json!({
                        "verification": verification,
                        "error": error,
                        "rollback_occurred": rollback_note.is_some()
                    }),
                );
            },
            status::FAILED => {
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskFailed,
                    task.correlation_id.as_ref().map(|s| s.as_str()),
                    task.requirement_id.as_ref().map(|s| s.as_str()),
                    Some(&task_id),
                    serde_json::json!({
                        "verification": verification,
                        "error": error,
                        "rollback_occurred": rollback_note.is_some()
                    }),
                );
            },
            status::ROLLED_BACK => {
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskRolledBack,
                    task.correlation_id.as_ref().map(|s| s.as_str()),
                    task.requirement_id.as_ref().map(|s| s.as_str()),
                    Some(&task_id),
                    serde_json::json!({
                        "verification": verification,
                        "error": error,
                        "rollback_occurred": rollback_note.is_some()
                    }),
                );
            },
            _ => {}, // Don't record events for intermediate states
        }

        // Sprint 3: if this task is a DAG node and it just failed or rolled
        // back, its dependents can never run - mark them blocked now.
        // Sprint 8: if it COMPLETED and is a retry, previously-blocked
        // dependents of the failed attempt become plannable again.
        if let Some(dag_id) = &task.dag_id {
            if matches!(final_status, status::FAILED | status::ROLLED_BACK) {
                let _ = propagate_dag_blocks(conn, dag_id);
            } else if final_status == status::COMPLETED && task.retry_of.is_some() {
                let _ = reopen_blocked_dependents(conn, dag_id);
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub fn reject_task(db: tauri::State<crate::database::DbState>, task_id: String) -> AppResult<()> {
    let task = crate::database::with_conn(&db, |conn| {
        update_status(conn, &task_id, status::REJECTED, None, None)?;
        get_task(conn, &task_id)
    })?;
    
    // Record ledger event for task rejection
    let _ = crate::database::with_conn(&db, |conn| {
        ledger::append(
            conn,
            LedgerEvent::TaskRejected,
            task.correlation_id.as_ref().map(|s| s.as_str()),
            task.requirement_id.as_ref().map(|s| s.as_str()),
            Some(&task_id),
            serde_json::json!({
                "reason": "user_rejection"
            }),
        )
    });
    
    tracing::info!(target: "agent", event = "task_rejected", task_id = %task_id);
    Ok(())
}

#[tauri::command]
pub fn list_agent_tasks(db: tauri::State<crate::database::DbState>) -> AppResult<Vec<AgentTask>> {
    crate::database::with_conn(&db, list_tasks)
}

/// All tasks belonging to one DAG, in insertion order.
pub fn list_dag_tasks(conn: &Connection, dag_id: &str) -> AppResult<Vec<AgentTask>> {
    let mut stmt = conn
        .prepare("SELECT * FROM agent_tasks WHERE dag_id = ?1 ORDER BY created_at ASC, id ASC")
        .map_err(|e| AppError::Provider(format!("failed to query DAG tasks: {e}")))?;
    let rows = stmt
        .query_map(params![dag_id], row_to_task)
        .map_err(|e| AppError::Provider(format!("failed to query DAG tasks: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read task row: {e}")))
}

/// Sprint 8: how a dependency looks once retry lineage is considered. A
/// dependency is satisfied if IT or any retry of it (transitively)
/// completed; it is pending while any attempt in the lineage is still
/// in flight; it has failed only when every attempt is terminally failed.
/// With no retry rows this reduces exactly to the Sprint 3 single-row
/// judgment - the no-retry path is behavior-identical.
#[derive(PartialEq, Clone, Copy)]
enum DepState {
    Completed,
    Pending,
    Failed,
}

fn dep_state_with_lineage(tasks: &[AgentTask], dep_id: &str) -> DepState {
    // Collect dep_id plus every transitive retry-descendant (cycle-guarded).
    let mut lineage: Vec<&AgentTask> = Vec::new();
    let mut frontier: Vec<&str> = vec![dep_id];
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    while let Some(id) = frontier.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(t) = tasks.iter().find(|t| t.id == id) {
            lineage.push(t);
        }
        for t in tasks.iter().filter(|t| t.retry_of.as_deref() == Some(id)) {
            frontier.push(t.id.as_str());
        }
    }
    if lineage.is_empty() {
        return DepState::Failed; // missing dep row = never runnable
    }
    if lineage.iter().any(|t| t.status == status::COMPLETED) {
        return DepState::Completed;
    }
    if lineage.iter().any(|t| matches!(t.status.as_str(), status::PLANNING | status::AWAITING_APPROVAL | status::APPLYING)) {
        return DepState::Pending;
    }
    DepState::Failed
}

/// Sprint 3 DAG walk, step one: which tasks may run RIGHT NOW. A task is
/// runnable when it hasn't reached a terminal state and every dependency
/// is COMPLETED (directly, or - Sprint 8 - via a completed retry).
/// Failed dependencies make a task blocked (handled by
/// propagate_dag_blocks), never runnable. Independent branches stay
/// runnable regardless of failures elsewhere in the DAG.
pub fn dag_runnable_tasks(conn: &Connection, dag_id: &str) -> AppResult<Vec<AgentTask>> {
    let tasks = list_dag_tasks(conn, dag_id)?;

    Ok(tasks
        .iter()
        .filter(|t| {
            matches!(t.status.as_str(), status::PLANNING | status::AWAITING_APPROVAL)
                && t.depends_on.iter().all(|d| dep_state_with_lineage(&tasks, d) == DepState::Completed)
        })
        .cloned()
        .collect())
}

/// Sprint 3 DAG walk, step two: after any task reaches a terminal failure
/// state, mark every (transitive) dependent as BLOCKED. Iterates to a
/// fixpoint so chains block all the way down. Each newly blocked task is
/// ledgered under the DAG's correlation_id - the Sprint 2 chain records
/// why work never happened, not just work that did.
pub fn propagate_dag_blocks(conn: &Connection, dag_id: &str) -> AppResult<Vec<String>> {
    let mut newly_blocked = Vec::new();
    loop {
        let tasks = list_dag_tasks(conn, dag_id)?;

        let mut changed = false;
        for task in &tasks {
            if matches!(task.status.as_str(), status::COMPLETED | status::FAILED | status::ROLLED_BACK | status::BLOCKED | status::REJECTED) {
                continue;
            }
            // Sprint 8: lineage-aware - a dependency with a pending or
            // completed retry is NOT failed, so its dependents don't block.
            let has_failed_dep = task.depends_on.iter().any(|d| dep_state_with_lineage(&tasks, d) == DepState::Failed);
            if has_failed_dep {
                update_status(conn, &task.id, status::BLOCKED, None, Some("a dependency failed - task was never attempted"))?;
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskFailed,
                    task.correlation_id.as_deref(),
                    task.requirement_id.as_deref(),
                    Some(&task.id),
                    serde_json::json!({ "dag_id": dag_id, "blocked": true, "reason": "dependency failed" }),
                );
                newly_blocked.push(task.id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(newly_blocked)
}

/// Sprint 8: the inverse of propagate_dag_blocks. After a retry succeeds,
/// BLOCKED dependents whose failed dependencies are now covered by a
/// completed retry-descendant become PLANNING again - recoverable, exactly
/// as if the dependency had succeeded first time. In a DAG with no retry
/// rows this can never fire (a blocked task's deps have no completed
/// lineage), so the Sprint 3 no-retry behavior is untouched.
pub fn reopen_blocked_dependents(conn: &Connection, dag_id: &str) -> AppResult<Vec<String>> {
    let mut reopened = Vec::new();
    loop {
        let tasks = list_dag_tasks(conn, dag_id)?;
        let mut changed = false;
        for task in &tasks {
            if task.status != status::BLOCKED {
                continue;
            }
            let all_deps_completed = task.depends_on.iter().all(|d| dep_state_with_lineage(&tasks, d) == DepState::Completed);
            if all_deps_completed {
                update_status(conn, &task.id, status::PLANNING, None, None)?;
                let _ = ledger::append(
                    conn,
                    LedgerEvent::TaskPlanned,
                    task.correlation_id.as_deref(),
                    task.requirement_id.as_deref(),
                    Some(&task.id),
                    serde_json::json!({ "dag_id": dag_id, "reopened": true, "reason": "a retry of a failed dependency completed" }),
                );
                reopened.push(task.id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(reopened)
}

/// Sprint 8: creates the retry row itself - a NEW task cloning the failed
/// attempt's work (objective, file, contents, requirement/correlation
/// linkage, DAG membership, worker assignment) with retry_of pointing at
/// it. Status is AWAITING_APPROVAL: retries prepare work for the same
/// human gate every task goes through; nothing executes unattended.
pub fn insert_retry_task(conn: &Connection, failed: &AgentTask, new_id: &str) -> AppResult<()> {
    let (original_content, proposed_content) = get_task_content(conn, &failed.id)?;
    insert_task(
        conn,
        new_id,
        &failed.objective,
        &failed.task_type,
        failed.files.first().map(|s| s.as_str()).unwrap_or_default(),
        status::AWAITING_APPROVAL,
        &original_content,
        &proposed_content,
        failed.risk_summary.as_deref().unwrap_or_default(),
        failed.requirement_id.as_deref(),
        failed.correlation_id.as_deref(),
    )?;
    conn.execute(
        "UPDATE agent_tasks SET retry_of = ?1, worker_id = (SELECT worker_id FROM agent_tasks WHERE id = ?1) WHERE id = ?2",
        params![failed.id, new_id],
    )
    .map_err(|e| AppError::Provider(format!("failed to link retry lineage: {e}")))?;
    if let Some(dag_id) = &failed.dag_id {
        set_dag_membership(conn, new_id, dag_id, &failed.depends_on)?;
    }
    Ok(())
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

    /// Audit remediation 1: the post-execution read-back with the
    /// connection gone returns a clean, explanatory error - it must not
    /// panic (the old code was `guard.as_ref().unwrap()`).
    #[test]
    fn read_task_after_finish_with_no_connection_errors_instead_of_panicking() {
        let err = match read_task_after_finish(None, "any-task") {
            Err(e) => e.to_string(),
            Ok(_) => panic!("must error with no connection"),
        };
        assert!(err.contains("workspace was closed"), "got: {err}");

        // And with a live connection it still reads the task (success path
        // preserved).
        let (dir, conn) = temp_conn();
        insert_task(&conn, "rb-task", "obj", task_type::EDIT_FILE, "f.rs", status::COMPLETED, "o", "n", "low", None, None).unwrap();
        assert_eq!(read_task_after_finish(Some(&conn), "rb-task").unwrap().id, "rb-task");
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Audit remediation 2: the whole task outcome (status + evidence +
    /// promotion + ledger) is ONE transaction. Sabotage the evidence table
    /// mid-sequence (rename it away) and the ENTIRE outcome must roll back
    /// - no COMPLETED task with missing evidence/promotion can exist.
    #[test]
    fn task_outcome_is_all_or_nothing() {
        let (dir, conn) = temp_conn();
        insert_task(&conn, "atomic-task", "obj", task_type::EDIT_FILE, "f.rs", status::APPLYING, "old", "new", "low", Some("req-1"), Some("corr-atomic")).unwrap();
        let task = get_task(&conn, "atomic-task").unwrap();
        let ledger_before: i64 = conn.query_row("SELECT COUNT(*) FROM ledger_entries", [], |r| r.get(0)).unwrap();

        // Sabotage: the evidence write mid-transaction will fail.
        conn.execute("ALTER TABLE evidence RENAME TO evidence_sabotaged", []).unwrap();
        let result = record_task_outcome_atomic(&conn, &task, "atomic-task", status::COMPLETED, "cargo check passed", None, None);
        conn.execute("ALTER TABLE evidence_sabotaged RENAME TO evidence", []).unwrap();

        assert!(result.is_err(), "a failed evidence write must fail the outcome");
        // NOTHING partial persisted: status untouched, no ledger growth,
        // no evidence, no promotion.
        assert_eq!(get_task(&conn, "atomic-task").unwrap().status, status::APPLYING, "status update must have rolled back");
        let ledger_after: i64 = conn.query_row("SELECT COUNT(*) FROM ledger_entries", [], |r| r.get(0)).unwrap();
        assert_eq!(ledger_before, ledger_after, "no ledger events may survive the rollback");
        assert!(crate::governance::evidence::for_task(&conn, "atomic-task").unwrap().is_empty());
        assert!(crate::governance::promotion::for_task(&conn, "atomic-task").unwrap().is_empty());

        // Un-sabotaged, the identical call commits everything together.
        record_task_outcome_atomic(&conn, &task, "atomic-task", status::COMPLETED, "cargo check passed", None, None).unwrap();
        assert_eq!(get_task(&conn, "atomic-task").unwrap().status, status::COMPLETED);
        let evidence = crate::governance::evidence::for_task(&conn, "atomic-task").unwrap();
        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].success);
        let promotions = crate::governance::promotion::for_task(&conn, "atomic-task").unwrap();
        assert_eq!(promotions.len(), 1);
        assert_eq!(promotions[0].status, crate::governance::promotion::status::PROMOTED);
        let chain = crate::governance::ledger::list_by_correlation(&conn, "corr-atomic").unwrap();
        assert!(chain.iter().any(|e| e.event_type == "task_completed"));

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Audit remediation 2, failure side: a rolled-back task's outcome
    /// (rollback evidence + BLOCKED promotion + task_rolled_back event)
    /// also lands atomically and with the same content as before the
    /// remediation.
    #[test]
    fn rolled_back_task_outcome_writes_rollback_evidence_and_blocked_promotion() {
        let (dir, conn) = temp_conn();
        insert_task(&conn, "rb-atomic", "obj", task_type::EDIT_FILE, "f.rs", status::APPLYING, "old", "new", "low", None, Some("corr-rb")).unwrap();
        let task = get_task(&conn, "rb-atomic").unwrap();

        record_task_outcome_atomic(
            &conn, &task, "rb-atomic", status::ROLLED_BACK,
            "cargo check failed:\nerror[E0308]", Some("verification failed"),
            Some("original content restored after failed verification"),
        ).unwrap();

        let task = get_task(&conn, "rb-atomic").unwrap();
        assert_eq!(task.status, status::ROLLED_BACK);
        let evidence = crate::governance::evidence::for_task(&conn, "rb-atomic").unwrap();
        assert_eq!(evidence.len(), 2, "verification + rollback evidence");
        assert_eq!(evidence[0].kind, crate::governance::evidence::kind::VERIFICATION);
        assert!(!evidence[0].success);
        assert_eq!(evidence[1].kind, crate::governance::evidence::kind::ROLLBACK);
        let promotions = crate::governance::promotion::for_task(&conn, "rb-atomic").unwrap();
        assert_eq!(promotions[0].status, crate::governance::promotion::status::BLOCKED);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn insert_get_update_roundtrip() {
        let (dir, conn) = temp_conn();

        insert_task(&conn, "task-1", "add a comment", task_type::EDIT_FILE, "main.rs", status::AWAITING_APPROVAL, "old", "new", "1 line changed", Some("req-1"), Some("corr-1")).unwrap();

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

        assert_eq!(task.requirement_id.as_deref(), Some("req-1"));
        assert_eq!(task.correlation_id.as_deref(), Some("corr-1"));

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Sprint 1 gate at the level create_and_plan_task actually calls
    /// it (the command itself needs a live tauri::State - see the
    /// MockRuntime decision in decisions.md - so the gate function is
    /// tested directly, same pattern as every other command-layer test).
    #[test]
    fn planning_gate_refuses_missing_or_retired_requirement() {
        let (dir, conn) = temp_conn();

        // Missing requirement: hard error, and no task row may exist.
        assert!(crate::governance::requirements::get_active(&conn, "no-such-requirement").is_err());
        assert!(list_tasks(&conn).unwrap().is_empty());

        // Retired requirement: also refused.
        let req = crate::governance::requirements::create(
            &conn,
            "Personalize greeting",
            "The greeting should address the user by name",
            vec!["the output contains the user's name".to_string()],
            "test-user",
        )
        .unwrap();
        crate::governance::requirements::set_status(&conn, &req.id, crate::governance::requirements::status::RETIRED).unwrap();
        assert!(crate::governance::requirements::get_active(&conn, &req.id).is_err());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end at the pure-core level: a valid requirement produces an
    /// objective containing its intent and every acceptance criterion, and
    /// the resulting task row carries the requirement's IDs - the exact
    /// data flow create_and_plan_task performs before calling the planner.
    #[test]
    fn valid_requirement_flows_into_task_objective_and_links() {
        let (dir, conn) = temp_conn();

        let req = crate::governance::requirements::create(
            &conn,
            "Personalize greeting",
            "The greeting should address the user by name",
            vec!["the output contains the user's name".to_string(), "existing tests still pass".to_string()],
            "test-user",
        )
        .unwrap();

        let active = crate::governance::requirements::get_active(&conn, &req.id).unwrap();
        let objective = objective_from_requirement(&active);
        assert!(objective.contains("address the user by name"));
        assert!(objective.contains("- the output contains the user's name"));
        assert!(objective.contains("- existing tests still pass"));

        insert_task(
            &conn,
            "gated-task",
            &objective,
            task_type::EDIT_FILE,
            "main.rs",
            status::PLANNING,
            "old",
            "",
            "",
            Some(&req.id),
            Some(&req.correlation_id),
        )
        .unwrap();

        let task = get_task(&conn, "gated-task").unwrap();
        assert_eq!(task.requirement_id.as_deref(), Some(req.id.as_str()));
        assert_eq!(task.correlation_id.as_deref(), Some(req.correlation_id.as_str()), "task must share the requirement's correlation ID");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_code_task_reports_no_files_since_it_never_touches_the_workspace() {
        let (dir, conn) = temp_conn();

        insert_task(&conn, "code-task-1", "print 42", task_type::RUN_CODE, "", status::AWAITING_APPROVAL, "", "print(42)", "low risk", None, None).unwrap();

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
        insert_task(&conn, id, objective, task_type::RUN_CODE, "", status::PLANNING, "", "", "", None, None).unwrap();
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
