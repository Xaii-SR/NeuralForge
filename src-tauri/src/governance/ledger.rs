use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Append-only, hash-chained governance event log. Scope honesty (same
/// discipline as the extension-sandbox non-claim in ARCHITECTURE.md):
/// this is tamper-EVIDENCE for a single-user local desktop app - it
/// detects casual edits and corruption via verify_chain(), it is not a
/// distributed ledger and makes no Byzantine-resistance claims. Someone
/// with the SQLite file and this source could rewrite the whole chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Every governance-relevant event type, in one place instead of
/// scattered string literals. Display produces the exact snake_case
/// strings stored in ledger_entries.event_type.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LedgerEvent {
    RequirementCreated,
    RequirementUpdated,
    RequirementRejected,
    RequirementUpdateRejected,
    RequirementRetired,
    RequirementReactivated,
    TaskCreated,
    TaskPlanned,
    TaskPlanFailed,
    TaskApproved,
    TaskRejected,
    TaskCompleted,
    TaskRolledBack,
    TaskFailed,
    TaskRetried,
    PromotionRequested,
    PromotionApproved,
    PromotionBlocked,
}

impl LedgerEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            LedgerEvent::RequirementCreated => "requirement_created",
            LedgerEvent::RequirementUpdated => "requirement_updated",
            LedgerEvent::RequirementRejected => "requirement_rejected",
            LedgerEvent::RequirementUpdateRejected => "requirement_update_rejected",
            LedgerEvent::RequirementRetired => "requirement_retired",
            LedgerEvent::RequirementReactivated => "requirement_reactivated",
            LedgerEvent::TaskCreated => "task_created",
            LedgerEvent::TaskPlanned => "task_planned",
            LedgerEvent::TaskPlanFailed => "task_plan_failed",
            LedgerEvent::TaskApproved => "task_approved",
            LedgerEvent::TaskRejected => "task_rejected",
            LedgerEvent::TaskCompleted => "task_completed",
            LedgerEvent::TaskRolledBack => "task_rolled_back",
            LedgerEvent::TaskFailed => "task_failed",
            LedgerEvent::TaskRetried => "task_retried",
            LedgerEvent::PromotionRequested => "promotion_requested",
            LedgerEvent::PromotionApproved => "promotion_approved",
            LedgerEvent::PromotionBlocked => "promotion_blocked",
        }
    }
}

impl std::fmt::Display for LedgerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Serialize, Type, Clone)]
pub struct LedgerEntry {
    pub seq: i64,
    pub event_type: String,
    pub correlation_id: Option<String>,
    pub requirement_id: Option<String>,
    pub task_id: Option<String>,
    pub payload: String,
    pub created_at: i64,
    pub prev_hash: String,
    pub entry_hash: String,
}

