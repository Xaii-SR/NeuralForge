pub mod dag;
pub mod planner;

use crate::core::errors::AppResult;
use crate::database::{with_conn, DbState};
use tauri::State;

/// Sprint 3 commands - additive alongside the Sprint 1/2 single-task flow.
/// Decomposition specs come from the caller; validation + persistence +
/// ledgering happen in planner::plan_dag.
#[tauri::command]
pub fn plan_requirement_dag(
    state: State<crate::core::state::AppState>,
    db: State<DbState>,
    requirement_id: String,
    specs: Vec<planner::DagTaskSpec>,
) -> AppResult<planner::TaskDagRecord> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| crate::core::errors::AppError::InvalidPath("no workspace open".to_string()))?;

    // Same workspace-containment gate as create_and_plan_task, per file.
    let mut original_contents = Vec::with_capacity(specs.len());
    for spec in &specs {
        let target = root.join(&spec.file_path);
        let canonical_root = std::fs::canonicalize(&root)?;
        let canonical_target = std::fs::canonicalize(&target)
            .map_err(|_| crate::core::errors::AppError::NotFound(spec.file_path.clone()))?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(crate::core::errors::AppError::InvalidPath(format!("{} is outside the workspace", spec.file_path)));
        }
        original_contents.push(std::fs::read_to_string(&target)?);
    }

    with_conn(&db, |conn| {
        let requirement = crate::governance::requirements::get_active(conn, &requirement_id)?;
        planner::plan_dag(conn, &requirement, &specs, &original_contents)
    })
}

#[tauri::command]
pub fn get_dag(db: State<DbState>, dagId: String) -> AppResult<planner::TaskDagRecord> {
    with_conn(&db, |conn| planner::load_dag(conn, &dagId).map(|(record, _)| record))
}

#[tauri::command]
pub fn get_dag_runnable_tasks(db: State<DbState>, dagId: String) -> AppResult<Vec<crate::agent::AgentTask>> {
    with_conn(&db, |conn| {
        planner::load_dag(conn, &dagId)?; // orphan/cycle gate before anything runs
        crate::agent::dag_runnable_tasks(conn, &dagId)
    })
}

/// Sprint 3 acceptance tests: real DB rows, the real (unmodified)
/// executor per node, no mocks. Command-layer functions need a live
/// tauri::State, so - same pattern as Sprint 1/2 - these drive the
/// underlying plan/load/walk functions directly.
#[cfg(test)]
mod tests {
    use super::planner::{plan_dag, load_dag, DagTaskSpec};
    use crate::agent::{self, status};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> (std::path::PathBuf, rusqlite::Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_planning_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        (dir, conn)
    }

    fn make_requirement(conn: &rusqlite::Connection) -> crate::governance::requirements::RequirementContract {
        crate::governance::requirements::create(
            conn,
            "Multi-file docs update",
            "Update the project documentation across several files consistently",
            vec!["each touched file mentions the new feature".to_string()],
            "test-user",
        )
        .unwrap()
    }

    fn spec(file: &str, deps: &[usize]) -> DagTaskSpec {
        DagTaskSpec { file_path: file.to_string(), note: None, depends_on: deps.to_vec() }
    }

