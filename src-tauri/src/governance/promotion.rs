use super::evidence;
use super::ledger::{self, LedgerEvent};
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Sprint 4: one controller for both promotion patterns - Phase 5's
/// approve-then-apply agent flow and Phase 7's bootstrap branch/commit/test
/// flow. "Approval" now means exactly one thing everywhere: sufficient
/// passing evidence exists for the task. Promotion is the separate,
/// explicit step that consumes that evidence, and it is recorded as a
/// first-class row plus ledger events under the task's correlation chain.
#[derive(Serialize, Clone, Debug)]
pub struct PromotionRequest {
    pub id: String,
    /// The evidence row this promotion was judged against. Empty string
    /// when promotion was requested for a task with NO evidence at all -
    /// the row still exists (the refusal is auditable) but references
    /// nothing.
    pub evidence_id: String,
    pub task_id: String,
    pub status: String,
    pub requested_at: i64,
    pub promoted_at: Option<i64>,
}

pub mod status {
    pub const REQUESTED: &str = "requested";
    pub const PROMOTED: &str = "promoted";
    pub const BLOCKED: &str = "blocked";
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn insert_request(conn: &Connection, req: &PromotionRequest) -> AppResult<()> {
    conn.execute(
        "INSERT INTO promotion_requests (id, evidence_id, task_id, status, requested_at, promoted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            req.id,
            if req.evidence_id.is_empty() { None } else { Some(req.evidence_id.as_str()) },
            req.task_id,
            req.status,
            req.requested_at,
            req.promoted_at
        ],
    )
    .map_err(|e| AppError::Provider(format!("failed to record promotion request: {e}")))?;
    Ok(())
}

/// The evidence gate. Judges the task's LATEST evidence record (the most
/// recent verification is the one that describes the current state of the
/// change): no evidence -> BLOCKED, failing evidence -> BLOCKED, passing
/// evidence -> PROMOTED. Every outcome persists a promotion_requests row
/// and two ledger events (promotion_requested, then approved/blocked)
/// under the task's correlation chain - refusals are as auditable as
/// promotions.
pub fn request_promotion(conn: &Connection, task_id: &str, correlation_id: Option<&str>) -> AppResult<PromotionRequest> {
    let all_evidence = evidence::for_task(conn, task_id)?;
    let latest = all_evidence.last();

    let requested_at = now_secs();
    let (evidence_id, verdict, reason) = match latest {
        None => (String::new(), status::BLOCKED, "no evidence exists for this task"),
        Some(ev) if !ev.success => (ev.id.clone(), status::BLOCKED, "latest evidence records a failure"),
        Some(ev) => (ev.id.clone(), status::PROMOTED, "latest evidence records a pass"),
    };

    let req = PromotionRequest {
        id: uuid::Uuid::new_v4().to_string(),
        evidence_id,
        task_id: task_id.to_string(),
        status: verdict.to_string(),
        requested_at,
        promoted_at: if verdict == status::PROMOTED { Some(requested_at) } else { None },
    };
    insert_request(conn, &req)?;

    let _ = ledger::append(
        conn,
        LedgerEvent::PromotionRequested,
        correlation_id,
        None,
        Some(task_id),
        serde_json::json!({ "promotion_id": req.id, "evidence_id": req.evidence_id }),
    );
    let (event, level) = if verdict == status::PROMOTED {
        (LedgerEvent::PromotionApproved, "promoted")
    } else {
        (LedgerEvent::PromotionBlocked, "blocked")
    };
    let _ = ledger::append(
        conn,
        event,
        correlation_id,
        None,
        Some(task_id),
        serde_json::json!({ "promotion_id": req.id, "evidence_id": req.evidence_id, "reason": reason }),
    );
    tracing::info!(target: "governance", event = "promotion_judged", task_id = %task_id, outcome = level, reason = reason);
    Ok(req)
}

