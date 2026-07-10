pub mod matcher;
pub mod registry;
pub mod reliability;

use crate::core::errors::AppResult;
use crate::database::{with_conn, DbState};
use tauri::State;

#[tauri::command]
pub fn list_worker_profiles(db: State<DbState>) -> AppResult<Vec<registry::WorkerProfile>> {
    with_conn(&db, registry::list)
}

#[tauri::command]
pub fn upsert_worker_profile(db: State<DbState>, profile: registry::WorkerProfile) -> AppResult<registry::WorkerProfile> {
    with_conn(&db, |conn| {
        registry::upsert(conn, &profile)?;
        registry::get(conn, &profile.id)
    })
}

#[tauri::command]
pub fn delete_worker_profile(db: State<DbState>, workerId: String) -> AppResult<()> {
    with_conn(&db, |conn| registry::delete(conn, &workerId))
}

#[tauri::command]
pub fn refresh_worker_reliability(db: State<DbState>, workerId: String) -> AppResult<registry::WorkerProfile> {
    with_conn(&db, |conn| registry::refresh_reliability(conn, &workerId))
}

#[tauri::command]
pub fn match_workers(db: State<DbState>, requiredCapabilities: Vec<String>) -> AppResult<Vec<matcher::WorkerMatch>> {
    with_conn(&db, |conn| Ok(matcher::rank(&registry::list(conn)?, &requiredCapabilities)))
}

// ---- Sprint 8 commands (additive) ----

#[tauri::command]
pub fn retry_failed_task(db: State<DbState>, taskId: String, maxRetries: Option<usize>) -> AppResult<reliability::RetryDecision> {
    let policy = reliability::RetryPolicy {
        max_retries: maxRetries.unwrap_or_else(|| reliability::RetryPolicy::default().max_retries),
        ..Default::default()
    };
    with_conn(&db, |conn| reliability::request_retry(conn, &taskId, &policy))
}

#[tauri::command]
pub fn get_task_confidence(db: State<DbState>, taskId: String) -> AppResult<reliability::ConfidenceReport> {
    with_conn(&db, |conn| reliability::confidence_for_task(conn, &taskId))
}

#[tauri::command]
pub fn get_task_report(db: State<DbState>, taskId: String) -> AppResult<reliability::TaskReport> {
    with_conn(&db, |conn| reliability::task_report(conn, &taskId))
}

