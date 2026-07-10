//! Sprint 10: Release Candidate Validation for v1.1.
//!
//! Entirely `#[cfg(test)]` - this file compiles to NOTHING in a shipped
//! binary and can provably not alter production behavior. The default
//! `cargo test` run includes the fast scenarios (fresh-database,
//! aged-database upgrade); the long-running and repeated-north-star
//! scenarios are `#[ignore]`d so the everyday suite stays fast, and are
//! run explicitly via:
//!
//! ```text
//! cargo test release_validation -- --ignored
//! ```

#![cfg(test)]

use crate::agent::{self, status};
use crate::governance::{evidence, ledger, promotion, requirements};
use rusqlite::Connection;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn temp_workspace(tag: &str) -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    dir.push(format!("neuralforge_release_{tag}_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_requirement(conn: &Connection, title: &str) -> requirements::RequirementContract {
    requirements::create(
        conn,
        title,
        "release validation requirement exercising the full governed pipeline",
        vec!["the change applies and verifies".to_string(), "the audit chain records it".to_string()],
        "release-validation",
    )
    .unwrap()
}

/// One complete pipeline cycle against a real workspace file through the
/// real executor and the real atomic outcome recorder. Returns the task id.
async fn full_cycle(conn: &Connection, dir: &std::path::Path, req: &requirements::RequirementContract, file: &str, original: &str, proposed: &str) -> String {
    let task_id = uuid::Uuid::new_v4().to_string();
    agent::insert_task(
        conn,
        &task_id,
        &agent::objective_from_requirement(req),
        agent::task_type::EDIT_FILE,
        file,
        status::APPLYING,
        original,
        proposed,
        "low risk: release validation",
        Some(&req.id),
        Some(&req.correlation_id),
    )
    .unwrap();

    let result = agent::executor::apply_and_verify(dir, file, original, proposed).await.unwrap();
    let task = agent::get_task(conn, &task_id).unwrap();
    let final_status = if result.rolled_back { status::ROLLED_BACK } else { status::COMPLETED };
    let error = if result.rolled_back { Some("verification failed") } else { None };
    let note = if result.rolled_back { Some("original content restored after failed verification") } else { None };
    agent::record_task_outcome_atomic(conn, &task, &task_id, final_status, &result.verification, error, note).unwrap();
    task_id
}

// =====================================================================
// Task 2 - Fresh database validation (runs in the default suite)
// =====================================================================

/// A brand-new workspace: schema creates cleanly, every table starts
/// empty, and the very first requirement -> task -> execution -> evidence
/// -> promotion cycle works, ending with a valid hash chain.
#[tokio::test]
async fn release_fresh_database_first_pipeline_cycle_from_zero() {
    let dir = temp_workspace("fresh");
    let conn = crate::database::open_for_workspace(&dir).unwrap();

    for table in ["requirements", "agent_tasks", "ledger_entries", "evidence", "promotion_requests", "task_dags", "worker_profiles"] {
        let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "fresh workspace must start empty in {table}");
    }
    assert!(ledger::verify_chain(&conn).unwrap().valid, "empty chain is valid");

    std::fs::write(dir.join("notes.md"), "first content").unwrap();
    let req = make_requirement(&conn, "First-ever change in a fresh workspace");
    let task_id = full_cycle(&conn, &dir, &req, "notes.md", "first content", "second content").await;

    // The file really changed; the whole story is recorded and promoted.
    assert_eq!(std::fs::read_to_string(dir.join("notes.md")).unwrap(), "second content");
    assert_eq!(agent::get_task(&conn, &task_id).unwrap().status, status::COMPLETED);
    let ev = evidence::for_task(&conn, &task_id).unwrap();
    assert_eq!(ev.len(), 1);
    assert!(ev[0].success);
    let promos = promotion::for_task(&conn, &task_id).unwrap();
    assert_eq!(promos.len(), 1);
    assert_eq!(promos[0].status, promotion::status::PROMOTED);
    let chain = ledger::list_by_correlation(&conn, &req.correlation_id).unwrap();
    assert!(chain.iter().any(|e| e.event_type == "requirement_created"));
    assert!(chain.iter().any(|e| e.event_type == "task_completed"));
    assert!(chain.iter().any(|e| e.event_type == "promotion_approved"));
    let verification = ledger::verify_chain(&conn).unwrap();
    assert!(verification.valid, "{:?}", verification.problem);

    drop(conn);
    std::fs::remove_dir_all(&dir).ok();
}

