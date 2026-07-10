pub mod evidence;
pub mod ledger;
pub mod promotion;
pub mod requirements;
pub mod validator;

use crate::core::errors::AppResult;
use crate::database::{with_conn, DbState};
use evidence::EvidenceRecord;
use ledger::{ChainVerification, LedgerEntry};
use requirements::{RequirementContract, RequirementHistoryEntry};
use tauri::State;

/// Sprint 1 scope note: "created_by" is a fixed local identity for now -
/// NeuralForge has no user accounts. The column exists so multi-author
/// attribution is a data migration away, not a schema change.
const LOCAL_USER: &str = "local";

#[tauri::command]
pub fn create_requirement(db: State<DbState>, title: String, intent: String, acceptance_criteria: Vec<String>) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::create(conn, &title, &intent, acceptance_criteria, LOCAL_USER))
}

#[tauri::command]
pub fn update_requirement(db: State<DbState>, id: String, title: String, intent: String, acceptance_criteria: Vec<String>) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::update(conn, &id, &title, &intent, acceptance_criteria))
}

#[tauri::command]
pub fn set_requirement_status(db: State<DbState>, id: String, status: String) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::set_status(conn, &id, &status))
}

#[tauri::command]
pub fn get_requirement(db: State<DbState>, id: String) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::get(conn, &id))
}

#[tauri::command]
pub fn list_requirements(db: State<DbState>) -> AppResult<Vec<RequirementContract>> {
    with_conn(&db, requirements::list)
}

#[tauri::command]
pub fn get_requirement_history(db: State<DbState>, id: String) -> AppResult<Vec<RequirementHistoryEntry>> {
    with_conn(&db, |conn| requirements::history(conn, &id))
}

#[tauri::command]
pub fn get_ledger(db: State<DbState>, limit: usize) -> AppResult<Vec<LedgerEntry>> {
    with_conn(&db, |conn| ledger::list(conn, limit))
}

#[tauri::command]
pub fn get_ledger_for_correlation(db: State<DbState>, correlationId: String) -> AppResult<Vec<LedgerEntry>> {
    with_conn(&db, |conn| ledger::list_by_correlation(conn, &correlationId))
}

#[tauri::command]
pub fn verify_ledger(db: State<DbState>) -> AppResult<ChainVerification> {
    with_conn(&db, ledger::verify_chain)
}

#[tauri::command]
pub fn get_evidence_for_task(db: State<DbState>, taskId: String) -> AppResult<Vec<EvidenceRecord>> {
    with_conn(&db, |conn| evidence::for_task(conn, &taskId))
}

#[tauri::command]
pub fn get_promotions_for_task(db: State<DbState>, taskId: String) -> AppResult<Vec<promotion::PromotionRequest>> {
    with_conn(&db, |conn| promotion::for_task(conn, &taskId))
}

/// Sprint 2 acceptance tests: these drive the real requirement -> task ->
/// execute -> verify flow through the actual executor (real `cargo check`
/// against a scratch crate), not mocks, and prove the ledger/evidence
/// traceability contract end-to-end. Command-layer functions need a live
/// tauri::State, so - same pattern used throughout this codebase - these
/// call the underlying gate/executor/ledger functions directly.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::governance::{evidence, ledger, requirements};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_conn() -> (std::path::PathBuf, rusqlite::Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_governance_integration_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    /// Full requirement -> task -> execute -> verify flow against a real
    /// scratch cargo crate. Every ledger entry and evidence record produced
    /// along the way must share the requirement's correlation_id, and the
    /// whole chain must be retrievable and hash-valid from that one ID.
    #[tokio::test]
    async fn correlation_id_threads_end_to_end_through_ledger_and_evidence() {
        let (dir, conn) = temp_conn();

        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"corr_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let lib_path = dir.join("src").join("lib.rs");
        let original = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(&lib_path, original).unwrap();
        let proposed = "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n";

        let req = requirements::create(
            &conn,
            "Add subtraction helper",
            "Expose a sub() helper alongside add()",
            vec!["sub(5,3) returns 2".to_string()],
            "test-user",
        )
        .unwrap();
        let correlation_id = req.correlation_id.clone();

        let task_id = uuid::Uuid::new_v4().to_string();
        crate::agent::insert_task(
            &conn,
            &task_id,
            "add sub helper",
            crate::agent::task_type::EDIT_FILE,
            "src/lib.rs",
            crate::agent::status::AWAITING_APPROVAL,
            original,
            proposed,
            "low risk",
            Some(&req.id),
            Some(&correlation_id),
        )
        .unwrap();

        ledger::append(
            &conn,
            ledger::LedgerEvent::TaskApproved,
            Some(&correlation_id),
            Some(&req.id),
            Some(&task_id),
            serde_json::json!({}),
        )
        .unwrap();

        let result = crate::agent::executor::apply_and_verify(&dir, "src/lib.rs", original, proposed).await.unwrap();
        assert!(!result.rolled_back, "valid change must not roll back: {}", result.verification);

        evidence::record(&conn, &task_id, Some(&correlation_id), evidence::kind::VERIFICATION, &result.verification, true).unwrap();
        ledger::append(
            &conn,
            ledger::LedgerEvent::TaskCompleted,
            Some(&correlation_id),
            Some(&req.id),
            Some(&task_id),
            serde_json::json!({"verification": result.verification}),
        )
        .unwrap();

        let chain = ledger::list_by_correlation(&conn, &correlation_id).unwrap();
        assert!(chain.len() >= 3, "expected at least requirement_created, task_approved, task_completed");
        assert!(chain.iter().all(|e| e.correlation_id.as_deref() == Some(correlation_id.as_str())));
        assert_eq!(chain.first().unwrap().event_type, "requirement_created");
        assert_eq!(chain.last().unwrap().event_type, "task_completed");

        let ev = evidence::for_correlation(&conn, &correlation_id).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].success);
        assert_eq!(ev[0].task_id, task_id);

        let verification = ledger::verify_chain(&conn).unwrap();
        assert!(verification.valid, "chain must verify: {:?}", verification.problem);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Evidence.success mirrors the real outcome of `cargo check` - a broken
    /// change produces success=false evidence carrying the actual compiler
    /// output, a valid one produces success=true. No mocked verification.
    #[tokio::test]
    async fn evidence_success_reflects_real_cargo_check_pass_and_fail() {
        let (dir, conn) = temp_conn();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"ev_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let lib_path = dir.join("src").join("lib.rs");
        let original = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(&lib_path, original).unwrap();

        let broken = "pub fn add(a: i32, b: i32) -> i32 { a + b +\n";
        let broken_result = crate::agent::executor::apply_and_verify(&dir, "src/lib.rs", original, broken).await.unwrap();
        assert!(broken_result.rolled_back);
        let rec_fail = evidence::record(&conn, "task-fail", None, evidence::kind::VERIFICATION, &broken_result.verification, !broken_result.rolled_back).unwrap();
        assert!(!rec_fail.success);
        assert!(rec_fail.content.contains("cargo check failed"));

        let valid = "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn double(a: i32) -> i32 { a * 2 }\n";
        let ok_result = crate::agent::executor::apply_and_verify(&dir, "src/lib.rs", original, valid).await.unwrap();
        assert!(!ok_result.rolled_back);
        let rec_ok = evidence::record(&conn, "task-ok", None, evidence::kind::VERIFICATION, &ok_result.verification, !ok_result.rolled_back).unwrap();
        assert!(rec_ok.success);
        assert!(rec_ok.content.contains("cargo check passed"));

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
