pub mod diff;
pub mod git;
pub mod selfanalyze;
pub mod suggest;

use crate::core::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// The read-only output of "analyze -> generate suggestion -> diff". Nothing
/// in producing this touches git or the filesystem beyond reading - no
/// branch, no commit, no write. The frontend shows this to a human, who
/// must explicitly approve before apply_self_improvement is ever called.
#[derive(Serialize, Deserialize, Clone)]
pub struct SelfImprovementProposal {
    pub title: String,
    pub slug: String,
    pub file_path: String,
    pub rationale: String,
    pub original_content: String,
    pub proposed_content: String,
    pub risk_summary: String,
    pub diff: String,
}

#[derive(Serialize, Clone)]
pub struct SelfImprovementResult {
    pub branch_name: String,
    pub diff: String,
    pub tests_passed: bool,
    pub test_output: String,
    pub pr_summary: String,
}

fn format_pr_summary(proposal: &SelfImprovementProposal, branch_name: &str, tests_passed: bool, test_output: &str) -> String {
    format!(
        "# {title}\n\n\
        Branch: `{branch_name}` (created locally - not pushed anywhere)\n\
        File: `{file}`\n\n\
        ## Why\n{rationale}\n\n\
        ## Risk\n{risk}\n\n\
        ## Test results: {status}\n```\n{test_output}\n```\n\n\
        ## Diff\n```diff\n{diff}\n```\n\n\
        ---\n\
        This branch exists only in your local git repository. NeuralForge does not push branches, \
        open pull requests, or merge anything automatically - review the diff and test results above, \
        then push and open a PR yourself if you approve.",
        title = proposal.title,
        file = proposal.file_path,
        rationale = proposal.rationale,
        risk = proposal.risk_summary,
        status = if tests_passed { "PASSED" } else { "FAILED" },
        diff = proposal.diff,
    )
}

/// "Self-analysis" + "Generate suggestions" + "Create diff", all in one
/// read-only round trip: scans the open workspace's own source files and
/// project memory, asks the model to pick one focused improvement, plans
/// the change (reusing agent::planner::plan_change - same Simulation Mode
/// contract, no filesystem writes), and renders a diff. No git operation
/// happens until a human approves via apply_self_improvement.
#[tauri::command]
pub async fn propose_self_improvement(state: tauri::State<'_, crate::core::state::AppState>) -> AppResult<SelfImprovementProposal> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let analysis = selfanalyze::analyze(&root)?;
    let target = suggest::choose_target(&analysis).await?;

    let original_content =
        std::fs::read_to_string(root.join(&target.file_path)).map_err(|e| AppError::Provider(format!("failed to read {}: {e}", target.file_path)))?;

    let (proposed_content, risk_summary) = crate::agent::planner::plan_change(&target.rationale, &target.file_path, &original_content)
        .await
        .map_err(|e| AppError::Provider(format!("failed to plan the suggested change: {e}")))?;

    let diff = diff::unified_diff(&target.file_path, &original_content, &proposed_content);

    tracing::info!(target: "bootstrap", event = "self_improvement_proposed", file = %target.file_path, title = %target.title);

    Ok(SelfImprovementProposal {
        title: target.title,
        slug: target.slug,
        file_path: target.file_path,
        rationale: target.rationale,
        original_content,
        proposed_content,
        risk_summary,
        diff,
    })
}

/// Sprint 4: records the bootstrap flow's outcome through the same
/// governance chain the agent flow uses - a real agent_tasks row, a real
/// evidence row carrying the actual test output, and a promotion verdict
/// from the shared PromotionController (PROMOTED when the tests passed,
/// BLOCKED when they failed). Pure bookkeeping: the git branch/commit/test
/// behavior is complete before this runs and is not altered by it.
pub fn record_promotion_bookkeeping(
    conn: &rusqlite::Connection,
    proposal: &SelfImprovementProposal,
    tests_passed: bool,
    test_output: &str,
) -> AppResult<crate::governance::promotion::PromotionRequest> {
    let task_id = format!("bootstrap-{}", uuid::Uuid::new_v4());
    // Sprint 7: task + evidence + promotion verdict land atomically.
    crate::database::in_transaction(conn, |conn| {
    crate::agent::insert_task(
        conn,
        &task_id,
        &proposal.title,
        crate::agent::task_type::EDIT_FILE,
        &proposal.file_path,
        if tests_passed { crate::agent::status::COMPLETED } else { crate::agent::status::FAILED },
        &proposal.original_content,
        &proposal.proposed_content,
        &proposal.risk_summary,
        None,
        None,
    )?;
    crate::governance::evidence::record(
        conn,
        &task_id,
        None,
        crate::governance::evidence::kind::VERIFICATION,
        test_output,
        tests_passed,
    )?;
    crate::governance::promotion::request_promotion(conn, &task_id, None)
    })
}