// =====================================================================
// Task 3 - Existing-database upgrade validation (default suite)
// =====================================================================

/// An AGED database carrying every layer's data - requirement versions,
/// completed and failed tasks, a DAG with a blocked->retry->reopened
/// history, retry lineage, evidence, promotions, worker profiles with
/// derived reliability, and a sizeable ledger - is closed and reopened
/// (migrations re-run) three times. Nothing may be lost, the chain must
/// keep verifying, and every read API must keep working over the old rows.
#[tokio::test]
async fn release_aged_database_survives_reopen_and_all_read_apis_work() {
    let dir = temp_workspace("aged");
    let tables = ["requirements", "requirement_history", "agent_tasks", "ledger_entries", "evidence", "promotion_requests", "task_dags", "worker_profiles"];

    // ---- Build the aged state ----
    let (counts_before, req_corr, dag_id, retry_id, worker_score) = {
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        // Requirement with a version bump (populates requirement_history).
        let req = make_requirement(&conn, "Aged requirement");
        requirements::update(&conn, &req.id, "Aged requirement v2", "updated intent for the aged-database validation run", vec!["still checkable".to_string()]).unwrap();
        let req = requirements::get_active(&conn, &req.id).unwrap();

        // A completed single task with real executor + real cargo fixture.
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"aged_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let original_rs = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(dir.join("src").join("lib.rs"), original_rs).unwrap();
        std::fs::write(dir.join("notes.md"), "old").unwrap();
        full_cycle(&conn, &dir, &req, "notes.md", "old", "new").await;

        // A DAG whose first node fails (real broken rust -> real rollback),
        // blocking its dependent; then a successful retry reopens it.
        let record = crate::planning::planner::plan_dag(
            &conn,
            &req,
            &[
                crate::planning::planner::DagTaskSpec { file_path: "src/lib.rs".to_string(), note: None, depends_on: vec![] },
                crate::planning::planner::DagTaskSpec { file_path: "notes.md".to_string(), note: None, depends_on: vec![0] },
            ],
            &[original_rs.to_string(), "new".to_string()],
        )
        .unwrap();
        let lib_task = record.task_ids[0].clone();
        let broken = "pub fn add(a: i32, b: i32) -> i32 { a + b +\n";
        let fail = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, broken).await.unwrap();
        assert!(fail.rolled_back);
        let lib = agent::get_task(&conn, &lib_task).unwrap();
        agent::record_task_outcome_atomic(&conn, &lib, &lib_task, status::ROLLED_BACK, &fail.verification, Some("verification failed"), Some("restored")).unwrap();

        let decision = crate::intelligence::reliability::request_retry(&conn, &lib_task, &crate::intelligence::reliability::RetryPolicy::default()).unwrap();
        assert!(decision.allowed);
        let retry_id = decision.retry_task_id.unwrap();
        let fixed = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let ok = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, fixed).await.unwrap();
        assert!(!ok.rolled_back);
        let retry_task = agent::get_task(&conn, &retry_id).unwrap();
        agent::record_task_outcome_atomic(&conn, &retry_task, &retry_id, status::COMPLETED, &ok.verification, None, None).unwrap();

        // Worker with derived reliability over real verdicts.
        crate::intelligence::registry::upsert(&conn, &crate::intelligence::registry::WorkerProfile {
            id: "aged-coder".to_string(),
            name: "Aged Coder".to_string(),
            capabilities: vec!["coding".to_string(), "testing".to_string()],
            reliability_score: 1.0,
            tasks_completed: 0,
            tasks_failed: 0,
        }).unwrap();
        crate::intelligence::registry::assign_task(&conn, &lib_task, "aged-coder").unwrap();
        crate::intelligence::registry::assign_task(&conn, &retry_id, "aged-coder").unwrap();
        let profile = crate::intelligence::registry::refresh_reliability(&conn, "aged-coder").unwrap();

        // Ledger volume on top.
        for i in 0..100 {
            ledger::append(&conn, ledger::LedgerEvent::TaskCreated, Some("aged-volume"), None, Some(&format!("vol-{i}")), serde_json::json!({"i": i})).unwrap();
        }
        assert!(ledger::verify_chain(&conn).unwrap().valid);

        let counts: Vec<i64> = tables.iter().map(|t| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()).collect();
        assert!(counts.iter().all(|&n| n > 0), "every table must carry aged data: {counts:?}");
        (counts, req.correlation_id.clone(), record.id.clone(), retry_id, profile.reliability_score)
        // conn dropped: simulated shutdown.
    };

    // ---- Reopen three times (each reopen re-runs schema + migrations) ----
    for round in 1..=3 {
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        let counts_after: Vec<i64> = tables.iter().map(|t| conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()).collect();
        assert_eq!(counts_before, counts_after, "reopen round {round}: no migration may drop or truncate anything");

        let verification = ledger::verify_chain(&conn).unwrap();
        assert!(verification.valid, "round {round}: chain must verify: {:?}", verification.problem);

        // Every read API still functions over the aged rows.
        let report = crate::intelligence::reliability::task_report(&conn, &retry_id).unwrap();
        assert_eq!(report.attempts, 2, "retry lineage visible after reopen");
        assert!(report.completeness.complete, "aged record complete: {:?}", report.completeness.missing);
        assert!(report.confidence.score > 0.0);

        let (dag_record, _) = crate::planning::planner::load_dag(&conn, &dag_id).unwrap();
        assert_eq!(dag_record.correlation_id, req_corr);
        // The reopened dependent (notes.md node) is runnable over aged data.
        let runnable = agent::dag_runnable_tasks(&conn, &dag_id).unwrap();
        assert_eq!(runnable.len(), 1, "round {round}: reopened dependent stays runnable");

        let profiles = crate::intelligence::registry::list(&conn).unwrap();
        let best = crate::intelligence::matcher::best_match(&profiles, &["testing".to_string()]).unwrap();
        assert_eq!(best.profile.id, "aged-coder");
        assert_eq!(best.profile.reliability_score, worker_score, "derived score survives reopen");

        assert_eq!(ledger::list_by_correlation(&conn, "aged-volume").unwrap().len(), 100);
        drop(conn);
    }

    std::fs::remove_dir_all(&dir).ok();
}