    /// Test 1: A->B->A is rejected before ANY row exists - no task_dags
    /// row, no agent_tasks rows.
    #[test]
    fn cycle_is_rejected_before_any_row_is_written() {
        let (dir, conn) = temp_workspace();
        let req = make_requirement(&conn);

        let result = plan_dag(&conn, &req, &[spec("a.md", &[1]), spec("b.md", &[0])], &["".into(), "".into()]);
        assert!(result.unwrap_err().to_string().contains("cycle"));

        let dag_count: i64 = conn.query_row("SELECT COUNT(*) FROM task_dags", [], |r| r.get(0)).unwrap();
        assert_eq!(dag_count, 0, "a rejected DAG must not persist");
        assert!(agent::list_tasks(&conn).unwrap().is_empty(), "no task rows may exist for a rejected DAG");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 2: tasks whose dag_id has no task_dags row are orphans -
    /// load_dag refuses to hand them to any executor. Real rows: the task
    /// exists in agent_tasks, the dag row genuinely does not.
    #[test]
    fn orphaned_tasks_without_a_dag_row_are_rejected() {
        let (dir, conn) = temp_workspace();

        agent::insert_task(&conn, "orphan-task", "objective", agent::task_type::EDIT_FILE, "a.md", status::PLANNING, "", "", "", None, None).unwrap();
        agent::set_dag_membership(&conn, "orphan-task", "no-such-dag", &[]).unwrap();

        let err = load_dag(&conn, "no-such-dag").unwrap_err().to_string();
        assert!(err.contains("orphan"), "got: {err}");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 3: a 3-task chain plans, persists, and executes through the
    /// real executor strictly in topological order (specs deliberately
    /// declared dependency-last).
    #[tokio::test]
    async fn three_task_dag_executes_in_topological_order() {
        let (dir, conn) = temp_workspace();
        for f in ["one.md", "two.md", "three.md"] {
            std::fs::write(dir.join(f), "old").unwrap();
        }
        let req = make_requirement(&conn);

        // three depends on two depends on one.
        let record = plan_dag(
            &conn,
            &req,
            &[spec("three.md", &[1]), spec("two.md", &[2]), spec("one.md", &[])],
            &["old".into(), "old".into(), "old".into()],
        )
        .unwrap();
        assert_eq!(record.task_ids.len(), 3);
        assert_eq!(record.correlation_id, req.correlation_id, "DAG must join the requirement's correlation chain");

        let (loaded, dag) = load_dag(&conn, &record.id).unwrap();
        assert_eq!(loaded.execution_order.len(), 3);

        let file_of = |id: &str| dag.nodes.iter().find(|n| n.id == *id).unwrap().file_path.clone();
        let order_files: Vec<String> = loaded.execution_order.iter().map(|id| file_of(id)).collect();
        assert_eq!(order_files, vec!["one.md", "two.md", "three.md"]);

        // Walk: at each step exactly the expected node is runnable; run it
        // through the real executor, mark completed, repeat.
        let mut executed = Vec::new();
        for _ in 0..3 {
            let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
            assert_eq!(runnable.len(), 1, "chain DAG must expose exactly one runnable task at a time");
            let task = &runnable[0];
            let file = task.files.first().unwrap().clone();
            let result = agent::executor::apply_and_verify(&dir, &file, "old", "new content").await.unwrap();
            assert!(!result.rolled_back);
            agent::update_status(&conn, &task.id, status::COMPLETED, Some(&result.verification), None).unwrap();
            executed.push(file);
        }
        assert_eq!(executed, vec!["one.md", "two.md", "three.md"], "execution must follow topological order");
        assert!(agent::dag_runnable_tasks(&conn, &record.id).unwrap().is_empty(), "nothing left to run");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 4: a REAL cargo-check failure (broken Rust, real rollback by
    /// the unmodified executor) blocks the dependent task but leaves the
    /// independent branch runnable.
    #[tokio::test]
    async fn failure_blocks_dependents_but_not_independent_tasks() {
        let (dir, conn) = temp_workspace();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"dag_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let original_rs = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(dir.join("src").join("lib.rs"), original_rs).unwrap();
        std::fs::write(dir.join("notes.md"), "old").unwrap();
        std::fs::write(dir.join("readme.md"), "old").unwrap();
        let req = make_requirement(&conn);

        // b (src/lib.rs) -> c (notes.md depends on b); d (readme.md) independent.
        let record = plan_dag(
            &conn,
            &req,
            &[spec("src/lib.rs", &[]), spec("notes.md", &[0]), spec("readme.md", &[])],
            &[original_rs.into(), "old".into(), "old".into()],
        )
        .unwrap();
        let b_id = record.task_ids[0].clone();
        let c_id = record.task_ids[1].clone();
        let d_id = record.task_ids[2].clone();

        // Execute b with genuinely broken Rust: the real executor runs the
        // real cargo check, fails, and rolls back - safety logic untouched.
        let broken = "pub fn add(a: i32, b: i32) -> i32 { a + b +\n";
        let result = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, broken).await.unwrap();
        assert!(result.rolled_back, "broken rust must roll back");
        assert_eq!(std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap(), original_rs);
        agent::update_status(&conn, &b_id, status::ROLLED_BACK, Some(&result.verification), Some("verification failed")).unwrap();

        let blocked = agent::propagate_dag_blocks(&conn, &record.id).unwrap();
        assert_eq!(blocked, vec![c_id.clone()], "exactly the dependent task gets blocked");
        assert_eq!(agent::get_task(&conn, &c_id).unwrap().status, status::BLOCKED);

        // Independent branch d is unaffected and still runnable.
        let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].id, d_id);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 7 rollback-reliability edge case: failure on the SECOND node
    /// of a chain after the first succeeded. Only the failed node rolls
    /// back; the completed node's file keeps its new content and its
    /// COMPLETED status; the third (dependent) node blocks. Error text at
    /// every failure point is informative, not Err(()).
    #[tokio::test]
    async fn second_node_failure_rolls_back_only_itself_and_keeps_node_one() {
        let (dir, conn) = temp_workspace();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"partial_fail_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("notes.md"), "old notes").unwrap();
        let original_rs = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(dir.join("src").join("lib.rs"), original_rs).unwrap();
        std::fs::write(dir.join("readme.md"), "old readme").unwrap();
        let req = make_requirement(&conn);

        // Chain: notes.md -> src/lib.rs -> readme.md
        let record = plan_dag(
            &conn,
            &req,
            &[spec("notes.md", &[]), spec("src/lib.rs", &[0]), spec("readme.md", &[1])],
            &["old notes".into(), original_rs.into(), "old readme".into()],
        )
        .unwrap();
        let (n1, n2, n3) = (&record.task_ids[0], &record.task_ids[1], &record.task_ids[2]);

        // Node 1 succeeds through the real executor.
        let ok = agent::executor::apply_and_verify(&dir, "notes.md", "old notes", "new notes").await.unwrap();
        assert!(!ok.rolled_back);
        agent::update_status(&conn, n1, status::COMPLETED, Some(&ok.verification), None).unwrap();

        // Node 2 fails: genuinely broken rust, real cargo check, real rollback.
        let broken = "pub fn add(a: i32, b: i32) -> i32 { a + b +\n";
        let fail = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, broken).await.unwrap();
        assert!(fail.rolled_back);
        // Error reporting is informative: the verifier output carries the
        // actual compiler diagnostics, not a bare error.
        assert!(fail.verification.contains("cargo check failed"), "got: {}", fail.verification);
        assert!(fail.verification.contains("error"), "compiler diagnostics must be present: {}", fail.verification);
        agent::update_status(&conn, n2, status::ROLLED_BACK, Some(&fail.verification), Some("verification failed")).unwrap();
        agent::propagate_dag_blocks(&conn, &record.id).unwrap();

        // Node 1: completed work is untouched by node 2's rollback.
        assert_eq!(std::fs::read_to_string(dir.join("notes.md")).unwrap(), "new notes", "node 1's applied change must survive");
        assert_eq!(agent::get_task(&conn, n1).unwrap().status, status::COMPLETED);
        // Node 2: rolled back on disk and in the DB.
        assert_eq!(std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap(), original_rs);
        assert_eq!(agent::get_task(&conn, n2).unwrap().status, status::ROLLED_BACK);
        // Node 3: blocked, never attempted, file untouched.
        assert_eq!(agent::get_task(&conn, n3).unwrap().status, status::BLOCKED);
        assert_eq!(std::fs::read_to_string(dir.join("readme.md")).unwrap(), "old readme");
        assert!(agent::dag_runnable_tasks(&conn, &record.id).unwrap().is_empty());

        // Error-message informativeness across the seams:
        let orphan = load_dag(&conn, "nonexistent-dag").unwrap_err().to_string();
        assert!(orphan.contains("nonexistent-dag"), "orphan error names the dag: {orphan}");
        let premature = crate::governance::promotion::request_promotion(&conn, n3, None).unwrap();
        let refusal = crate::governance::promotion::apply_to_workspace(&conn, &premature.id, &dir, "readme.md", "x").unwrap_err().to_string();
        assert!(refusal.contains("blocked") && refusal.contains("evidence gate"), "promotion refusal explains itself: {refusal}");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 8: the full retry-recovery sequence over a real DAG with a
    /// real cargo check at every step. Dependency fails (genuinely broken
    /// rust, real rollback) -> dependent blocks -> retry is prepared under
    /// the bounded policy -> the retry executes through the real untouched
    /// executor and passes -> the blocked dependent becomes runnable
    /// again, exactly as if the dependency had succeeded first time.
    #[tokio::test]
    async fn retry_success_reopens_blocked_dependents_via_real_cargo_check() {
        let (dir, conn) = temp_workspace();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"retry_reopen_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let original_rs = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        std::fs::write(dir.join("src").join("lib.rs"), original_rs).unwrap();
        std::fs::write(dir.join("notes.md"), "old").unwrap();
        let req = make_requirement(&conn);

        // notes.md depends on src/lib.rs.
        let record = plan_dag(
            &conn,
            &req,
            &[spec("src/lib.rs", &[]), spec("notes.md", &[0])],
            &[original_rs.into(), "old".into()],
        )
        .unwrap();
        let lib_task = record.task_ids[0].clone();
        let notes_task = record.task_ids[1].clone();

        // Attempt 1: genuinely broken rust -> real cargo check failure ->
        // real rollback -> dependent blocks.
        let broken = "pub fn add(a: i32, b: i32) -> i32 { a + b +\n";
        let fail = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, broken).await.unwrap();
        assert!(fail.rolled_back);
        let lib = agent::get_task(&conn, &lib_task).unwrap();
        agent::record_task_outcome_atomic(&conn, &lib, &lib_task, status::ROLLED_BACK, &fail.verification, Some("verification failed"), Some("original content restored")).unwrap();
        assert_eq!(agent::get_task(&conn, &notes_task).unwrap().status, status::BLOCKED, "dependent must block after the failure");
        assert!(agent::dag_runnable_tasks(&conn, &record.id).unwrap().is_empty());

        // Retry prepared under the bounded policy; classified from the
        // REAL compiler output the executor recorded.
        let decision = crate::intelligence::reliability::request_retry(&conn, &lib_task, &crate::intelligence::reliability::RetryPolicy::default()).unwrap();
        assert!(decision.allowed, "{}", decision.reason);
        assert_eq!(decision.failure_class, crate::intelligence::reliability::FailureClass::CompileError);
        let retry_id = decision.retry_task_id.unwrap();

        // While the retry is only PENDING the dependent stays blocked and
        // nothing but the retry itself is runnable.
        let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].id, retry_id, "the retry is the only runnable work");
        assert_eq!(agent::get_task(&conn, &notes_task).unwrap().status, status::BLOCKED);