/// "Create branch" + "Run tests" + "Format PR". Only reachable after a
/// human has reviewed the proposal's diff and explicitly clicked Approve -
/// the frontend gates this call behind that click, the same discipline as
/// agent::approve_task. Creates a real local branch, writes and commits the
/// change, runs whatever real test suite covers the file, and formats a
/// human-readable summary. Never pushes to a remote, never opens a PR,
/// never merges - see ARCHITECTURE.md "Self-bootstrap (Phase 7)".
#[tauri::command]
pub async fn apply_self_improvement(
    state: tauri::State<'_, crate::core::state::AppState>,
    db: tauri::State<'_, crate::database::DbState>,
    proposal: SelfImprovementProposal,
) -> AppResult<SelfImprovementResult> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let branch_name = git::create_branch(&root, &proposal.slug).await?;
    git::write_and_commit(&root, &proposal.file_path, &proposal.proposed_content, &proposal.title).await?;
    let (tests_passed, test_output) = git::run_tests(&root, &proposal.file_path).await?;

    // Sprint 4: bookkeep the outcome through the shared
    // PromotionController. Best-effort by design - a workspace without an
    // open DB still gets the exact same git branch/commit/test behavior
    // and result payload as before this sprint.
    {
        let guard = db.conn.lock().unwrap();
        if let Some(conn) = guard.as_ref() {
            let _ = record_promotion_bookkeeping(conn, &proposal, tests_passed, &test_output);
        }
    }

    let pr_summary = format_pr_summary(&proposal, &branch_name, tests_passed, &test_output);

    tracing::info!(target: "bootstrap", event = "self_improvement_applied", branch = %branch_name, tests_passed);

    Ok(SelfImprovementResult { branch_name, diff: proposal.diff.clone(), tests_passed, test_output, pr_summary })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_throwaway_repo() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_bootstrap_gate_test_{nanos}"));
        std::fs::create_dir_all(dir.join("src")).unwrap();

        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"throwaway_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(
            dir.join("src").join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {\n        assert_eq!(super::add(2, 2), 4);\n    }\n}\n",
        )
        .unwrap();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        std::fs::write(
            dir.join(".neuralforge").join("memory").join("architecture.md"),
            "# Architecture\n\nA throwaway fixture crate used only by NeuralForge's own self-bootstrap gate test.",
        )
        .unwrap();

        std::process::Command::new("git").arg("init").arg("--quiet").current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "test@example.com"]).current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "Test"]).current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "initial"]).current_dir(&dir).output().unwrap();

        dir
    }

    /// End-to-end gate test on a throwaway repo (never the live NeuralForge
    /// checkout): proves self-analysis finds real source files, a proposal
    /// produces a real diff, approval creates a real git branch with a real
    /// commit, and a real `cargo test` run passes against the changed file.
    /// The "generate suggestion" step itself is supplied deterministically
    /// here rather than via a live Ollama call (see suggest::tests and
    /// agent::planner::tests for the live-model-dependent, #[ignore]'d
    /// coverage of that step) - this test's job is to prove the mechanical
    /// pipeline around it (analyze/diff/branch/commit/test) is real.
    #[tokio::test]
    async fn gate_test_self_improvement_lifecycle_on_a_throwaway_repo() {
        let dir = temp_throwaway_repo();

        // "NeuralForge analyzes own code"
        let analysis = selfanalyze::analyze(&dir).unwrap();
        assert!(analysis.source_files.contains(&"src/lib.rs".to_string()));
        assert!(analysis.memory_context.contains("throwaway fixture"));

        // "Proposes refactoring suggestion" + "Generates diff for human review"
        let original_content = std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap();
        let proposed_content = original_content.replace("a + b\n", "a + b // sum of the two operands\n");
        assert_ne!(original_content, proposed_content, "fixture change should actually differ");

        let title = "Document the add() return value".to_string();
        let proposal = SelfImprovementProposal {
            slug: suggest::slugify(&title),
            title,
            file_path: "src/lib.rs".to_string(),
            rationale: "Clarifies what the addition computes for future readers".to_string(),
            diff: diff::unified_diff("src/lib.rs", &original_content, &proposed_content),
            original_content,
            proposed_content,
            risk_summary: "low risk: +1/-1 lines".to_string(),
        };
        assert!(proposal.diff.contains("+     a + b // sum of the two operands"));
        assert!(proposal.diff.contains("-     a + b"));

        // "YOU review + approve/reject" - this test takes the approve path,
        // exercising exactly what apply_self_improvement's body does.
        let branch_name = git::create_branch(&dir, &proposal.slug).await.unwrap();
        git::write_and_commit(&dir, &proposal.file_path, &proposal.proposed_content, &proposal.title).await.unwrap();
        let (tests_passed, test_output) = git::run_tests(&dir, &proposal.file_path).await.unwrap();
        let pr_summary = format_pr_summary(&proposal, &branch_name, tests_passed, &test_output);

        // "Creates branch + applies changes"
        assert!(branch_name.starts_with("neuralforge/suggest-"));
        assert_eq!(std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap(), proposal.proposed_content);

        // "Runs tests (all pass)"
        assert!(tests_passed, "expected the real cargo test run to pass, got:\n{test_output}");
        assert!(test_output.contains("test result: ok"));

        // Never pushed - no remote was ever configured on this throwaway repo.
        let remotes = std::process::Command::new("git").arg("remote").current_dir(&dir).output().unwrap();
        assert!(String::from_utf8_lossy(&remotes.stdout).trim().is_empty());

        assert!(pr_summary.contains("PASSED"));
        assert!(pr_summary.contains(&branch_name));
        assert!(pr_summary.contains("does not push branches"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Sprint 4 test 4: the Phase 7 bootstrap flow, now routed through the
    /// shared PromotionController (write via write_promoted_content inside
    /// git::write_and_commit, plus promotion bookkeeping), produces the
    /// exact same observable behavior the pre-refactor gate test asserts:
    /// same branch name shape, same commit, same file content, same test
    /// outcome, no remote. On top of that, the outcome is now a queryable
    /// promotion row judged from real evidence.
    #[tokio::test]
    async fn gate_test_bootstrap_flow_via_promotion_controller_matches_pre_refactor_behavior() {
        let dir = temp_throwaway_repo();
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        let original_content = std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap();
        let proposed_content = original_content.replace("a + b\n", "a + b // sum of the two operands\n");
        let title = "Document the add() return value".to_string();
        let proposal = SelfImprovementProposal {
            slug: suggest::slugify(&title),
            title,
            file_path: "src/lib.rs".to_string(),
            rationale: "Clarifies what the addition computes".to_string(),
            diff: diff::unified_diff("src/lib.rs", &original_content, &proposed_content),
            original_content,
            proposed_content,
            risk_summary: "low risk: +1/-1 lines".to_string(),
        };

        // Identical sequence to apply_self_improvement's body.
        let branch_name = git::create_branch(&dir, &proposal.slug).await.unwrap();
        git::write_and_commit(&dir, &proposal.file_path, &proposal.proposed_content, &proposal.title).await.unwrap();
        let (tests_passed, test_output) = git::run_tests(&dir, &proposal.file_path).await.unwrap();
        let promotion = record_promotion_bookkeeping(&conn, &proposal, tests_passed, &test_output).unwrap();

        // Pre-refactor observable behavior, unchanged:
        assert!(branch_name.starts_with("neuralforge/suggest-"));
        assert_eq!(std::fs::read_to_string(dir.join("src").join("lib.rs")).unwrap(), proposal.proposed_content);
        assert!(tests_passed, "real cargo test must pass: {test_output}");
        assert!(test_output.contains("test result: ok"));
        let log = std::process::Command::new("git").args(["log", "-1", "--pretty=%s"]).current_dir(&dir).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "neuralforge: Document the add() return value");
        let remotes = std::process::Command::new("git").arg("remote").current_dir(&dir).output().unwrap();
        assert!(String::from_utf8_lossy(&remotes.stdout).trim().is_empty(), "never pushed anywhere");

        // New Sprint 4 guarantees on top:
        assert_eq!(promotion.status, crate::governance::promotion::status::PROMOTED);
        assert!(promotion.promoted_at.is_some());
        let evidence = crate::governance::evidence::for_task(&conn, &promotion.task_id).unwrap();
        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].success);
        assert!(evidence[0].content.contains("test result: ok"), "evidence carries the real test output");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}
