use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Sprint 5: a worker the router can assign tasks to. Today only the Coder
/// agent type actually executes anything, so this registry is machinery -
/// the profiles, scoring, and reliability derivation are real, but routing
/// value stays capped until more worker types exist (expected per spec).
///
/// reliability_score / tasks_completed / tasks_failed are DERIVED
/// COLUMNS: refresh_reliability() recomputes them from the evidence and
/// promotion_requests rows Sprints 2-4 already write (via the additive
/// agent_tasks.worker_id assignment column). There is deliberately no
/// parallel tracking mechanism to drift out of sync - the governance
/// tables are the single source of truth and this table is a cache of
/// them.
#[derive(Type, Serialize, Deserialize, Clone, Debug)]
pub struct WorkerProfile {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub reliability_score: f64,
    pub tasks_completed: i64,
    pub tasks_failed: i64,
}

fn row_to_profile(row: &rusqlite::Row) -> rusqlite::Result<WorkerProfile> {
    let caps_json: String = row.get("capabilities")?;
    Ok(WorkerProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
        reliability_score: row.get("reliability_score")?,
        tasks_completed: row.get("tasks_completed")?,
        tasks_failed: row.get("tasks_failed")?,
    })
}

pub fn upsert(conn: &Connection, profile: &WorkerProfile) -> AppResult<()> {
    let caps_json =
        serde_json::to_string(&profile.capabilities).map_err(|e| AppError::Provider(format!("failed to encode capabilities: {e}")))?;
    conn.execute(
        "INSERT INTO worker_profiles (id, name, capabilities, reliability_score, tasks_completed, tasks_failed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, capabilities = excluded.capabilities,
             reliability_score = excluded.reliability_score,
             tasks_completed = excluded.tasks_completed, tasks_failed = excluded.tasks_failed",
        params![profile.id, profile.name, caps_json, profile.reliability_score, profile.tasks_completed, profile.tasks_failed],
    )
    .map_err(|e| AppError::Provider(format!("failed to upsert worker profile: {e}")))?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<WorkerProfile> {
    conn.query_row("SELECT * FROM worker_profiles WHERE id = ?1", params![id], row_to_profile)
        .map_err(|_| AppError::NotFound(format!("worker profile {id}")))
}

pub fn list(conn: &Connection) -> AppResult<Vec<WorkerProfile>> {
    let mut stmt = conn
        .prepare("SELECT * FROM worker_profiles ORDER BY name ASC")
        .map_err(|e| AppError::Provider(format!("failed to query worker profiles: {e}")))?;
    let rows = stmt
        .query_map([], row_to_profile)
        .map_err(|e| AppError::Provider(format!("failed to query worker profiles: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read worker profile row: {e}")))
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM worker_profiles WHERE id = ?1", params![id])
        .map_err(|e| AppError::Provider(format!("failed to delete worker profile: {e}")))?;
    Ok(())
}

/// Assigns a task to a worker - the additive link that lets reliability be
/// derived from governance data instead of tracked separately.
pub fn assign_task(conn: &Connection, task_id: &str, worker_id: &str) -> AppResult<()> {
    conn.execute("UPDATE agent_tasks SET worker_id = ?1 WHERE id = ?2", params![worker_id, task_id])
        .map_err(|e| AppError::Provider(format!("failed to assign task to worker: {e}")))?;
    Ok(())
}

/// Recomputes a worker's reliability from the promotion verdicts of its
/// assigned tasks (promotion_requests is the Sprint 4 judgment of the
/// Sprint 2 evidence, so counting promotions counts evidence outcomes
/// without duplicating the logic that judges them). One promotion verdict
/// per task - the LATEST - so retries don't double-count.
///
/// Score = promoted / (promoted + blocked); a worker with no judged work
/// keeps the optimistic default 1.0 (the schema default) rather than
/// being penalized for never having run.
pub fn refresh_reliability(conn: &Connection, worker_id: &str) -> AppResult<WorkerProfile> {
    let (completed, failed): (i64, i64) = conn
        .query_row(
            "WITH latest AS (
                 SELECT pr.task_id, pr.status,
                        -- rowid, not the random UUID id, breaks same-second
                        -- ties: insertion order is the true latest (same
                        -- lesson as evidence.insertion_sequence in Sprint 2).
                        ROW_NUMBER() OVER (PARTITION BY pr.task_id ORDER BY pr.requested_at DESC, pr.rowid DESC) AS rn
                 FROM promotion_requests pr
                 JOIN agent_tasks t ON t.id = pr.task_id
                 WHERE t.worker_id = ?1
             )
             SELECT
                 COALESCE(SUM(CASE WHEN status = 'promoted' THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN status != 'promoted' THEN 1 ELSE 0 END), 0)
             FROM latest WHERE rn = 1",
            params![worker_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| AppError::Provider(format!("failed to derive reliability: {e}")))?;

    let score = if completed + failed == 0 { 1.0 } else { completed as f64 / (completed + failed) as f64 };

    conn.execute(
        "UPDATE worker_profiles SET reliability_score = ?1, tasks_completed = ?2, tasks_failed = ?3 WHERE id = ?4",
        params![score, completed, failed, worker_id],
    )
    .map_err(|e| AppError::Provider(format!("failed to update reliability: {e}")))?;

    get(conn, worker_id)
}