        // The retry executes through the real executor with a FIXED change
        // and passes a real cargo check.
        let fixed = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b // sum\n}\n";
        let ok = agent::executor::apply_and_verify(&dir, "src/lib.rs", original_rs, fixed).await.unwrap();
        assert!(!ok.rolled_back, "{}", ok.verification);
        let retry_task = agent::get_task(&conn, &retry_id).unwrap();
        agent::record_task_outcome_atomic(&conn, &retry_task, &retry_id, status::COMPLETED, &ok.verification, None, None).unwrap();

        // THE RE-OPEN: the blocked dependent is plannable and runnable again.
        assert_eq!(agent::get_task(&conn, &notes_task).unwrap().status, status::PLANNING, "dependent must reopen after the retry succeeds");
        let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].id, notes_task);

        // The reopen is on the ledger and the file really changed.
        let chain = crate::governance::ledger::list_by_correlation(&conn, &req.correlation_id).unwrap();
        assert!(chain.iter().any(|e| e.event_type == "task_retried"));
        assert!(chain.iter().any(|e| e.event_type == "task_planned" && e.payload.contains("reopened")));
        assert_eq!(std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap(), fixed);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// SPRINT 6 NORTH-STAR TEST: the full autonomous engineering loop,
    /// every layer real, no mocks anywhere in the chain.
    ///
    /// Scenario: "add input validation to a file save function, with a
    /// test", driven through the actual pipeline on a throwaway cargo
    /// crate:
    ///   Sprint 1  - requirement created + validated
    ///   Sprint 3  - decomposed into a 2-node DAG (test task depends on
    ///               the edit task), topological walk
    ///   Sprint 5  - each task routed to a worker via the capability
    ///               matcher and REALLY assigned (worker_id stamped)
    ///   untouched executor - real snapshot/apply/cargo-check per node
    ///   Sprint 2  - real evidence rows carrying the actual verifier output
    ///   Sprint 4  - promotion gate judges that evidence before the task
    ///               counts as promotable
    ///   finale    - the real files changed and a real `cargo test` passes
    ///               against the modified code
    #[tokio::test]
    async fn north_star_full_autonomous_loop_add_validation_with_test() {
        let (dir, conn) = temp_workspace();

        // A real (throwaway) crate with a save function that lacks
        // validation, and a test module with only a placeholder test.
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"north_star_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), "pub mod save;\n\n#[cfg(test)]\nmod save_tests;\n").unwrap();
        let original_save = "pub fn save_to_file(path: &str, content: &str) -> std::io::Result<()> {\n    std::fs::write(path, content)\n}\n";
        std::fs::write(dir.join("src").join("save.rs"), original_save).unwrap();
        let original_tests = "#[test]\nfn placeholder() {\n    assert!(true);\n}\n";
        std::fs::write(dir.join("src").join("save_tests.rs"), original_tests).unwrap();

        // ---- Sprint 1: requirement created and validated ----
        let req = crate::governance::requirements::create(
            &conn,
            "Validate save_to_file input",
            "save_to_file must reject an empty path with an InvalidInput error instead of passing it to the OS, and a test must prove both the rejection and the happy path",
            vec![
                "empty path returns Err(InvalidInput)".to_string(),
                "non-empty path still saves".to_string(),
                "a test covers both behaviors".to_string(),
            ],
            "test-user",
        )
        .unwrap();

        // ---- Sprint 3: decompose into a DAG; the test task depends on the edit task ----
        let record = plan_dag(
            &conn,
            &req,
            &[
                spec("src/save.rs", &[]),
                DagTaskSpec { file_path: "src/save_tests.rs".to_string(), note: Some("add the validation tests".to_string()), depends_on: vec![0] },
            ],
            &[original_save.to_string(), original_tests.to_string()],
        )
        .unwrap();
        assert_eq!(record.execution_order.len(), 2);
        assert_eq!(record.correlation_id, req.correlation_id);

        // ---- Sprint 5: route each task to a worker via the capability matcher ----
        let coder = crate::intelligence::registry::WorkerProfile {
            id: "coder".to_string(),
            name: "Coder".to_string(),
            capabilities: vec!["coding".to_string(), "testing".to_string()],
            reliability_score: 1.0,
            tasks_completed: 0,
            tasks_failed: 0,
        };
        crate::intelligence::registry::upsert(&conn, &coder).unwrap();
        let profiles = crate::intelligence::registry::list(&conn).unwrap();

        for (task_id, required) in [(&record.task_ids[0], "coding"), (&record.task_ids[1], "testing")] {
            let chosen = crate::intelligence::matcher::best_match(&profiles, &[required.to_string()]).unwrap();
            assert!(chosen.missing.is_empty(), "the matched worker must actually cover '{required}'");
            crate::intelligence::registry::assign_task(&conn, task_id, &chosen.profile.id).unwrap();
        }
        // The assignment is real: worker_id is stamped on the rows.
        let assigned: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_tasks WHERE dag_id = ?1 AND worker_id = 'coder'", rusqlite::params![record.id], |r| r.get(0))
            .unwrap();
        assert_eq!(assigned, 2, "both DAG tasks must be genuinely assigned");

        // The engineered changes each node will apply.
        let validated_save = "pub fn save_to_file(path: &str, content: &str) -> std::io::Result<()> {\n    if path.trim().is_empty() {\n        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, \"path must not be empty\"));\n    }\n    std::fs::write(path, content)\n}\n";
        let validation_tests = "use crate::save::save_to_file;\n\n#[test]\nfn empty_path_is_rejected() {\n    let err = save_to_file(\"\", \"data\").unwrap_err();\n    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);\n}\n\n#[test]\nfn whitespace_path_is_rejected() {\n    assert!(save_to_file(\"   \", \"data\").is_err());\n}\n\n#[test]\nfn valid_path_still_saves() {\n    let mut p = std::env::temp_dir();\n    p.push(\"north_star_save_ok.txt\");\n    save_to_file(p.to_str().unwrap(), \"data\").unwrap();\n    assert_eq!(std::fs::read_to_string(&p).unwrap(), \"data\");\n    std::fs::remove_file(&p).ok();\n}\n";
        let mut proposed: std::collections::HashMap<String, (&str, &str)> = std::collections::HashMap::new();
        proposed.insert("src/save.rs".to_string(), (original_save, validated_save));
        proposed.insert("src/save_tests.rs".to_string(), (original_tests, validation_tests));

        // ---- The loop: topological walk, real executor, real evidence,
        // ---- real promotion gate, per node.
        for step in 0..2 {
            let runnable = agent::dag_runnable_tasks(&conn, &record.id).unwrap();
            assert_eq!(runnable.len(), 1, "step {step}: exactly one task may run");
            let task = &runnable[0];
            let file = task.files.first().unwrap().clone();
            let (original, new_content) = proposed[&file];

            // Sprint 4 gate, negative side: BEFORE any evidence exists for
            // this task, promotion is refused.
            let premature = crate::governance::promotion::request_promotion(&conn, &task.id, task.correlation_id.as_deref()).unwrap();
            assert_eq!(premature.status, crate::governance::promotion::status::BLOCKED, "no evidence yet -> not promotable");

            // Untouched executor: snapshot, apply, REAL cargo check, rollback-if-broken.
            let result = agent::executor::apply_and_verify(&dir, &file, original, new_content).await.unwrap();
            assert!(!result.rolled_back, "step {step} must verify cleanly: {}", result.verification);
            assert!(result.verification.contains("cargo check passed"), "got: {}", result.verification);

            // Sprint 2: evidence carrying the actual verifier output.
            crate::governance::evidence::record(
                &conn,
                &task.id,
                task.correlation_id.as_deref(),
                crate::governance::evidence::kind::VERIFICATION,
                &result.verification,
                true,
            )
            .unwrap();
            agent::update_status(&conn, &task.id, status::COMPLETED, Some(&result.verification), None).unwrap();

            // Sprint 4 gate, positive side: with passing evidence, promoted.
            let verdict = crate::governance::promotion::request_promotion(&conn, &task.id, task.correlation_id.as_deref()).unwrap();
            assert_eq!(verdict.status, crate::governance::promotion::status::PROMOTED);
        }
        assert_eq!(agent::dag_runnable_tasks(&conn, &record.id).unwrap().len(), 0, "loop complete");

        // ---- The files really changed in the real workspace ----
        let saved = std::fs::read_to_string(dir.join("src").join("save.rs")).unwrap();
        assert!(saved.contains("ErrorKind::InvalidInput"), "validation must be in the real file");
        assert!(std::fs::read_to_string(dir.join("src").join("save_tests.rs")).unwrap().contains("empty_path_is_rejected"));

        // ---- Real `cargo test` passes against the modified code ----
        let (tests_passed, test_output) = crate::bootstrap::git::run_tests(&dir, "src/save.rs").await.unwrap();
        assert!(tests_passed, "real cargo test must pass:\n{test_output}");
        assert!(test_output.contains("test result: ok"), "got: {test_output}");
        assert!(test_output.contains("empty_path_is_rejected"), "the new validation test must have actually run");

        // ---- The whole story is on ONE correlation chain, and the ledger verifies ----
        let chain = crate::governance::ledger::list_by_correlation(&conn, &req.correlation_id).unwrap();
        let has = |t: &str| chain.iter().any(|e| e.event_type == t);
        assert!(has("requirement_created"));
        assert!(has("task_planned"));
        assert!(has("promotion_requested"));
        assert!(has("promotion_blocked")); // the premature attempts
        assert!(has("promotion_approved")); // the evidenced ones
        assert_eq!(chain.iter().filter(|e| e.event_type == "task_planned").count(), 2);
        let verification = crate::governance::ledger::verify_chain(&conn).unwrap();
        assert!(verification.valid, "hash chain must verify after the full loop: {:?}", verification.problem);

        // ---- Sprint 5 closes the loop: reliability reflects the real outcome ----
        let refreshed = crate::intelligence::registry::refresh_reliability(&conn, "coder").unwrap();
        assert_eq!(refreshed.tasks_completed, 2);
        assert_eq!(refreshed.tasks_failed, 0);
        assert_eq!(refreshed.reliability_score, 1.0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test 5 (regression): the Sprint 1/2 single-task flow never touches
    /// DAG columns - a task inserted the old way has no dag_id, no
    /// depends_on, and round-trips exactly as before.
    #[test]
    fn single_task_flow_is_unchanged_by_dag_support() {
        let (dir, conn) = temp_workspace();

        agent::insert_task(&conn, "solo-task", "add a comment", agent::task_type::EDIT_FILE, "main.rs", status::AWAITING_APPROVAL, "old", "new", "low", Some("req-1"), Some("corr-1")).unwrap();
        let task = agent::get_task(&conn, "solo-task").unwrap();
        assert!(task.dag_id.is_none(), "single-task flow must not acquire a dag_id");
        assert!(task.depends_on.is_empty());
        assert_eq!(task.status, status::AWAITING_APPROVAL);
        assert_eq!(task.correlation_id.as_deref(), Some("corr-1"));

        agent::update_status(&conn, "solo-task", status::COMPLETED, Some("cargo check passed"), None).unwrap();
        assert_eq!(agent::get_task(&conn, "solo-task").unwrap().status, status::COMPLETED);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