// =====================================================================
// Task 4 - Long-running agent validation (ignored: run explicitly via
//          `cargo test release_validation -- --ignored`)
// =====================================================================

/// 60 sequential full pipeline cycles in one workspace/connection - 54
/// fast markdown edits plus a real cargo-check cycle every 10th - watching
/// for integrity drift, ordering breaks, lock errors, snapshot/temp file
/// accumulation, and per-iteration timing degradation.
#[tokio::test]
#[ignore = "release validation long-run - execute via: cargo test release_validation -- --ignored"]
async fn release_long_run_60_cycles_stays_correct_and_flat() {
    let dir = temp_workspace("longrun");
    let conn = crate::database::open_for_workspace(&dir).unwrap();

    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"longrun_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src").join("lib.rs"), "pub fn f() -> i32 { 0 }\n").unwrap();
    std::fs::write(dir.join("doc.md"), "iteration 0").unwrap();

    let files_before = std::fs::read_dir(&dir).unwrap().count();
    let mut timings_ms: Vec<u128> = Vec::with_capacity(60);
    let mut current_md = "iteration 0".to_string();
    let mut current_rs = "pub fn f() -> i32 { 0 }\n".to_string();

    for i in 1..=60u32 {
        let started = Instant::now();
        let req = make_requirement(&conn, &format!("Long-run cycle {i}"));
        let task_id = if i % 10 == 0 {
            // Real cargo-check cycle.
            let next_rs = format!("pub fn f() -> i32 {{ {i} }}\n");
            let id = full_cycle(&conn, &dir, &req, "src/lib.rs", &current_rs, &next_rs).await;
            current_rs = next_rs;
            id
        } else {
            let next_md = format!("iteration {i}");
            let id = full_cycle(&conn, &dir, &req, "doc.md", &current_md, &next_md).await;
            current_md = next_md;
            id
        };
        // Every cycle must fully succeed - status, evidence, verdict.
        assert_eq!(agent::get_task(&conn, &task_id).unwrap().status, status::COMPLETED, "cycle {i} failed");
        assert_eq!(promotion::for_task(&conn, &task_id).unwrap().last().unwrap().status, promotion::status::PROMOTED, "cycle {i} not promoted");
        timings_ms.push(started.elapsed().as_millis());
    }

    // Integrity after sustained volume.
    let verification = ledger::verify_chain(&conn).unwrap();
    assert!(verification.valid, "chain must verify after 60 cycles: {:?}", verification.problem);
    assert!(verification.entries >= 240, "expected >=4 events per cycle, got {}", verification.entries);

    // Evidence ordering: strictly increasing insertion_sequence globally.
    let seqs: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT insertion_sequence FROM evidence ORDER BY rowid ASC").unwrap();
        let v: Vec<i64> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        v
    };
    assert_eq!(seqs.len(), 60);
    assert!(seqs.windows(2).all(|w| w[1] > w[0]), "evidence insertion order must be strictly monotonic");

    // No snapshot/temp accumulation in the workspace root: the executor
    // cleans up after itself, so after 60 cycles the ONLY new entries may
    // be cargo's own build artifacts (Cargo.lock + target/), created once
    // by the first real cargo check. A real leak grows per cycle.
    let new_entries: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| !["Cargo.toml", "src", "doc.md", ".neuralforge", "Cargo.lock", "target"].contains(&n.as_str()))
        .collect();
    assert!(new_entries.is_empty(), "leaked temp/snapshot files after 60 cycles: {new_entries:?}");
    let files_after = std::fs::read_dir(&dir).unwrap().count();
    assert!(files_after <= files_before + 2, "entry count grew beyond cargo artifacts: {files_before} -> {files_after}");

    // Timing trend: compare markdown-cycle cost early vs late (cargo
    // cycles excluded - their cost is dominated by the compiler). Guard is
    // deliberately generous; it exists to catch O(n^2) blowups, not noise.
    let md_only: Vec<u128> = timings_ms.iter().enumerate().filter(|(i, _)| (i + 1) % 10 != 0).map(|(_, &t)| t).collect();
    let early: u128 = md_only[..10].iter().sum::<u128>() / 10;
    let late: u128 = md_only[md_only.len() - 10..].iter().sum::<u128>() / 10;
    println!("long-run timing: early md-cycle avg {early}ms, late md-cycle avg {late}ms, all timings: {timings_ms:?}");
    assert!(late <= early.max(1) * 3, "per-cycle cost degraded: early avg {early}ms -> late avg {late}ms");

    drop(conn);
    std::fs::remove_dir_all(&dir).ok();
}

