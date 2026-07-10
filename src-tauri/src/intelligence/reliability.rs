use crate::agent::{self, status, task_type, AgentTask};
use crate::core::errors::{AppError, AppResult};
use crate::governance::evidence::{self, kind, EvidenceRecord};
use crate::governance::ledger::{self, LedgerEvent};
use crate::governance::promotion::{self, PromotionRequest};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Sprint 8: the Autonomous Reliability Layer. Everything here is DERIVED
/// from rows the Sprint 1-7 systems already write - classification reads
/// recorded outcomes, confidence and completeness read evidence/promotion
/// rows, retries create ordinary task rows through the ordinary human
/// approval gate. No frozen system's writes or judgments are changed.

// ---------------------------------------------------------------------
// Failure classification
// ---------------------------------------------------------------------

/// Closed set of failure causes, derived on read from the status, error,
/// and verification text the pipeline already records.
#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// cargo check rejected the change (compiler diagnostics present).
    CompileError,
    /// a test run failed (test-runner output present).
    TestFailure,
    /// a run_code execution failed in the extension sandbox.
    ExecutionError,
    /// never attempted - a dependency failed.
    BlockedDependency,
    /// a human rejected it.
    UserRejected,
    /// failed, but the recorded text doesn't match a known cause.
    Unknown,
    /// not a failure at all.
    NotFailed,
}

impl FailureClass {
    /// Retry only helps when a fresh attempt could plausibly differ:
    /// compile/test/execution failures. A human's rejection is not
    /// overridden by a robot, and a blocked task recovers via its
    /// DEPENDENCY's retry, not its own.
    pub fn is_retryable(&self) -> bool {
        matches!(self, FailureClass::CompileError | FailureClass::TestFailure | FailureClass::ExecutionError | FailureClass::Unknown)
    }
}

pub fn classify_failure(task: &AgentTask) -> FailureClass {
    match task.status.as_str() {
        status::REJECTED => FailureClass::UserRejected,
        status::BLOCKED => FailureClass::BlockedDependency,
        status::FAILED | status::ROLLED_BACK => {
            let text = format!(
                "{}\n{}",
                task.verification.as_deref().unwrap_or_default(),
                task.error.as_deref().unwrap_or_default()
            );
            if text.contains("cargo check failed") || text.contains("error[E") {
                FailureClass::CompileError
            } else if text.contains("test result: FAILED") || text.contains("test failed") || text.contains("FAILED. ") {
                FailureClass::TestFailure
            } else if task.task_type == task_type::RUN_CODE {
                FailureClass::ExecutionError
            } else {
                FailureClass::Unknown
            }
        }
        _ => FailureClass::NotFailed,
    }
}

// ---------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct RetryPolicy {
    /// Maximum retries AFTER the original attempt (2 = up to 3 attempts).
    pub max_retries: usize,
    /// Optional floor: refuse auto-retry for workers below this derived
    /// reliability (consumed read-only from the Sprint 5 score). None
    /// disables the gate - the default while only the Coder exists.
    pub min_worker_reliability: Option<f64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy { max_retries: 2, min_worker_reliability: None }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct RetryDecision {
    pub allowed: bool,
    pub reason: String,
    /// The failure class the decision was based on.
    pub failure_class: FailureClass,
    /// Attempt number of the ORIGINAL+retries chain so far (1 = only the
    /// original attempt exists).
    pub attempts_so_far: usize,
    /// The new task awaiting human approval, when allowed.
    pub retry_task_id: Option<String>,
}

/// Walks retry_of links back to the lineage root, cycle-guarded.
fn lineage_root(conn: &Connection, task_id: &str) -> AppResult<String> {
    let mut current = task_id.to_string();
    let mut seen = std::collections::HashSet::new();
    while seen.insert(current.clone()) {
        match agent::get_task(conn, &current)?.retry_of {
            Some(parent) => current = parent,
            None => return Ok(current),
        }
    }
    Err(AppError::Provider(format!("retry lineage of {task_id} contains a cycle - refusing to retry")))
}

