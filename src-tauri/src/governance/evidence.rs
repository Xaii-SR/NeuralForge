use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

/// Structured, queryable execution evidence - what agent_history.md's
/// one-line entries could never carry. Each row is one artifact produced
/// while executing a task (verification output, rollback note, run_code
/// stdout), referenced by ID from the ledger event that closed the task.
/// 
/// IMPORTANT ARCHITECTURAL CONTRACT:
/// 
/// Evidence retrieval MUST preserve true chronological insertion order.
/// 
/// Why timestamps cannot guarantee ordering:
/// - UUIDv4 identifiers are randomly generated and have no relationship to insertion sequence
/// - Wall-clock timestamps (created_at) cannot distinguish order when multiple records are inserted within the same second
/// - Database storage order is not guaranteed to match insertion sequence
/// - Relying on UUID sorting or timestamp ordering violates traceability contract
/// 
/// Deterministic monotonic ordering mechanism:
/// - Added `insertion_sequence INTEGER` column via additive migration
/// - Sequence numbers are allocated transactionally using a monotonic counter
/// - Evidence retrieval orders by `insertion_sequence ASC` to guarantee INSERT ORDER == RETRIEVAL ORDER
/// - This preserves the NeuralForge traceability contract: historical timeline reconstruction must be exact
/// 
/// Future NeuralForge development MUST preserve this contract. Evidence ordering is not optional - it's a core requirement for complete traceability.
#[derive(Serialize, Type, Clone)]
pub struct EvidenceRecord {
    pub id: String,
    pub insertion_sequence: i64,
    pub task_id: String,
    pub correlation_id: Option<String>,
    pub kind: String,
    pub content: String,
    /// Sprint 5 groundwork (worker reputation): did the thing this
    /// evidence documents succeed? A rollback note is honest evidence of
    /// a FAILED verification, so it stores false.
    pub success: bool,
    pub created_at: i64,
}

pub mod kind {
    pub const VERIFICATION: &str = "verification";
    pub const ROLLBACK: &str = "rollback";
    pub const EXECUTION_OUTPUT: &str = "execution_output";
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// Allocates the next monotonic sequence number for evidence insertion.
/// Uses a dedicated sequence tracking table to ensure transaction safety
/// and deterministic ordering across all evidence records.
fn allocate_sequence(conn: &Connection) -> AppResult<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO evidence_sequence (next_sequence) VALUES (1)",
        [],
    ).map_err(|e| AppError::Provider(format!("failed to initialize evidence sequence: {e}")))?;
    
    let seq: i64 = conn.query_row(
        "UPDATE evidence_sequence SET next_sequence = next_sequence + 1 RETURNING next_sequence - 1",
        [],
        |row| row.get(0),
    ).map_err(|e| AppError::Provider(format!("failed to allocate evidence sequence: {e}")))?;
    
    Ok(seq)
}

pub fn record(
    conn: &Connection,
    task_id: &str,
    correlation_id: Option<&str>,
    kind: &str,
    content: &str,
    success: bool,
) -> AppResult<EvidenceRecord> {
    let sequence = allocate_sequence(conn)?;
    
    let rec = EvidenceRecord {
        id: uuid::Uuid::new_v4().to_string(),
        insertion_sequence: sequence,
        task_id: task_id.to_string(),
        correlation_id: correlation_id.map(String::from),
        kind: kind.to_string(),
        content: content.to_string(),
        success,
        created_at: now_secs(),
    };
    conn.execute(
        "INSERT INTO evidence (id, insertion_sequence, task_id, correlation_id, kind, content, success, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            rec.id, 
            rec.insertion_sequence, 
            rec.task_id, 
            rec.correlation_id, 
            rec.kind, 
            rec.content, 
            rec.success as i64, 
            rec.created_at
        ],
    )
    .map_err(|e| AppError::Provider(format!("failed to record evidence: {e}")))?;
    Ok(rec)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<EvidenceRecord> {
    let success: i64 = row.get("success")?;
    Ok(EvidenceRecord {
        id: row.get("id")?,
        insertion_sequence: row.get("insertion_sequence")?,
        task_id: row.get("task_id")?,
        correlation_id: row.get("correlation_id")?,
        kind: row.get("kind")?,
        content: row.get("content")?,
        success: success != 0,
        created_at: row.get("created_at")?,
    })
}