// =====================================================================
// Task 5 - Repeated north-star validation (ignored: run explicitly via
//          `cargo test release_validation -- --ignored`)
// =====================================================================

/// The Sprint 6 north-star scenario ("add input validation to a save
/// function, with a test") executed 5 consecutive times in fresh
/// workspaces. Every run must produce the SAME event-type sequence, the
/// same verdicts, and a valid chain - hunting flakiness and the
/// same-second-timestamp bug class specifically.
#[tokio::test]
#[ignore = "release validation repeated north-star - execute via: cargo test release_validation -- --ignored"]
async fn release_north_star_is_deterministic_across_5_runs() {
    let mut sequences: Vec<Vec<String>> = Vec::new();

    for run in 1..=5 {
        let dir = temp_workspace(&format!("northstar{run}"));
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"north_star_repeat\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), "pub mod save;\n\n#[cfg(test)]\nmod save_tests;\n").unwrap();
        let original_save = "pub fn save_to_file(path: &str, content: &str) -> std::io::Result<()> {\n    std::fs::write(path, content)\n}\n";
        std::fs::write(dir.join("src").join("save.rs"), original_save).unwrap();
        let original_tests = "#[test]\nfn placeholder() {\n    assert!(true);\n}\n";
        std::fs::write(dir.join("src").join("save_tests.rs"), original_tests).unwrap();

        let req = make_requirement(&conn, "Validate save_to_file input");
        let record = crate::planning::planner::plan_dag(
            &conn,
            &req,
            &[
                crate::planning::planner::DagTaskSpec { file_path: "src/save.rs".to_string(), note: None, depends_on: vec![] },
                crate::planning::planner::DagTaskSpec { file_path: "src/save_tests.rs".to_string(), note: Some("add the validation tests".to_string()), depends_on: vec![0] },
            ],
            &[original_save.to_string(), original_tests.to_string()],
        )
        .unwrap();

        let validated_save = "pub fn save_to_file(path: &str, content: &str) -> std::io::Result<()> {\n    if path.trim().is_empty() {\n        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, \"path must not be empty\"));\n    }\n    std::fs::write(path, content)\n}\n";
        let validation_tests = "use crate::save::save_to_file;\n\n#[test]\nfn empty_path_is_rejected() {\n    assert!(save_to_file(\"\", \"data\").is_err());\n}\n";
        let mut proposed = std::collections::HashMap::new();
        proposed.insert("src/save.rs".to_string(), (original_save, validated_save));
        proposed.insert("src/save_tests.rs".to_string(), (original_tests, validation_tests));

        for step in 0..2 {
            let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
            assert_eq!(runnable.len(), 1, "run {run} step {step}: exactly one runnable task");
            let task = &runnable[0];
            let file = task.files.first().unwrap().clone();
            let (original, new_content) = proposed[&file];
            let result = agent::executor::apply_and_verify(&dir, &file, original, new_content).await.unwrap();
            assert!(!result.rolled_back, "run {run} step {step}: {}", result.verification);
            let t = agent::get_task(&conn, &task.id).unwrap();
            agent::record_task_outcome_atomic(&conn, &t, &task.id, status::COMPLETED, &result.verification, None, None).unwrap();
        }

        // Real cargo test against the modified code, every run.
        let (tests_passed, out) = crate::bootstrap::git::run_tests(&dir, "src/save.rs").await.unwrap();
        assert!(tests_passed, "run {run}: real cargo test must pass:\n{out}");

        let verification = ledger::verify_chain(&conn).unwrap();
        assert!(verification.valid, "run {run}: {:?}", verification.problem);

        let sequence: Vec<String> = ledger::list_by_correlation(&conn, &req.correlation_id).unwrap().into_iter().map(|e| e.event_type).collect();
        sequences.push(sequence);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    // Determinism: every run produced the identical event-type sequence.
    for (i, seq) in sequences.iter().enumerate().skip(1) {
        assert_eq!(&sequences[0], seq, "run {} produced a different event sequence than run 1:\nrun1: {:?}\nrun{}: {:?}", i + 1, sequences[0], i + 1, seq);
    }
    println!("north-star determinism: 5/5 identical sequences ({} events each): {:?}", sequences[0].len(), sequences[0]);
}