pub fn get(conn: &Connection, promotion_id: &str) -> AppResult<PromotionRequest> {
    conn.query_row("SELECT * FROM promotion_requests WHERE id = ?1", params![promotion_id], |row| {
        Ok(PromotionRequest {
            id: row.get("id")?,
            evidence_id: row.get::<_, Option<String>>("evidence_id")?.unwrap_or_default(),
            task_id: row.get("task_id")?,
            status: row.get("status")?,
            requested_at: row.get("requested_at")?,
            promoted_at: row.get("promoted_at")?,
        })
    })
    .map_err(|_| AppError::NotFound(format!("promotion request {promotion_id}")))
}

pub fn for_task(conn: &Connection, task_id: &str) -> AppResult<Vec<PromotionRequest>> {
    let mut stmt = conn
        .prepare("SELECT * FROM promotion_requests WHERE task_id = ?1 ORDER BY requested_at ASC, rowid ASC")
        .map_err(|e| AppError::Provider(format!("failed to query promotions: {e}")))?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(PromotionRequest {
                id: row.get("id")?,
                evidence_id: row.get::<_, Option<String>>("evidence_id")?.unwrap_or_default(),
                task_id: row.get("task_id")?,
                status: row.get("status")?,
                requested_at: row.get("requested_at")?,
                promoted_at: row.get("promoted_at")?,
            })
        })
        .map_err(|e| AppError::Provider(format!("failed to query promotions: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read promotion row: {e}")))
}

/// The single shared file-mutation primitive both promotion flows use.
/// Phase 7's bootstrap git flow routes its write through here; Phase 5's
/// agent flow applies via the (untouched) executor, which is why
/// apply_to_workspace below re-checks the promotion row instead of
/// assuming the caller behaved.
pub fn write_promoted_content(target: &Path, content: &str) -> AppResult<()> {
    std::fs::write(target, content)?;
    Ok(())
}

