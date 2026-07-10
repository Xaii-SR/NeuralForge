use super::ledger::{self, LedgerEvent};
use super::validator::{validate, RequirementInput};
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// The validated contract that must exist before the agent planner will
/// accept a task. Everything the planner consumes (intent + acceptance
/// criteria) lives here instead of in a raw prompt string, and every
/// version of it is preserved in requirement_history.
#[derive(Serialize, Deserialize, Clone)]
pub struct RequirementContract {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub intent: String,
    pub acceptance_criteria: Vec<String>,
    pub status: String,
    /// Shared across the requirement and everything downstream of it
    /// (tasks now, evidence/ledger records in Sprint 2) so an entire
    /// chain of work is queryable from one ID.
    pub correlation_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: String,
}

/// One row per version, written on create and on every update - the
/// requirement's append-only history, queryable by requirement or by
/// correlation ID.
#[derive(Serialize, Clone)]
pub struct RequirementHistoryEntry {
    pub requirement_id: String,
    pub version: i64,
    pub status: String,
    pub title: String,
    pub intent: String,
    pub acceptance_criteria: Vec<String>,
    pub changed_at: i64,
}

pub mod status {
    /// Validated and usable by the planner.
    pub const ACTIVE: &str = "active";
    /// No longer usable for new tasks; kept for traceability.
    pub const RETIRED: &str = "retired";
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn criteria_to_json(criteria: &[String]) -> AppResult<String> {
    serde_json::to_string(criteria).map_err(|e| AppError::Provider(format!("failed to encode acceptance criteria: {e}")))
}

fn criteria_from_json(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn append_history(conn: &Connection, req: &RequirementContract) -> AppResult<()> {
    conn.execute(
        "INSERT INTO requirement_history (requirement_id, version, status, title, intent, acceptance_criteria, changed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![req.id, req.version, req.status, req.title, req.intent, criteria_to_json(&req.acceptance_criteria)?, req.updated_at],
    )
    .map_err(|e| AppError::Provider(format!("failed to record requirement history: {e}")))?;
    Ok(())
}

/// Rejection payloads keep enough of the submitted input to audit what
/// was rejected and why, without letting arbitrarily large garbage
/// input bloat the ledger.
const MAX_REJECTED_FIELD_CHARS: usize = 500;

fn truncated(s: &str) -> String {
    if s.len() > MAX_REJECTED_FIELD_CHARS {
        let mut t: String = s.chars().take(MAX_REJECTED_FIELD_CHARS).collect();
        t.push_str("...(truncated)");
        t
    } else {
        s.to_string()
    }
}

fn rejection_payload(title: &str, intent: &str, criteria_count: usize, problems: &[String]) -> serde_json::Value {
    serde_json::json!({
        "title": truncated(title),
        "intent": truncated(intent),
        "criteria_count": criteria_count,
        "problems": problems,
    })
}

/// Validates first, inserts only if validation passes - a weak request
/// never becomes a row, let alone reaches the planner or an LLM. The
/// rejection itself IS recorded: as a ledger event with no
/// correlation_id (no lifecycle chain was ever born), never as a
/// requirements row.
pub fn create(conn: &Connection, title: &str, intent: &str, acceptance_criteria: Vec<String>, created_by: &str) -> AppResult<RequirementContract> {
    if let Err(problems) = validate(&RequirementInput { title, intent, acceptance_criteria: &acceptance_criteria }) {
        ledger::append(
            conn,
            LedgerEvent::RequirementRejected,
            None,
            None,
            None,
            rejection_payload(title, intent, acceptance_criteria.len(), &problems),
        )?;
        return Err(AppError::Provider(format!("requirement rejected: {}", problems.join("; "))));
    }

    let now = now_secs();
    let req = RequirementContract {
        id: uuid::Uuid::new_v4().to_string(),
        version: 1,
        title: title.trim().to_string(),
        intent: intent.trim().to_string(),
        acceptance_criteria,
        status: status::ACTIVE.to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        created_at: now,
        updated_at: now,
        created_by: created_by.to_string(),
    };

    conn.execute(
        "INSERT INTO requirements (id, version, title, intent, acceptance_criteria, status, correlation_id, created_at, updated_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            req.id,
            req.version,
            req.title,
            req.intent,
            criteria_to_json(&req.acceptance_criteria)?,
            req.status,
            req.correlation_id,
            req.created_at,
            req.updated_at,
            req.created_by
        ],
    )
    .map_err(|e| AppError::Provider(format!("failed to create requirement: {e}")))?;

    append_history(conn, &req)?;
    ledger::append(
        conn,
        LedgerEvent::RequirementCreated,
        Some(&req.correlation_id),
        Some(&req.id),
        None,
        serde_json::json!({ "title": req.title, "version": req.version, "created_by": created_by }),
    )?;
    tracing::info!(target: "governance", event = "requirement_created", requirement_id = %req.id, correlation_id = %req.correlation_id);
    Ok(req)
}

/// Re-validates the new content, bumps the version, and appends a history
/// row. The requirement's id and correlation_id never change across
/// versions - that's the whole point of them. A rejected update is
/// ledgered against the requirement's real correlation_id, since here
/// (unlike a rejected create) the lifecycle chain does exist.
pub fn update(conn: &Connection, id: &str, title: &str, intent: &str, acceptance_criteria: Vec<String>) -> AppResult<RequirementContract> {
    if let Err(problems) = validate(&RequirementInput { title, intent, acceptance_criteria: &acceptance_criteria }) {
        let correlation = get(conn, id).ok().map(|r| r.correlation_id);
        ledger::append(
            conn,
            LedgerEvent::RequirementUpdateRejected,
            correlation.as_deref(),
            Some(id),
            None,
            rejection_payload(title, intent, acceptance_criteria.len(), &problems),
        )?;
        return Err(AppError::Provider(format!("requirement rejected: {}", problems.join("; "))));
    }

    let mut req = get(conn, id)?;
    req.version += 1;
    req.title = title.trim().to_string();
    req.intent = intent.trim().to_string();
    req.acceptance_criteria = acceptance_criteria;
    req.updated_at = now_secs();

    conn.execute(
        "UPDATE requirements SET version = ?1, title = ?2, intent = ?3, acceptance_criteria = ?4, updated_at = ?5 WHERE id = ?6",
        params![req.version, req.title, req.intent, criteria_to_json(&req.acceptance_criteria)?, req.updated_at, req.id],
    )
    .map_err(|e| AppError::Provider(format!("failed to update requirement: {e}")))?;

    append_history(conn, &req)?;
    ledger::append(
        conn,
        LedgerEvent::RequirementUpdated,
        Some(&req.correlation_id),
        Some(&req.id),
        None,
        serde_json::json!({ "title": req.title, "version": req.version }),
    )?;
    tracing::info!(target: "governance", event = "requirement_updated", requirement_id = %req.id, version = req.version);
    Ok(req)
}

pub fn set_status(conn: &Connection, id: &str, new_status: &str) -> AppResult<RequirementContract> {
    if new_status != status::ACTIVE && new_status != status::RETIRED {
        return Err(AppError::Provider(format!("unknown requirement status: {new_status}")));
    }
    let mut req = get(conn, id)?;
    req.status = new_status.to_string();
    req.updated_at = now_secs();

    conn.execute(
        "UPDATE requirements SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![req.status, req.updated_at, req.id],
    )
    .map_err(|e| AppError::Provider(format!("failed to update requirement status: {e}")))?;

    append_history(conn, &req)?;
    let event = if new_status == status::RETIRED { LedgerEvent::RequirementRetired } else { LedgerEvent::RequirementReactivated };
    ledger::append(
        conn,
        event,
        Some(&req.correlation_id),
        Some(&req.id),
        None,
        serde_json::json!({ "status": new_status }),
    )?;
    tracing::info!(target: "governance", event = "requirement_status_changed", requirement_id = %req.id, status = %new_status);
    Ok(req)
}

fn row_to_requirement(row: &rusqlite::Row) -> rusqlite::Result<RequirementContract> {
    let criteria_json: String = row.get("acceptance_criteria")?;
    Ok(RequirementContract {
        id: row.get("id")?,
        version: row.get("version")?,
        title: row.get("title")?,
        intent: row.get("intent")?,
        acceptance_criteria: criteria_from_json(&criteria_json),
        status: row.get("status")?,
        correlation_id: row.get("correlation_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        created_by: row.get("created_by")?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<RequirementContract> {
    conn.query_row("SELECT * FROM requirements WHERE id = ?1", params![id], row_to_requirement)
        .map_err(|_| AppError::NotFound(format!("requirement {id}")))
}

pub fn list(conn: &Connection) -> AppResult<Vec<RequirementContract>> {
    let mut stmt = conn
        .prepare("SELECT * FROM requirements ORDER BY created_at DESC")
        .map_err(|e| AppError::Provider(format!("failed to query requirements: {e}")))?;
    let rows = stmt
        .query_map([], row_to_requirement)
        .map_err(|e| AppError::Provider(format!("failed to query requirements: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read requirement row: {e}")))
}

pub fn history(conn: &Connection, requirement_id: &str) -> AppResult<Vec<RequirementHistoryEntry>> {
    let mut stmt = conn
        .prepare("SELECT * FROM requirement_history WHERE requirement_id = ?1 ORDER BY version ASC, changed_at ASC")
        .map_err(|e| AppError::Provider(format!("failed to query history: {e}")))?;
    let rows = stmt
        .query_map(params![requirement_id], |row| {
            let criteria_json: String = row.get("acceptance_criteria")?;
            Ok(RequirementHistoryEntry {
                requirement_id: row.get("requirement_id")?,
                version: row.get("version")?,
                status: row.get("status")?,
                title: row.get("title")?,
                intent: row.get("intent")?,
                acceptance_criteria: criteria_from_json(&criteria_json),
                changed_at: row.get("changed_at")?,
            })
        })
        .map_err(|e| AppError::Provider(format!("failed to query history: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read history row: {e}")))
}

/// The gate the planner calls: the requirement must exist AND be active.
/// A retired requirement is preserved for traceability but can't spawn
/// new work.
pub fn get_active(conn: &Connection, id: &str) -> AppResult<RequirementContract> {
    let req = get(conn, id)?;
    if req.status != status::ACTIVE {
        return Err(AppError::Provider(format!("requirement {id} is '{}', not active - it cannot gate new tasks", req.status)));
    }
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_governance_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    fn valid_criteria() -> Vec<String> {
        vec!["the greeting output contains the user's name".to_string()]
    }

    #[test]
    fn invalid_requirement_is_rejected_and_leaves_no_row() {
        let (dir, conn) = temp_conn();

        let result = create(&conn, "x", "fix", vec![], "test-user");
        assert!(result.is_err());
        assert!(list(&conn).unwrap().is_empty(), "a rejected requirement must not persist");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn valid_requirement_persists_and_round_trips() {
        let (dir, conn) = temp_conn();

        let req = create(&conn, "Personalize greeting", "The greeting should address the user by name", valid_criteria(), "test-user").unwrap();
        assert_eq!(req.version, 1);
        assert_eq!(req.status, status::ACTIVE);
        assert!(!req.correlation_id.is_empty());

        let fetched = get(&conn, &req.id).unwrap();
        assert_eq!(fetched.title, "Personalize greeting");
        assert_eq!(fetched.acceptance_criteria, valid_criteria());
        assert_eq!(fetched.correlation_id, req.correlation_id);

        assert_eq!(list(&conn).unwrap().len(), 1);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_bumps_version_and_preserves_history_of_both_versions() {
        let (dir, conn) = temp_conn();

        let req = create(&conn, "Personalize greeting", "The greeting should address the user by name", valid_criteria(), "test-user").unwrap();
        let updated = update(
            &conn,
            &req.id,
            "Personalize greeting warmly",
            "The greeting should address the user by name and wish them a good day",
            valid_criteria(),
        )
        .unwrap();

        assert_eq!(updated.version, 2);
        assert_eq!(updated.correlation_id, req.correlation_id, "correlation ID must survive versioning");

        let hist = history(&conn, &req.id).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].version, 1);
        assert_eq!(hist[0].title, "Personalize greeting");
        assert_eq!(hist[1].version, 2);
        assert_eq!(hist[1].title, "Personalize greeting warmly");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_with_invalid_content_is_rejected_and_version_unchanged() {
        let (dir, conn) = temp_conn();

        let req = create(&conn, "Personalize greeting", "The greeting should address the user by name", valid_criteria(), "test-user").unwrap();
        assert!(update(&conn, &req.id, "x", "no", vec![]).is_err());

        let unchanged = get(&conn, &req.id).unwrap();
        assert_eq!(unchanged.version, 1);
        assert_eq!(unchanged.title, "Personalize greeting");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn retired_requirement_fails_the_active_gate_but_stays_readable() {
        let (dir, conn) = temp_conn();

        let req = create(&conn, "Personalize greeting", "The greeting should address the user by name", valid_criteria(), "test-user").unwrap();
        set_status(&conn, &req.id, status::RETIRED).unwrap();

        assert!(get_active(&conn, &req.id).is_err(), "retired requirement must not gate new tasks");
        assert_eq!(get(&conn, &req.id).unwrap().status, status::RETIRED, "but it remains readable for traceability");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_status_is_rejected() {
        let (dir, conn) = temp_conn();
        let req = create(&conn, "Personalize greeting", "The greeting should address the user by name", valid_criteria(), "test-user").unwrap();
        assert!(set_status(&conn, &req.id, "bogus").is_err());
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