/// All attempts in the lineage (root + every transitive retry), oldest
/// first by rowid.
pub fn lineage_attempts(conn: &Connection, task_id: &str) -> AppResult<Vec<AgentTask>> {
    let root = lineage_root(conn, task_id)?;
    let all = agent::list_tasks(conn)?;
    let mut attempts = Vec::new();
    let mut frontier = vec![root];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = frontier.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(t) = all.iter().find(|t| t.id == id) {
            attempts.push(t.clone());
        }
        for t in all.iter().filter(|t| t.retry_of.as_deref() == Some(id.as_str())) {
            frontier.push(t.id.clone());
        }
    }
    Ok(attempts)
}

/// The bounded, human-gated retry decision. When allowed, a new task row
/// is created (status AWAITING_APPROVAL - it still goes through the same
/// approve_task gate as every task; nothing executes unattended) and a
/// task_retried ledger event lands on the correlation chain. The whole
/// write is one transaction. Refusals are returned with reasons, never
/// silently dropped.
pub fn request_retry(conn: &Connection, task_id: &str, policy: &RetryPolicy) -> AppResult<RetryDecision> {
    let task = agent::get_task(conn, task_id)?;
    let failure_class = classify_failure(&task);
    let attempts = lineage_attempts(conn, task_id)?;
    let attempts_so_far = attempts.len();

    let refuse = |reason: String| RetryDecision {
        allowed: false,
        reason,
        failure_class,
        attempts_so_far,
        retry_task_id: None,
    };

    if failure_class == FailureClass::NotFailed {
        return Ok(refuse(format!("task {task_id} has status '{}' - there is nothing to retry", task.status)));
    }
    if !failure_class.is_retryable() {
        return Ok(refuse(format!("failure class {failure_class:?} is not retryable: {}", match failure_class {
            FailureClass::UserRejected => "a human rejected this work; a retry would override that decision",
            FailureClass::BlockedDependency => "this task never ran - retry its failed dependency instead",
            _ => "not eligible",
        })));
    }
    if attempts.iter().any(|t| matches!(t.status.as_str(), status::PLANNING | status::AWAITING_APPROVAL | status::APPLYING)) {
        return Ok(refuse("an attempt in this lineage is already pending - not stacking another".to_string()));
    }
    // attempts_so_far includes the original; retries used = attempts - 1.
    if attempts_so_far.saturating_sub(1) >= policy.max_retries {
        return Ok(refuse(format!(
            "retry budget exhausted: {} attempts made, policy allows {} retries after the original",
            attempts_so_far, policy.max_retries
        )));
    }
    if let Some(floor) = policy.min_worker_reliability {
        let worker_id: Option<String> = conn
            .query_row("SELECT worker_id FROM agent_tasks WHERE id = ?1", rusqlite::params![task_id], |r| r.get(0))
            .unwrap_or(None);
        if let Some(wid) = worker_id {
            let profile = super::registry::get(conn, &wid)?;
            if profile.reliability_score < floor {
                return Ok(refuse(format!(
                    "worker '{}' reliability {:.2} is below the policy floor {:.2} - flag for human review instead of auto-retry",
                    wid, profile.reliability_score, floor
                )));
            }
        }
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    crate::database::in_transaction(conn, |conn| {
        agent::insert_retry_task(conn, &task, &new_id)?;
        let _ = ledger::append(
            conn,
            LedgerEvent::TaskRetried,
            task.correlation_id.as_deref(),
            task.requirement_id.as_deref(),
            Some(&new_id),
            serde_json::json!({
                "retry_of": task_id,
                "attempt": attempts_so_far + 1,
                "failure_class": failure_class,
                "max_retries": policy.max_retries,
            }),
        );
        Ok(())
    })?;

    tracing::info!(target: "intelligence", event = "task_retry_prepared", failed_task = %task_id, retry_task = %new_id, attempt = attempts_so_far + 1);
    Ok(RetryDecision {
        allowed: true,
        reason: format!("attempt {} of {} prepared - awaiting human approval", attempts_so_far + 1, policy.max_retries + 1),
        failure_class,
        attempts_so_far,
        retry_task_id: Some(new_id),
    })
}

// ---------------------------------------------------------------------
// Evidence completeness
// ---------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct CompletenessReport {
    pub complete: bool,
    pub missing: Vec<String>,
}