/// The explicit, gated apply step: refuses unless the promotion row says
/// PROMOTED (re-read from the DB, not trusted from the caller), then
/// performs the same workspace-containment check the agent flow uses and
/// writes via the shared primitive.
pub fn apply_to_workspace(
    conn: &Connection,
    promotion_id: &str,
    workspace_root: &Path,
    file_path: &str,
    content: &str,
) -> AppResult<()> {
    let promotion = get(conn, promotion_id)?;
    if promotion.status != status::PROMOTED {
        return Err(AppError::Provider(format!(
            "promotion {promotion_id} is '{}', not promoted - refusing to apply (evidence gate)",
            promotion.status
        )));
    }

    let target = workspace_root.join(file_path);
    let canonical_root = std::fs::canonicalize(workspace_root)?;
    let canonical_target = std::fs::canonicalize(&target).map_err(|_| AppError::NotFound(file_path.to_string()))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(format!("{file_path} is outside the workspace")));
    }

    write_promoted_content(&canonical_target, content)?;
    tracing::info!(target: "governance", event = "promotion_applied", promotion_id = %promotion_id, file = %file_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::evidence::kind;

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_promotion_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    /// Test 1: no evidence at all -> BLOCKED row, ledgered, apply refused.
    #[test]
    fn promotion_without_evidence_is_blocked() {
        let (dir, conn) = temp_conn();

        let req = request_promotion(&conn, "task-no-evidence", Some("corr-p1")).unwrap();
        assert_eq!(req.status, status::BLOCKED);
        assert!(req.promoted_at.is_none());
        assert!(req.evidence_id.is_empty());

        // The BLOCKED verdict is a real row, re-readable from the DB.
        let persisted = get(&conn, &req.id).unwrap();
        assert_eq!(persisted.status, status::BLOCKED);

        // And the refusal is on the correlation chain.
        let chain = ledger::list_by_correlation(&conn, "corr-p1").unwrap();
        assert!(chain.iter().any(|e| e.event_type == "promotion_requested"));
        assert!(chain.iter().any(|e| e.event_type == "promotion_blocked"));
        assert!(!chain.iter().any(|e| e.event_type == "promotion_approved"));

        // apply_to_workspace refuses a blocked promotion outright.
        std::fs::write(dir.join("f.md"), "old").unwrap();
        let err = apply_to_workspace(&conn, &req.id, &dir, "f.md", "new").unwrap_err().to_string();
        assert!(err.contains("refusing to apply"), "got: {err}");
        assert_eq!(std::fs::read_to_string(dir.join("f.md")).unwrap(), "old", "file must be untouched");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 2: evidence exists but records a failure -> BLOCKED.
    #[test]
    fn promotion_with_failing_evidence_is_blocked() {
        let (dir, conn) = temp_conn();

        evidence::record(&conn, "task-fail-ev", Some("corr-p2"), kind::VERIFICATION, "cargo check failed:\nerror[E0308]", false).unwrap();
        let req = request_promotion(&conn, "task-fail-ev", Some("corr-p2")).unwrap();
        assert_eq!(req.status, status::BLOCKED);
        assert!(!req.evidence_id.is_empty(), "blocked-on-failure still references the evidence it judged");

        let chain = ledger::list_by_correlation(&conn, "corr-p2").unwrap();
        assert!(chain.iter().any(|e| e.event_type == "promotion_blocked"));

        std::fs::write(dir.join("f.md"), "old").unwrap();
        assert!(apply_to_workspace(&conn, &req.id, &dir, "f.md", "new").is_err());
        assert_eq!(std::fs::read_to_string(dir.join("f.md")).unwrap(), "old");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 3: passing evidence -> PROMOTED, and apply_to_workspace
    /// actually changes the file in the workspace.
    #[test]
    fn promotion_with_passing_evidence_promotes_and_applies() {
        let (dir, conn) = temp_conn();

        evidence::record(&conn, "task-pass-ev", Some("corr-p3"), kind::VERIFICATION, "cargo check passed", true).unwrap();
        let req = request_promotion(&conn, "task-pass-ev", Some("corr-p3")).unwrap();
        assert_eq!(req.status, status::PROMOTED);
        assert!(req.promoted_at.is_some());

        let chain = ledger::list_by_correlation(&conn, "corr-p3").unwrap();
        assert!(chain.iter().any(|e| e.event_type == "promotion_approved"));
        assert!(!chain.iter().any(|e| e.event_type == "promotion_blocked"));

        std::fs::write(dir.join("f.md"), "old").unwrap();
        apply_to_workspace(&conn, &req.id, &dir, "f.md", "promoted content").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("f.md")).unwrap(), "promoted content", "the file must actually change");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The gate judges the LATEST evidence: an early failure followed by a
    /// later pass promotes; the reverse blocks.
    #[test]
    fn latest_evidence_wins() {
        let (dir, conn) = temp_conn();

        evidence::record(&conn, "task-retry", None, kind::VERIFICATION, "cargo check failed", false).unwrap();
        evidence::record(&conn, "task-retry", None, kind::VERIFICATION, "cargo check passed", true).unwrap();
        assert_eq!(request_promotion(&conn, "task-retry", None).unwrap().status, status::PROMOTED);

        evidence::record(&conn, "task-regress", None, kind::VERIFICATION, "cargo check passed", true).unwrap();
        evidence::record(&conn, "task-regress", None, kind::ROLLBACK, "restored after failed verification", false).unwrap();
        assert_eq!(request_promotion(&conn, "task-regress", None).unwrap().status, status::BLOCKED);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_rejects_path_outside_workspace_even_when_promoted() {
        let (dir, conn) = temp_conn();
        evidence::record(&conn, "task-esc", None, kind::VERIFICATION, "ok", true).unwrap();
        let req = request_promotion(&conn, "task-esc", None).unwrap();
        assert_eq!(req.status, status::PROMOTED);
        assert!(apply_to_workspace(&conn, &req.id, &dir, "../evil.md", "x").is_err());
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