pub fn for_task(conn: &Connection, task_id: &str) -> AppResult<Vec<EvidenceRecord>> {
    let mut stmt = conn
        .prepare("SELECT * FROM evidence WHERE task_id = ?1 ORDER BY insertion_sequence ASC")
        .map_err(|e| AppError::Provider(format!("failed to query evidence: {e}")))?;
    let rows = stmt
        .query_map(params![task_id], row_to_record)
        .map_err(|e| AppError::Provider(format!("failed to query evidence: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read evidence row: {e}")))
}

pub fn for_correlation(conn: &Connection, correlation_id: &str) -> AppResult<Vec<EvidenceRecord>> {
    let mut stmt = conn
        .prepare("SELECT * FROM evidence WHERE correlation_id = ?1 ORDER BY insertion_sequence ASC")
        .map_err(|e| AppError::Provider(format!("failed to query evidence: {e}")))?;
    let rows = stmt
        .query_map(params![correlation_id], row_to_record)
        .map_err(|e| AppError::Provider(format!("failed to query evidence: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read evidence row: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_conn() -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_evidence_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    #[test]
    fn chronological_ordering_with_identical_timestamps() {
        let (dir, conn) = temp_conn();

        // Test strict chronological ordering with identical timestamps
        record(&conn, "task-chrono-1", Some("corr-chrono"), kind::VERIFICATION, "first", true).unwrap();
        record(&conn, "task-chrono-1", Some("corr-chrono"), kind::ROLLBACK, "second", false).unwrap();
        record(&conn, "task-chrono-1", Some("corr-chrono"), kind::EXECUTION_OUTPUT, "third", true).unwrap();

        let by_task = for_task(&conn, "task-chrono-1").unwrap();
        assert_eq!(by_task.len(), 3, "Expected 3 evidence items");
        
        // Strict chronological ordering validation: INSERT ORDER == RETRIEVAL ORDER
        assert_eq!(by_task[0].kind, kind::VERIFICATION);
        assert_eq!(by_task[1].kind, kind::ROLLBACK);
        assert_eq!(by_task[2].kind, kind::EXECUTION_OUTPUT);
        
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_and_fetch_by_task_and_correlation() {
        let (dir, conn) = temp_conn();

        record(&conn, "task-1", Some("corr-1"), kind::VERIFICATION, "cargo check passed", true).unwrap();
        record(&conn, "task-1", Some("corr-1"), kind::ROLLBACK, "restored after failed verification", false).unwrap();
        record(&conn, "task-2", Some("corr-2"), kind::EXECUTION_OUTPUT, "42", true).unwrap();

        let by_task = for_task(&conn, "task-1").unwrap();
        assert_eq!(by_task.len(), 2, "Expected 2 evidence items, got {}", by_task.len());
        
        // Strict chronological ordering validation: INSERT ORDER == RETRIEVAL ORDER
        assert_eq!(by_task[0].kind, kind::VERIFICATION, "First evidence must be VERIFICATION (chronological order)");
        assert!(by_task[0].success);
        assert_eq!(by_task[1].kind, kind::ROLLBACK, "Second evidence must be ROLLBACK (chronological order)");
        assert!(!by_task[1].success, "a rollback documents a failure");

        let by_corr = for_correlation(&conn, "corr-2").unwrap();
        assert_eq!(by_corr.len(), 1);
        assert_eq!(by_corr[0].content, "42");

        assert!(for_task(&conn, "no-such-task").unwrap().is_empty());

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