/// Does the recorded story of this task hold together? Checks are against
/// what the pipeline PROMISES to write for each terminal status; a
/// discrepancy means rows were lost or externally deleted. Read-only -
/// the promotion gate's own judgment is untouched.
pub fn evidence_completeness(conn: &Connection, task_id: &str) -> AppResult<CompletenessReport> {
    let task = agent::get_task(conn, task_id)?;
    let evidence_rows = evidence::for_task(conn, task_id)?;
    let promotions = promotion::for_task(conn, task_id)?;
    let mut missing = Vec::new();

    let has_kind = |k: &str| evidence_rows.iter().any(|e| e.kind == k);
    match task.status.as_str() {
        status::COMPLETED | status::FAILED | status::ROLLED_BACK => {
            let expected_kind = if task.task_type == task_type::RUN_CODE { kind::EXECUTION_OUTPUT } else { kind::VERIFICATION };
            if !has_kind(expected_kind) {
                missing.push(format!("no {expected_kind} evidence for a terminal '{}' task", task.status));
            }
            if task.status == status::ROLLED_BACK && !has_kind(kind::ROLLBACK) {
                missing.push("rolled-back task has no rollback evidence".to_string());
            }
            if promotions.is_empty() {
                missing.push("terminal task has no promotion verdict".to_string());
            }
        }
        // blocked/rejected/pending tasks legitimately have no evidence.
        _ => {}
    }
    if task.requirement_id.is_some() && task.correlation_id.is_none() {
        missing.push("requirement-gated task is missing its correlation_id".to_string());
    }

    Ok(CompletenessReport { complete: missing.is_empty(), missing })
}

// ---------------------------------------------------------------------
// Confidence
// ---------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct ConfidenceReport {
    /// [0.0, 1.0]; only meaningful for COMPLETED tasks (others score 0).
    pub score: f64,
    pub factors: Vec<String>,
}

/// How much should a human trust this pass? Weighted, documented, and
/// derived entirely from existing rows:
///   0.4  verification strength (real cargo check/test > unverified file type)
///   0.2  evidence completeness
///   0.2  first-attempt bonus (scaled down by retry count)
///   0.2  assigned worker's derived reliability (0.1 neutral if unassigned)
pub fn confidence_for_task(conn: &Connection, task_id: &str) -> AppResult<ConfidenceReport> {
    let task = agent::get_task(conn, task_id)?;
    let mut factors = Vec::new();

    if task.status != status::COMPLETED {
        return Ok(ConfidenceReport { score: 0.0, factors: vec![format!("task status is '{}', not completed - no pass to be confident in", task.status)] });
    }

    let evidence_rows = evidence::for_task(conn, task_id)?;
    let latest_pass: Option<&EvidenceRecord> = evidence_rows.iter().rev().find(|e| e.success);
    let verification_score = match latest_pass {
        Some(ev) if ev.content.contains("cargo check passed") || ev.content.contains("test result: ok") => {
            factors.push("verified by a real build/test run (0.40)".to_string());
            0.40
        }
        Some(ev) if ev.content.contains("no automated") => {
            factors.push("no automated verifier exists for this file type (0.15)".to_string());
            0.15
        }
        Some(_) => {
            factors.push("passing evidence exists but verifier strength unknown (0.25)".to_string());
            0.25
        }
        None => {
            factors.push("no passing evidence at all (0.00)".to_string());
            0.0
        }
    };

    let completeness = evidence_completeness(conn, task_id)?;
    let completeness_score = if completeness.complete {
        factors.push("evidence record is complete (0.20)".to_string());
        0.20
    } else {
        factors.push(format!("evidence record incomplete: {} (0.00)", completeness.missing.join("; ")));
        0.0
    };

    let attempts = lineage_attempts(conn, task_id)?.len().max(1);
    let retry_score = 0.20 / attempts as f64;
    factors.push(if attempts == 1 {
        "passed on the first attempt (0.20)".to_string()
    } else {
        format!("passed after {} attempts ({:.2})", attempts, retry_score)
    });

    let worker_id: Option<String> = conn
        .query_row("SELECT worker_id FROM agent_tasks WHERE id = ?1", rusqlite::params![task_id], |r| r.get(0))
        .unwrap_or(None);
    let worker_score = match worker_id.as_deref().map(|w| super::registry::get(conn, w)) {
        Some(Ok(profile)) => {
            let s = 0.20 * profile.reliability_score;
            factors.push(format!("worker '{}' reliability {:.2} ({:.2})", profile.id, profile.reliability_score, s));
            s
        }
        _ => {
            factors.push("no worker assigned - neutral (0.10)".to_string());
            0.10
        }
    };

    let score = (verification_score + completeness_score + retry_score + worker_score).clamp(0.0, 1.0);
    Ok(ConfidenceReport { score, factors })
}