#[derive(Serialize, Type, Clone)]
pub struct ChainVerification {
    pub valid: bool,
    pub entries: i64,
    /// Human-readable description of the first problem found, None when valid.
    pub problem: Option<String>,
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// The canonical byte string that gets hashed - explicit concatenation
/// with newline separators (NULLs as empty strings), not serde output,
/// so the hashed bytes can never drift with a serializer version.
fn canonical(
    prev_hash: &str,
    seq: i64,
    event_type: &str,
    correlation_id: Option<&str>,
    requirement_id: Option<&str>,
    task_id: Option<&str>,
    payload: &str,
    created_at: i64,
) -> String {
    format!(
        "{prev_hash}\n{seq}\n{event_type}\n{}\n{}\n{}\n{payload}\n{created_at}",
        correlation_id.unwrap_or(""),
        requirement_id.unwrap_or(""),
        task_id.unwrap_or("")
    )
}

fn hash_of(canonical: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Appends one event, chained to the previous entry's hash. The single
/// app-wide Mutex<Connection> already serializes every caller, so the
/// read-last-then-insert pair can't interleave. seq is written
/// explicitly (last+1) rather than left to AUTOINCREMENT because the
/// hash must cover the seq the row actually gets.
pub fn append(
    conn: &Connection,
    event: LedgerEvent,
    correlation_id: Option<&str>,
    requirement_id: Option<&str>,
    task_id: Option<&str>,
    payload: serde_json::Value,
) -> AppResult<LedgerEntry> {
    let (last_seq, prev_hash): (i64, String) = conn
        .query_row("SELECT seq, entry_hash FROM ledger_entries ORDER BY seq DESC LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap_or((0, GENESIS_HASH.to_string()));

    let seq = last_seq + 1;
    let payload_str = payload.to_string();
    let created_at = now_secs();
    let event_type = event.as_str();
    let entry_hash = hash_of(&canonical(&prev_hash, seq, event_type, correlation_id, requirement_id, task_id, &payload_str, created_at));

    conn.execute(
        "INSERT INTO ledger_entries (seq, event_type, correlation_id, requirement_id, task_id, payload, created_at, prev_hash, entry_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![seq, event_type, correlation_id, requirement_id, task_id, payload_str, created_at, prev_hash, entry_hash],
    )
    .map_err(|e| AppError::Provider(format!("failed to append ledger entry: {e}")))?;

    Ok(LedgerEntry {
        seq,
        event_type: event_type.to_string(),
        correlation_id: correlation_id.map(String::from),
        requirement_id: requirement_id.map(String::from),
        task_id: task_id.map(String::from),
        payload: payload_str,
        created_at,
        prev_hash,
        entry_hash,
    })
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<LedgerEntry> {
    Ok(LedgerEntry {
        seq: row.get("seq")?,
        event_type: row.get("event_type")?,
        correlation_id: row.get("correlation_id")?,
        requirement_id: row.get("requirement_id")?,
        task_id: row.get("task_id")?,
        payload: row.get("payload")?,
        created_at: row.get("created_at")?,
        prev_hash: row.get("prev_hash")?,
        entry_hash: row.get("entry_hash")?,
    })
}

pub fn list(conn: &Connection, limit: usize) -> AppResult<Vec<LedgerEntry>> {
    let mut stmt = conn
        .prepare("SELECT * FROM ledger_entries ORDER BY seq DESC LIMIT ?1")
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?;
    let rows = stmt
        .query_map(params![limit], row_to_entry)
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read ledger row: {e}")))
}

pub fn list_by_correlation(conn: &Connection, correlation_id: &str) -> AppResult<Vec<LedgerEntry>> {
    let mut stmt = conn
        .prepare("SELECT * FROM ledger_entries WHERE correlation_id = ?1 ORDER BY seq ASC")
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?;
    let rows = stmt
        .query_map(params![correlation_id], row_to_entry)
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read ledger row: {e}")))
}

/// Walks the whole chain in seq order, recomputing every hash and
/// checking linkage plus seq contiguity. Reports the first problem
/// found rather than a bare boolean, so a broken chain is diagnosable.
pub fn verify_chain(conn: &Connection) -> AppResult<ChainVerification> {
    let mut stmt = conn
        .prepare("SELECT * FROM ledger_entries ORDER BY seq ASC")
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?;
    let entries: Vec<LedgerEntry> = stmt
        .query_map([], row_to_entry)
        .map_err(|e| AppError::Provider(format!("failed to query ledger: {e}")))?
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read ledger row: {e}")))?;

    let mut expected_prev = GENESIS_HASH.to_string();
    let mut expected_seq = 1i64;

    for entry in &entries {
        if entry.seq != expected_seq {
            return Ok(ChainVerification {
                valid: false,
                entries: entries.len() as i64,
                problem: Some(format!("seq gap: expected {expected_seq}, found {}", entry.seq)),
            });
        }
        if entry.prev_hash != expected_prev {
            return Ok(ChainVerification {
                valid: false,
                entries: entries.len() as i64,
                problem: Some(format!("broken link at seq {}: prev_hash does not match previous entry", entry.seq)),
            });
        }
        let recomputed = hash_of(&canonical(
            &entry.prev_hash,
            entry.seq,
            &entry.event_type,
            entry.correlation_id.as_deref(),
            entry.requirement_id.as_deref(),
            entry.task_id.as_deref(),
            &entry.payload,
            entry.created_at,
        ));
        if recomputed != entry.entry_hash {
            return Ok(ChainVerification {
                valid: false,
                entries: entries.len() as i64,
                problem: Some(format!("hash mismatch at seq {}: entry content does not match its recorded hash", entry.seq)),
            });
        }
        expected_prev = entry.entry_hash.clone();
        expected_seq += 1;
    }

    Ok(ChainVerification { valid: true, entries: entries.len() as i64, problem: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_ledger_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    #[test]
    fn genesis_entry_chains_from_the_zero_hash() {
        let (dir, conn) = temp_conn();
        let entry = append(&conn, LedgerEvent::RequirementCreated, Some("corr-1"), Some("req-1"), None, serde_json::json!({"v": 1})).unwrap();
        assert_eq!(entry.seq, 1);
        assert_eq!(entry.prev_hash, GENESIS_HASH);
        assert_eq!(entry.entry_hash.len(), 64);
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn second_entry_links_to_the_first_and_chain_verifies() {
        let (dir, conn) = temp_conn();
        let first = append(&conn, LedgerEvent::RequirementCreated, Some("corr-1"), Some("req-1"), None, serde_json::json!({})).unwrap();
        let second = append(&conn, LedgerEvent::TaskCreated, Some("corr-1"), Some("req-1"), Some("task-1"), serde_json::json!({})).unwrap();

        assert_eq!(second.seq, 2);
        assert_eq!(second.prev_hash, first.entry_hash);

        let verification = verify_chain(&conn).unwrap();
        assert!(verification.valid, "clean chain must verify: {:?}", verification.problem);
        assert_eq!(verification.entries, 2);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The tamper-detection proof: mutates a committed row via raw SQL
    /// (exactly what a casual edit of the DB file would do) and asserts
    /// verify_chain catches it at the right seq - not a mocked assertion.
    #[test]
    fn tampering_with_a_payload_via_raw_sql_is_detected_at_the_right_seq() {
        let (dir, conn) = temp_conn();
        append(&conn, LedgerEvent::RequirementCreated, Some("corr-1"), None, None, serde_json::json!({"a": 1})).unwrap();
        append(&conn, LedgerEvent::TaskCreated, Some("corr-1"), None, Some("task-1"), serde_json::json!({"b": 2})).unwrap();
        append(&conn, LedgerEvent::TaskApproved, Some("corr-1"), None, Some("task-1"), serde_json::json!({"c": 3})).unwrap();

        conn.execute("UPDATE ledger_entries SET payload = '{\"b\": 999}' WHERE seq = 2", []).unwrap();

        let verification = verify_chain(&conn).unwrap();
        assert!(!verification.valid);
        assert!(verification.problem.as_deref().unwrap().contains("hash mismatch at seq 2"), "got: {:?}", verification.problem);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_a_middle_row_is_detected_as_a_seq_gap() {
        let (dir, conn) = temp_conn();
        append(&conn, LedgerEvent::RequirementCreated, None, None, None, serde_json::json!({})).unwrap();
        append(&conn, LedgerEvent::TaskCreated, None, None, None, serde_json::json!({})).unwrap();
        append(&conn, LedgerEvent::TaskApproved, None, None, None, serde_json::json!({})).unwrap();

        conn.execute("DELETE FROM ledger_entries WHERE seq = 2", []).unwrap();

        let verification = verify_chain(&conn).unwrap();
        assert!(!verification.valid);
        assert!(verification.problem.as_deref().unwrap().contains("seq gap"), "got: {:?}", verification.problem);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_by_correlation_returns_only_that_chain_in_order() {
        let (dir, conn) = temp_conn();
        append(&conn, LedgerEvent::RequirementCreated, Some("corr-a"), None, None, serde_json::json!({})).unwrap();
        append(&conn, LedgerEvent::RequirementCreated, Some("corr-b"), None, None, serde_json::json!({})).unwrap();
        append(&conn, LedgerEvent::TaskCreated, Some("corr-a"), None, Some("t1"), serde_json::json!({})).unwrap();

        let chain = list_by_correlation(&conn, "corr-a").unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].event_type, "requirement_created");
        assert_eq!(chain[1].event_type, "task_created");
        assert!(chain[0].seq < chain[1].seq);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7: an empty ledger is honestly reported - valid (nothing to
    /// dispute) with an explicit zero entry count, not a panic and not a
    /// fake problem.
    #[test]
    fn empty_ledger_verifies_as_valid_with_zero_entries() {
        let (dir, conn) = temp_conn();
        let v = verify_chain(&conn).unwrap();
        assert!(v.valid);
        assert_eq!(v.entries, 0);
        assert!(v.problem.is_none());
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7: a chain whose FIRST rows were deleted (gap at the start,
    /// not the middle) is detected as a seq gap at entry 1.
    #[test]
    fn missing_rows_at_the_start_of_the_chain_are_detected() {
        let (dir, conn) = temp_conn();
        append(&conn, LedgerEvent::RequirementCreated, None, None, None, serde_json::json!({})).unwrap();
        append(&conn, LedgerEvent::TaskCreated, None, None, None, serde_json::json!({})).unwrap();
        conn.execute("DELETE FROM ledger_entries WHERE seq = 1", []).unwrap();

        let v = verify_chain(&conn).unwrap();
        assert!(!v.valid);
        assert!(v.problem.as_deref().unwrap().contains("seq gap: expected 1"), "got: {:?}", v.problem);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7 volume test: 1000 appends, full verification, then
    /// mid-chain tampering detected at exactly the right seq. Also a
    /// coarse performance guard - generous bound, exists to catch
    /// accidental O(n^2) blowups, not to time-attack CI.
    #[test]
    fn thousand_entry_chain_appends_verifies_and_detects_mid_tampering() {
        let (dir, conn) = temp_conn();
        let started = std::time::Instant::now();
        for i in 0..1000 {
            append(
                &conn,
                if i % 2 == 0 { LedgerEvent::TaskCreated } else { LedgerEvent::TaskCompleted },
                Some(&format!("corr-{}", i % 10)),
                None,
                Some(&format!("task-{i}")),
                serde_json::json!({"i": i, "payload": "x".repeat(200)}),
            )
            .unwrap();
        }
        let v = verify_chain(&conn).unwrap();
        assert!(v.valid, "{:?}", v.problem);
        assert_eq!(v.entries, 1000);

        // A correlation query over the volume stays correct.
        assert_eq!(list_by_correlation(&conn, "corr-3").unwrap().len(), 100);

        conn.execute("UPDATE ledger_entries SET payload = '{}' WHERE seq = 500", []).unwrap();
        let tampered = verify_chain(&conn).unwrap();
        assert!(!tampered.valid);
        assert!(tampered.problem.as_deref().unwrap().contains("hash mismatch at seq 500"), "got: {:?}", tampered.problem);

        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() < 30, "1000 appends + 2 verifies took {elapsed:?} - something is pathologically slow");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7 graceful degradation: a tampered ledger makes verify_chain
    /// report invalid, but evidence reads and promotion judgments keep
    /// working - the audit trail being disputed must not crash the app.
    #[test]
    fn tampered_ledger_does_not_break_evidence_reads_or_promotion_checks() {
        let (dir, conn) = temp_conn();
        append(&conn, LedgerEvent::TaskCreated, Some("corr-x"), None, Some("t1"), serde_json::json!({})).unwrap();
        crate::governance::evidence::record(&conn, "t1", Some("corr-x"), crate::governance::evidence::kind::VERIFICATION, "cargo check passed", true).unwrap();

        conn.execute("UPDATE ledger_entries SET payload = 'tampered' WHERE seq = 1", []).unwrap();
        assert!(!verify_chain(&conn).unwrap().valid);

        // Evidence still reads; promotion still judges (and even appends
        // new ledger entries - the chain stays broken at seq 1, which
        // verify_chain keeps reporting, but nothing panics).
        let ev = crate::governance::evidence::for_task(&conn, "t1").unwrap();
        assert_eq!(ev.len(), 1);
        let verdict = crate::governance::promotion::request_promotion(&conn, "t1", Some("corr-x")).unwrap();
        assert_eq!(verdict.status, crate::governance::promotion::status::PROMOTED);
        assert!(!verify_chain(&conn).unwrap().valid, "tampering is still reported after further appends");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn event_enum_serializes_to_the_exact_snake_case_strings() {
        assert_eq!(LedgerEvent::RequirementCreated.to_string(), "requirement_created");
        assert_eq!(LedgerEvent::RequirementRejected.to_string(), "requirement_rejected");
        assert_eq!(LedgerEvent::TaskRolledBack.to_string(), "task_rolled_back");
        assert_eq!(LedgerEvent::TaskPlanFailed.to_string(), "task_plan_failed");
    }
}