/// Sprint 5 integration tests: reliability derived from REAL governance
/// rows - actual evidence records judged by the actual Sprint 4
/// PromotionController - not hand-set counters.
#[cfg(test)]
mod tests {
    use super::registry::{self, WorkerProfile};
    use crate::agent::{self, status};
    use crate::governance::{evidence, evidence::kind, promotion};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_conn() -> (std::path::PathBuf, rusqlite::Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_intelligence_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    fn worker(id: &str) -> WorkerProfile {
        WorkerProfile {
            id: id.to_string(),
            name: format!("{id} worker"),
            capabilities: vec!["coding".to_string()],
            reliability_score: 1.0,
            tasks_completed: 0,
            tasks_failed: 0,
        }
    }

    /// Runs one task through the real Sprint 2/4 pipeline: task row ->
    /// evidence row -> PromotionController verdict, assigned to a worker.
    fn run_task_for(conn: &rusqlite::Connection, worker_id: &str, task_id: &str, passed: bool) {
        agent::insert_task(
            conn,
            task_id,
            "objective",
            agent::task_type::EDIT_FILE,
            "f.rs",
            if passed { status::COMPLETED } else { status::ROLLED_BACK },
            "old",
            "new",
            "low",
            None,
            None,
        )
        .unwrap();
        registry::assign_task(conn, task_id, worker_id).unwrap();
        evidence::record(conn, task_id, None, kind::VERIFICATION, if passed { "cargo check passed" } else { "cargo check failed" }, passed).unwrap();
        promotion::request_promotion(conn, task_id, None).unwrap();
    }

    /// Sprint 5 test 2: reliability updates from real completions. Three
    /// passes and one failure through the genuine evidence + promotion
    /// pipeline must derive to exactly 0.75.
    #[test]
    fn reliability_derives_from_real_evidence_and_promotion_rows() {
        let (dir, conn) = temp_conn();
        registry::upsert(&conn, &worker("coder-1")).unwrap();

        // Fresh worker: no judged work, keeps the optimistic default.
        let fresh = registry::refresh_reliability(&conn, "coder-1").unwrap();
        assert_eq!(fresh.reliability_score, 1.0);
        assert_eq!(fresh.tasks_completed, 0);

        run_task_for(&conn, "coder-1", "t1", true);
        run_task_for(&conn, "coder-1", "t2", true);
        run_task_for(&conn, "coder-1", "t3", true);
        run_task_for(&conn, "coder-1", "t4", false);

        let refreshed = registry::refresh_reliability(&conn, "coder-1").unwrap();
        assert_eq!(refreshed.tasks_completed, 3);
        assert_eq!(refreshed.tasks_failed, 1);
        assert!((refreshed.reliability_score - 0.75).abs() < 1e-9, "got {}", refreshed.reliability_score);

        // And it round-trips through the stored profile.
        assert!((registry::get(&conn, "coder-1").unwrap().reliability_score - 0.75).abs() < 1e-9);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A retry that eventually passes counts once, as a pass: only the
    /// LATEST promotion verdict per task is counted.
    #[test]
    fn retried_task_counts_its_latest_verdict_only() {
        let (dir, conn) = temp_conn();
        registry::upsert(&conn, &worker("coder-2")).unwrap();

        agent::insert_task(&conn, "retry-t", "objective", agent::task_type::EDIT_FILE, "f.rs", status::COMPLETED, "old", "new", "low", None, None).unwrap();
        registry::assign_task(&conn, "retry-t", "coder-2").unwrap();

        // First attempt fails and is judged; second attempt passes and is judged.
        evidence::record(&conn, "retry-t", None, kind::VERIFICATION, "cargo check failed", false).unwrap();
        promotion::request_promotion(&conn, "retry-t", None).unwrap();
        evidence::record(&conn, "retry-t", None, kind::VERIFICATION, "cargo check passed", true).unwrap();
        // Ensure the later request sorts strictly after the first even at
        // 1-second timestamp resolution.
        conn.execute("UPDATE promotion_requests SET requested_at = requested_at - 10", []).unwrap();
        promotion::request_promotion(&conn, "retry-t", None).unwrap();

        let refreshed = registry::refresh_reliability(&conn, "coder-2").unwrap();
        assert_eq!(refreshed.tasks_completed, 1);
        assert_eq!(refreshed.tasks_failed, 0);
        assert_eq!(refreshed.reliability_score, 1.0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Workers only own their own history: another worker's failures must
    /// not leak into this worker's score.
    #[test]
    fn reliability_is_scoped_to_the_assigned_worker() {
        let (dir, conn) = temp_conn();
        registry::upsert(&conn, &worker("good")).unwrap();
        registry::upsert(&conn, &worker("bad")).unwrap();

        run_task_for(&conn, "good", "g1", true);
        run_task_for(&conn, "bad", "b1", false);
        run_task_for(&conn, "bad", "b2", false);

        assert_eq!(registry::refresh_reliability(&conn, "good").unwrap().reliability_score, 1.0);
        let bad = registry::refresh_reliability(&conn, "bad").unwrap();
        assert_eq!(bad.reliability_score, 0.0);
        assert_eq!(bad.tasks_failed, 2);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7 failed-worker recovery: a task whose assigned worker fails
    /// ends in a terminal, fully-readable state with an informative error
    /// - not stuck mid-flight - and the work is recoverable: a replacement
    /// task for the same file can be created and assigned immediately,
    /// while the failure stays on the worker's record.
    #[test]
    fn failed_worker_leaves_task_recoverable_not_stuck() {
        let (dir, conn) = temp_conn();
        registry::upsert(&conn, &worker("crashy")).unwrap();

        agent::insert_task(&conn, "doomed-task", "objective", agent::task_type::EDIT_FILE, "f.rs", status::APPLYING, "old", "new", "low", None, None).unwrap();
        registry::assign_task(&conn, "doomed-task", "crashy").unwrap();

        // The worker "fails": the task is finalized as rolled back with the
        // real failure text (this is what approve_task records on rollback).
        agent::update_status(&conn, "doomed-task", status::ROLLED_BACK, Some("cargo check failed:\nerror[E0308]: mismatched types"), Some("verification failed - original content restored")).unwrap();
        evidence::record(&conn, "doomed-task", None, kind::ROLLBACK, "original content restored after failed verification", false).unwrap();
        promotion::request_promotion(&conn, "doomed-task", None).unwrap();

        // Terminal + informative, not stuck in APPLYING.
        let task = agent::get_task(&conn, "doomed-task").unwrap();
        assert_eq!(task.status, status::ROLLED_BACK);
        assert!(task.error.as_deref().unwrap_or_default().contains("verification failed"), "error text must explain the failure: {:?}", task.error);
        assert!(!agent::dag_runnable_tasks(&conn, "no-dag").unwrap().iter().any(|t| t.id == "doomed-task"));

        // Recoverable: a fresh attempt at the same work is immediately valid.
        agent::insert_task(&conn, "retry-attempt", "objective", agent::task_type::EDIT_FILE, "f.rs", status::AWAITING_APPROVAL, "old", "new", "low", None, None).unwrap();
        registry::assign_task(&conn, "retry-attempt", "crashy").unwrap();
        assert_eq!(agent::get_task(&conn, "retry-attempt").unwrap().status, status::AWAITING_APPROVAL);

        // And the worker's record honestly reflects the failure.
        let refreshed = registry::refresh_reliability(&conn, "crashy").unwrap();
        assert_eq!(refreshed.tasks_failed, 1);
        assert_eq!(refreshed.reliability_score, 0.0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end routing over real rows: profiles in the DB, reliability
    /// derived from real verdicts, then a capability-driven match.
    #[test]
    fn matching_over_db_profiles_uses_derived_reliability() {
        let (dir, conn) = temp_conn();
        let mut tester = worker("tester");
        tester.capabilities = vec!["testing".to_string()];
        let mut flaky_tester = worker("flaky-tester");
        flaky_tester.capabilities = vec!["testing".to_string()];
        registry::upsert(&conn, &tester).unwrap();
        registry::upsert(&conn, &flaky_tester).unwrap();

        run_task_for(&conn, "tester", "ok1", true);
        run_task_for(&conn, "flaky-tester", "no1", false);
        registry::refresh_reliability(&conn, "tester").unwrap();
        registry::refresh_reliability(&conn, "flaky-tester").unwrap();

        let profiles = registry::list(&conn).unwrap();
        let best = super::matcher::best_match(&profiles, &["testing".to_string()]).unwrap();
        assert_eq!(best.profile.id, "tester", "derived reliability must drive the tie-break");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