// ---------------------------------------------------------------------
// Structured task report
// ---------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct TaskReport {
    pub task: AgentTask,
    pub failure_class: FailureClass,
    pub attempts: usize,
    /// Every attempt in the lineage, oldest root first entry.
    pub lineage: Vec<String>,
    pub evidence: Vec<EvidenceRecord>,
    pub promotions: Vec<PromotionRequest>,
    /// Ledger events naming this specific task, in chain order.
    pub ledger_events: Vec<crate::governance::ledger::LedgerEntry>,
    pub confidence: ConfidenceReport,
    pub completeness: CompletenessReport,
}

/// One structured answer to "what happened with this task?" - assembled
/// entirely from existing governance rows, verifiable against the ledger.
pub fn task_report(conn: &Connection, task_id: &str) -> AppResult<TaskReport> {
    let task = agent::get_task(conn, task_id)?;
    let attempts = lineage_attempts(conn, task_id)?;
    let ledger_events = match task.correlation_id.as_deref() {
        Some(corr) => ledger::list_by_correlation(conn, corr)?
            .into_iter()
            .filter(|e| e.task_id.as_deref() == Some(task_id))
            .collect(),
        None => Vec::new(),
    };

    Ok(TaskReport {
        failure_class: classify_failure(&task),
        attempts: attempts.len(),
        lineage: attempts.iter().map(|t| t.id.clone()).collect(),
        evidence: evidence::for_task(conn, task_id)?,
        promotions: promotion::for_task(conn, task_id)?,
        confidence: confidence_for_task(conn, task_id)?,
        completeness: evidence_completeness(conn, task_id)?,
        ledger_events,
        task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_reliability_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    fn insert(conn: &Connection, id: &str, ttype: &str, st: &str, verification: Option<&str>, error: Option<&str>) {
        agent::insert_task(conn, id, "obj", ttype, "f.rs", status::APPLYING, "old", "new", "low", Some("req-1"), Some("corr-rel")).unwrap();
        agent::update_status(conn, id, st, verification, error).unwrap();
    }

    /// Every classification case, from outcomes recorded the way the real
    /// pipeline records them.
    #[test]
    fn classification_covers_every_failure_case() {
        let (dir, conn) = temp_conn();

        insert(&conn, "t-compile", task_type::EDIT_FILE, status::ROLLED_BACK, Some("cargo check failed:\nerror[E0308]: mismatched types"), Some("verification failed"));
        insert(&conn, "t-test", task_type::EDIT_FILE, status::FAILED, Some("running 3 tests\ntest result: FAILED. 2 passed; 1 failed"), None);
        insert(&conn, "t-exec", task_type::RUN_CODE, status::FAILED, Some(""), Some("extension runner exited with a nonzero status"));
        insert(&conn, "t-blocked", task_type::EDIT_FILE, status::BLOCKED, None, Some("a dependency failed - task was never attempted"));
        insert(&conn, "t-rejected", task_type::EDIT_FILE, status::REJECTED, None, None);
        insert(&conn, "t-unknown", task_type::EDIT_FILE, status::FAILED, Some("something odd happened"), None);
        insert(&conn, "t-ok", task_type::EDIT_FILE, status::COMPLETED, Some("cargo check passed"), None);

        let class_of = |id: &str| classify_failure(&agent::get_task(&conn, id).unwrap());
        assert_eq!(class_of("t-compile"), FailureClass::CompileError);
        assert_eq!(class_of("t-test"), FailureClass::TestFailure);
        assert_eq!(class_of("t-exec"), FailureClass::ExecutionError);
        assert_eq!(class_of("t-blocked"), FailureClass::BlockedDependency);
        assert_eq!(class_of("t-rejected"), FailureClass::UserRejected);
        assert_eq!(class_of("t-unknown"), FailureClass::Unknown);
        assert_eq!(class_of("t-ok"), FailureClass::NotFailed);

        assert!(FailureClass::CompileError.is_retryable());
        assert!(!FailureClass::UserRejected.is_retryable());
        assert!(!FailureClass::BlockedDependency.is_retryable());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Retry accepted: new row with lineage, correlation, content, and a
    /// task_retried event; the retry awaits HUMAN approval, never executes
    /// by itself.
    #[test]
    fn retry_accepted_preserves_lineage_correlation_and_human_gate() {
        let (dir, conn) = temp_conn();
        insert(&conn, "fail-1", task_type::EDIT_FILE, status::ROLLED_BACK, Some("cargo check failed:\nerror[E0308]"), Some("verification failed"));

        let decision = request_retry(&conn, "fail-1", &RetryPolicy::default()).unwrap();
        assert!(decision.allowed, "{}", decision.reason);
        assert_eq!(decision.failure_class, FailureClass::CompileError);
        assert_eq!(decision.attempts_so_far, 1);

        let retry_id = decision.retry_task_id.unwrap();
        let retry = agent::get_task(&conn, &retry_id).unwrap();
        assert_eq!(retry.retry_of.as_deref(), Some("fail-1"));
        assert_eq!(retry.status, status::AWAITING_APPROVAL, "retries go through the same human gate");
        assert_eq!(retry.correlation_id.as_deref(), Some("corr-rel"));
        assert_eq!(retry.objective, "obj");
        let (orig, prop) = agent::get_task_content(&conn, &retry_id).unwrap();
        assert_eq!((orig.as_str(), prop.as_str()), ("old", "new"), "work content is cloned");

        let chain = ledger::list_by_correlation(&conn, "corr-rel").unwrap();
        let retried: Vec<_> = chain.iter().filter(|e| e.event_type == "task_retried").collect();
        assert_eq!(retried.len(), 1);
        assert!(retried[0].payload.contains("\"attempt\":2"));
        assert!(retried[0].payload.contains("compile_error"));

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Refusals: non-retryable classes, pending attempt already in flight,
    /// and budget exhaustion - each with an explanatory reason and NO new
    /// row created.
    #[test]
    fn retry_refused_for_rejection_block_pending_and_exhausted_budget() {
        let (dir, conn) = temp_conn();
        let policy = RetryPolicy::default();

        insert(&conn, "rej", task_type::EDIT_FILE, status::REJECTED, None, None);
        let d = request_retry(&conn, "rej", &policy).unwrap();
        assert!(!d.allowed);
        assert!(d.reason.contains("human rejected"), "got: {}", d.reason);

        insert(&conn, "blk", task_type::EDIT_FILE, status::BLOCKED, None, None);
        let d = request_retry(&conn, "blk", &policy).unwrap();
        assert!(!d.allowed);
        assert!(d.reason.contains("dependency"), "got: {}", d.reason);

        insert(&conn, "ok", task_type::EDIT_FILE, status::COMPLETED, Some("cargo check passed"), None);
        assert!(!request_retry(&conn, "ok", &policy).unwrap().allowed);

        // Budget: original fails, retry 1 fails, retry 2 fails -> third
        // retry request refused. While a retry is PENDING, also refused.
        insert(&conn, "budget", task_type::EDIT_FILE, status::ROLLED_BACK, Some("cargo check failed: error[E1]"), None);
        let r1 = request_retry(&conn, "budget", &policy).unwrap();
        assert!(r1.allowed);
        let r1_id = r1.retry_task_id.unwrap();
        let d = request_retry(&conn, "budget", &policy).unwrap();
        assert!(!d.allowed, "must not stack retries while one is pending");
        assert!(d.reason.contains("pending"), "got: {}", d.reason);

        agent::update_status(&conn, &r1_id, status::ROLLED_BACK, Some("cargo check failed: error[E2]"), None).unwrap();
        let r2 = request_retry(&conn, &r1_id, &policy).unwrap();
        assert!(r2.allowed);
        let r2_id = r2.retry_task_id.unwrap();
        agent::update_status(&conn, &r2_id, status::ROLLED_BACK, Some("cargo check failed: error[E3]"), None).unwrap();

        // 3 attempts made, max_retries = 2 -> exhausted (from ANY member
        // of the lineage - counting walks the whole chain).
        for id in ["budget", r2_id.as_str()] {
            let d = request_retry(&conn, id, &policy).unwrap();
            assert!(!d.allowed, "budget must be exhausted for {id}");
            assert!(d.reason.contains("exhausted"), "got: {}", d.reason);
            assert_eq!(d.attempts_so_far, 3);
        }

        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM agent_tasks WHERE retry_of IS NOT NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2, "refusals must not have created rows");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Confidence ordering: verified first-attempt pass > unverified-type
    /// pass and > verified pass-after-retry, and factors name their inputs.
    #[test]
    fn confidence_reflects_verification_completeness_retries_and_worker() {
        let (dir, conn) = temp_conn();

        let finish = |id: &str, verification: &str| {
            let task = agent::get_task(&conn, id).unwrap();
            agent::record_task_outcome_atomic(&conn, &task, id, status::COMPLETED, verification, None, None).unwrap();
        };

        // A: first-attempt, real verification, complete record.
        agent::insert_task(&conn, "conf-a", "obj", task_type::EDIT_FILE, "f.rs", status::APPLYING, "o", "n", "low", None, Some("c-a")).unwrap();
        finish("conf-a", "cargo check passed");

        // B: first-attempt but no automated verifier for the file type.
        agent::insert_task(&conn, "conf-b", "obj", task_type::EDIT_FILE, "f.md", status::APPLYING, "o", "n", "low", None, Some("c-b")).unwrap();
        finish("conf-b", "no automated verification available for this file type - written without a build/test check");

        // C: passes only on the second attempt (real retry lineage).
        insert(&conn, "conf-c0", task_type::EDIT_FILE, status::ROLLED_BACK, Some("cargo check failed: error[E1]"), None);
        let retry = request_retry(&conn, "conf-c0", &RetryPolicy::default()).unwrap().retry_task_id.unwrap();
        finish(&retry, "cargo check passed");

        let a = confidence_for_task(&conn, "conf-a").unwrap();
        let b = confidence_for_task(&conn, "conf-b").unwrap();
        let c = confidence_for_task(&conn, &retry).unwrap();

        assert!(a.score > c.score, "first-attempt ({}) must beat retry pass ({})", a.score, c.score);
        assert!(a.score > b.score, "verified ({}) must beat unverified type ({})", a.score, b.score);
        assert!(a.factors.iter().any(|f| f.contains("real build/test run")));
        assert!(b.factors.iter().any(|f| f.contains("no automated verifier")));
        assert!(c.factors.iter().any(|f| f.contains("2 attempts")));

        // Failed task: zero confidence, honest factor.
        assert_eq!(confidence_for_task(&conn, "conf-c0").unwrap().score, 0.0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Completeness: a properly-finished task is complete; artificially
    /// deleting its evidence (raw SQL, the Sprint 7 pattern) is detected
    /// and named, without breaking any reads.
    #[test]
    fn completeness_detects_artificially_missing_evidence() {
        let (dir, conn) = temp_conn();
        agent::insert_task(&conn, "comp-t", "obj", task_type::EDIT_FILE, "f.rs", status::APPLYING, "o", "n", "low", None, Some("c-comp")).unwrap();
        let task = agent::get_task(&conn, "comp-t").unwrap();
        agent::record_task_outcome_atomic(&conn, &task, "comp-t", status::COMPLETED, "cargo check passed", None, None).unwrap();

        let report = evidence_completeness(&conn, "comp-t").unwrap();
        assert!(report.complete, "properly finished task must be complete: {:?}", report.missing);

        // Sabotage: someone deletes the record out from under the task.
        // (The FK from promotion_requests correctly refuses to orphan the
        // evidence - proof the remediation pragma bites - so a full
        // sabotage must remove the promotion verdict first.)
        assert!(conn.execute("DELETE FROM evidence WHERE task_id = 'comp-t'", []).is_err(), "FK must protect referenced evidence");
        conn.execute("DELETE FROM promotion_requests WHERE task_id = 'comp-t'", []).unwrap();
        conn.execute("DELETE FROM evidence WHERE task_id = 'comp-t'", []).unwrap();
        let report = evidence_completeness(&conn, "comp-t").unwrap();
        assert!(!report.complete);
        assert!(report.missing.iter().any(|m| m.contains("verification evidence")), "got: {:?}", report.missing);
        // And confidence degrades but does not crash.
        let conf = confidence_for_task(&conn, "comp-t").unwrap();
        assert!(conf.score < 0.5);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The structured report matches the real underlying rows end-to-end.
    #[test]
    fn task_report_matches_underlying_ledger_and_evidence_state() {
        let (dir, conn) = temp_conn();
        insert(&conn, "rep-0", task_type::EDIT_FILE, status::ROLLED_BACK, Some("cargo check failed: error[E1]"), Some("verification failed"));
        let retry_id = request_retry(&conn, "rep-0", &RetryPolicy::default()).unwrap().retry_task_id.unwrap();
        let retry_task = agent::get_task(&conn, &retry_id).unwrap();
        agent::record_task_outcome_atomic(&conn, &retry_task, &retry_id, status::COMPLETED, "cargo check passed", None, None).unwrap();

        let report = task_report(&conn, &retry_id).unwrap();
        assert_eq!(report.task.id, retry_id);
        assert_eq!(report.failure_class, FailureClass::NotFailed);
        assert_eq!(report.attempts, 2);
        assert!(report.lineage.contains(&"rep-0".to_string()) && report.lineage.contains(&retry_id));
        assert_eq!(report.evidence.len(), evidence::for_task(&conn, &retry_id).unwrap().len());
        assert_eq!(report.promotions.len(), 1);
        assert_eq!(report.promotions[0].status, promotion::status::PROMOTED);
        assert!(report.ledger_events.iter().any(|e| e.event_type == "task_retried"));
        assert!(report.ledger_events.iter().any(|e| e.event_type == "task_completed"));
        assert!(report.confidence.score > 0.0);
        assert!(report.completeness.complete);

        // The failed original's report tells its own honest story.
        let orig = task_report(&conn, "rep-0").unwrap();
        assert_eq!(orig.failure_class, FailureClass::CompileError);
        assert_eq!(orig.attempts, 2, "lineage is visible from every member");
        assert_eq!(orig.confidence.score, 0.0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
