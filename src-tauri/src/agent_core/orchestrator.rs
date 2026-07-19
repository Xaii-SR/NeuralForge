//! AgentCore's coordination layer: forwards requests to the existing
//! execution authorities (`agent::`, `agent_v2`) and records which backend
//! handled each task. Owns no execution logic of its own for the forwarding
//! functions below - each is a direct call into an existing public
//! function, per the "No God Object" rule. Nothing in those functions
//! touches `provider_router`, `std::fs`, rollback, or verification
//! directly; those remain entirely inside the execution authorities being
//! forwarded to.
//!
//! `run_council_pass` (Council v1) is the one exception: it genuinely calls
//! `ai::provider_router::generate_for_task` - AgentCore's sanctioned reuse
//! of the same single AI entry point `agent_v2` uses, not a new transport.
//! It still owns no execution/file/rollback logic of its own; the sequencing
//! core (`run_council_pass_with`) is generic over how a role's response is
//! obtained, so the real LLM call is injected by the thin `run_council_pass`
//! wrapper rather than hardcoded into the testable sequencing logic.

use crate::agent_core::lifecycle::{AgentLifecycleState, ExecutionBackend};
use crate::agent_core::types::{AgentEventType, AgentRole, CouncilError, CouncilPassResult, CouncilVerdict};
use crate::agent_core::AgentCoreState;
use crate::agent_v2::ApprovalRegistry;
use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry;
use crate::ai::provider_router::{self, TaskCapability};
use crate::core::errors::AppResult;
use crate::core::state::AppState;
use crate::database::DbState;
use tauri::{AppHandle, Manager, State};

/// Records which backend handled `task_id`, for future coordination/
/// observability (e.g. a unified task list spanning both backends). Purely
/// additive bookkeeping - never consulted to make a routing decision today,
/// since each entry point below already knows its own backend statically.
fn record_backend(core: &AgentCoreState, task_id: &str, backend: ExecutionBackend) {
    if let Ok(mut map) = core.task_backends.lock() {
        map.insert(task_id.to_string(), backend);
    }
}

/// Gives `task_id` its own advisory lifecycle instance in `core.agent_registry`,
/// under the `Architect` role (see `registry::AgentRegistry`'s doc comment
/// for why per-(task, role) isolation matters). `Architect` is the default
/// single-agent role until a real multi-role Council pass registers other
/// roles under the same task_id too (see `types::AgentRole`'s doc comment).
/// Best-effort like `record_backend` - a poisoned registry lock must not
/// fail task creation over advisory bookkeeping.
fn register_lifecycle(core: &AgentCoreState, task_id: &str) {
    let _ = core.agent_registry.register(task_id.to_string(), AgentRole::Architect, AgentLifecycleState::Created);
}

// ── Governed pipeline (agent::) forwarding ──────────────────────────────

/// Forwards to `agent::create_and_plan_task` unchanged. AgentCore does not
/// re-implement requirement gating, workspace-boundary checks, or planning
/// - see `agent/mod.rs::create_and_plan_task` (frozen execution authority).
pub async fn create_and_plan_task(
    core: &AgentCoreState,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    requirement_id: String,
    file_path: String,
) -> AppResult<crate::agent::AgentTask> {
    let task = crate::agent::create_and_plan_task(state, db, requirement_id, file_path).await?;
    record_backend(core, &task.id, ExecutionBackend::Governed);
    register_lifecycle(core, &task.id);
    Ok(task)
}

/// Forwards to `agent::create_and_plan_code_task` unchanged.
pub async fn create_and_plan_code_task(
    core: &AgentCoreState,
    db: State<'_, DbState>,
    objective: String,
) -> AppResult<crate::agent::AgentTask> {
    let task = crate::agent::create_and_plan_code_task(db, objective).await?;
    record_backend(core, &task.id, ExecutionBackend::Governed);
    register_lifecycle(core, &task.id);
    Ok(task)
}

/// Forwards to `agent::approve_task` unchanged - real apply/verify/rollback
/// happens entirely inside `agent::executor` (frozen), not here.
pub async fn approve_task(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    task_id: String,
) -> AppResult<crate::agent::AgentTask> {
    crate::agent::approve_task(state, db, task_id).await
}

/// Forwards to `agent::reject_task` unchanged.
pub fn reject_task(db: State<'_, DbState>, task_id: String) -> AppResult<()> {
    crate::agent::reject_task(db, task_id)
}

/// Forwards to `agent::list_agent_tasks` unchanged.
pub fn list_agent_tasks(db: State<'_, DbState>) -> AppResult<Vec<crate::agent::AgentTask>> {
    crate::agent::list_agent_tasks(db)
}

// ── V2 pipeline (agent_v2) forwarding ───────────────────────────────────

/// Forwards to `agent_v2::start_agent_task` unchanged - the real HITL
/// retry loop and its `provider_router` calls live entirely in
/// `agent_v2::AgentRunner::process_task`, not here.
pub async fn start_v2_task(
    core: &AgentCoreState,
    app_handle: AppHandle,
    registry: State<'_, ApprovalRegistry>,
    description: String,
) -> Result<String, String> {
    let task_id = crate::agent_v2::start_agent_task(app_handle, registry, description).await?;
    record_backend(core, &task_id, ExecutionBackend::V2);
    register_lifecycle(core, &task_id);
    Ok(task_id)
}

/// Forwards to `agent_v2::approve_agent_task` unchanged.
pub async fn approve_v2_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    crate::agent_v2::approve_agent_task(id, registry).await
}

/// Forwards to `agent_v2::reject_agent_task` unchanged.
pub async fn reject_v2_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    crate::agent_v2::reject_agent_task(id, registry).await
}

// ── Council v1: sequential Architect -> Critic -> Judge pass ───────────────

/// Advances `role`'s advisory lifecycle under `task_id` from `Created` to
/// `Planning` - fired once, right before that role's real LLM call starts.
/// Best-effort like `register_lifecycle`: a poisoned registry lock must not
/// abort a real, in-flight council pass over advisory bookkeeping.
fn mark_role_started(core: &AgentCoreState, task_id: &str, role: AgentRole) {
    let _ = core.agent_registry.transition(task_id, role, AgentEventType::PlanningStarted);
}

/// Walks `role`'s advisory lifecycle the rest of the way to `Completed`,
/// following the one valid chain `reducer::reduce` supports from `Planning`
/// (see `lifecycle::AgentLifecycleState`'s doc comment for the full
/// transition matrix) - fired once, right after that role's real LLM call
/// succeeds.
fn mark_role_completed(core: &AgentCoreState, task_id: &str, role: AgentRole) {
    let registry = &core.agent_registry;
    let _ = registry.transition(task_id, role, AgentEventType::ApprovalRequested);
    let _ = registry.transition(task_id, role, AgentEventType::ApprovalGranted);
    let _ = registry.transition(task_id, role, AgentEventType::ExecutionStarted);
    let _ = registry.transition(task_id, role, AgentEventType::VerificationStarted);
    let _ = registry.transition(task_id, role, AgentEventType::Completed);
}

/// Marks `role`'s advisory lifecycle `Failed` (absorbing from any state,
/// see `reducer::reduce`) - fired once, right after that role's real LLM
/// call errors.
fn mark_role_failed(core: &AgentCoreState, task_id: &str, role: AgentRole) {
    let _ = core.agent_registry.transition(task_id, role, AgentEventType::Failed);
}

/// Reads the Judge's real output for the one word it was explicitly
/// instructed to open with. `Unclear` is a real, successful outcome (see
/// `types::CouncilVerdict`'s doc comment), not a parse error - a model that
/// doesn't comply with the instruction still produced real output.
fn parse_verdict(judge_output: &str) -> CouncilVerdict {
    let first_word = judge_output
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_uppercase();
    match first_word.as_str() {
        "ACCEPT" => CouncilVerdict::Accept,
        "REJECT" => CouncilVerdict::Reject,
        "REVISE" => CouncilVerdict::Revise,
        _ => CouncilVerdict::Unclear,
    }
}

/// The testable sequencing core of Council v1: given anything that can turn
/// `(role, system_prompt, user_prompt)` into a real response (or a failure
/// reason), runs Architect -> Critic -> Judge strictly in order, each role
/// registered in `core.agent_registry` only once the previous role has
/// genuinely succeeded. `call_role` is real `ai::provider_router::
/// generate_for_task` in production (see `run_council_pass` below) and a
/// deterministic mock in tests - this function never knows or cares which,
/// keeping the sequencing/failure-halting contract testable without a live
/// model or a live `AppHandle` (this codebase's existing tests can't
/// construct one outside a running app - see the "MockRuntime decision"
/// referenced in `agent/mod.rs`).
///
/// Sequential only, by design: Critic's prompt is only built once
/// Architect's real output exists, and Judge's prompt is only built once
/// Critic's real output exists - there is no parallel/speculative call.
/// Any role's failure halts immediately: later roles are never called and
/// never registered (so `current_state` on an unreached role correctly
/// reports `TaskNotFound`, not a stale/default value), and the returned
/// `CouncilError` names exactly which role failed and why - never a
/// silent partial result.
pub async fn run_council_pass_with<F, Fut>(
    core: &AgentCoreState,
    task_id: &str,
    objective: &str,
    mut call_role: F,
) -> Result<CouncilPassResult, CouncilError>
where
    F: FnMut(AgentRole, String, String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    async fn run_role<F, Fut>(
        core: &AgentCoreState,
        task_id: &str,
        role: AgentRole,
        system_prompt: String,
        user_prompt: String,
        call_role: &mut F,
    ) -> Result<String, CouncilError>
    where
        F: FnMut(AgentRole, String, String) -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        let _ = core.agent_registry.register(task_id.to_string(), role, AgentLifecycleState::Created);
        mark_role_started(core, task_id, role);
        match call_role(role, system_prompt, user_prompt).await {
            Ok(output) => {
                mark_role_completed(core, task_id, role);
                Ok(output)
            }
            Err(reason) => {
                mark_role_failed(core, task_id, role);
                Err(CouncilError { role, reason })
            }
        }
    }

    let architect_output = run_role(
        core,
        task_id,
        AgentRole::Architect,
        "You are the Architect. Propose a concrete, specific solution to the user's objective.".to_string(),
        objective.to_string(),
        &mut call_role,
    )
    .await?;

    let critic_output = run_role(
        core,
        task_id,
        AgentRole::Critic,
        "You are the Critic. Review the Architect's proposal for correctness, risks, and gaps. Be specific.".to_string(),
        format!("Objective: {objective}\n\nArchitect's proposal:\n{architect_output}"),
        &mut call_role,
    )
    .await?;

    let judge_output = run_role(
        core,
        task_id,
        AgentRole::Judge,
        "You are the Judge. Read the objective, the Architect's proposal, and the Critic's review, then give a final verdict. Begin your response with exactly one word - ACCEPT, REJECT, or REVISE - then explain your reasoning.".to_string(),
        format!("Objective: {objective}\n\nArchitect's proposal:\n{architect_output}\n\nCritic's review:\n{critic_output}"),
        &mut call_role,
    )
    .await?;

    let judge_verdict = parse_verdict(&judge_output);

    Ok(CouncilPassResult { architect_output, critic_output, judge_output, judge_verdict })
}

/// The real Council v1 pass: resolves `providers`/`health` from `app_handle`
/// exactly the way `agent_v2`'s private `generate()` helper does (mirrored,
/// not imported - that helper isn't `pub` and lives in a file this mission
/// doesn't touch), then drives `run_council_pass_with` with real
/// `ai::provider_router::generate_for_task` calls. No mocked LLM response
/// ever reaches this function's callers.
pub async fn run_council_pass(
    core: &AgentCoreState,
    app_handle: AppHandle,
    task_id: &str,
    objective: &str,
) -> Result<CouncilPassResult, CouncilError> {
    run_council_pass_with(core, task_id, objective, move |_role, system_prompt, user_prompt| {
        let app_handle = app_handle.clone();
        async move {
            let health = app_handle.state::<HealthRegistry>();
            let providers = {
                let db = app_handle.state::<DbState>();
                let guard = db.conn.lock().map_err(|e| e.to_string())?;
                guard.as_ref().map(provider_registry::load_providers).unwrap_or_default()
                // guard dropped here, before the .await below - same
                // Send-safety constraint agent_v2::generate documents.
            };
            provider_router::generate_for_task(&providers, &health, TaskCapability::Reasoning, &system_prompt, &user_prompt)
                .await
                .map_err(|e| e.to_string())
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_backend_is_queryable_after_recording() {
        let core = AgentCoreState::default();
        record_backend(&core, "task-1", ExecutionBackend::Governed);
        record_backend(&core, "task-2", ExecutionBackend::V2);

        let map = core.task_backends.lock().unwrap();
        assert_eq!(map.get("task-1"), Some(&ExecutionBackend::Governed));
        assert_eq!(map.get("task-2"), Some(&ExecutionBackend::V2));
    }

    #[test]
    fn register_lifecycle_gives_the_task_its_own_advisory_state() {
        let core = AgentCoreState::default();
        register_lifecycle(&core, "task-1");
        assert_eq!(
            core.agent_registry.current_state("task-1", AgentRole::Architect).unwrap(),
            AgentLifecycleState::Created
        );
    }

    // ── Council v1 sequencing (no live model required) ──────────────────

    #[test]
    fn parse_verdict_reads_the_first_word_case_insensitively() {
        assert_eq!(parse_verdict("ACCEPT because it's correct"), CouncilVerdict::Accept);
        assert_eq!(parse_verdict("reject - too risky"), CouncilVerdict::Reject);
        assert_eq!(parse_verdict("Revise: needs more tests"), CouncilVerdict::Revise, "trailing punctuation on the first word must not prevent a match");
        assert_eq!(parse_verdict("REJECT."), CouncilVerdict::Reject);
        assert_eq!(parse_verdict("Hmm, not sure about this one"), CouncilVerdict::Unclear);
        assert_eq!(parse_verdict(""), CouncilVerdict::Unclear);
    }

    #[tokio::test]
    async fn full_pass_calls_each_role_once_in_order_and_each_sees_the_previous_roles_real_output() {
        let core = AgentCoreState::default();
        let calls = std::cell::RefCell::new(Vec::new());

        let result = run_council_pass_with(&core, "task-1", "Add input validation", |role, _system, user_prompt| {
            calls.borrow_mut().push(role);
            let response = match role {
                AgentRole::Architect => Ok("Add a validate() function".to_string()),
                AgentRole::Critic => {
                    assert!(user_prompt.contains("Add a validate() function"), "Critic must see Architect's real output, not a placeholder");
                    Ok("Looks reasonable but needs edge case handling".to_string())
                }
                AgentRole::Judge => {
                    assert!(user_prompt.contains("Add a validate() function"), "Judge must see the Architect's real output");
                    assert!(user_prompt.contains("needs edge case handling"), "Judge must see the Critic's real output");
                    Ok("ACCEPT - solid proposal with a minor caveat noted".to_string())
                }
                AgentRole::Specialist => unreachable!("Specialist is not part of the Council v1 sequential pass"),
            };
            std::future::ready(response)
        })
        .await
        .expect("all three roles succeed");

        assert_eq!(*calls.borrow(), vec![AgentRole::Architect, AgentRole::Critic, AgentRole::Judge], "roles must run in this exact order, once each");
        assert_eq!(result.architect_output, "Add a validate() function");
        assert_eq!(result.critic_output, "Looks reasonable but needs edge case handling");
        assert_eq!(result.judge_verdict, CouncilVerdict::Accept);

        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Completed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Critic).unwrap(), AgentLifecycleState::Completed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Judge).unwrap(), AgentLifecycleState::Completed);
    }

    #[tokio::test]
    async fn architect_failure_halts_before_critic_and_judge_are_ever_called() {
        let core = AgentCoreState::default();
        let calls = std::cell::RefCell::new(Vec::new());

        let result = run_council_pass_with(&core, "task-1", "objective", |role, _s, _u| {
            calls.borrow_mut().push(role);
            std::future::ready(Err::<String, String>("model unavailable".to_string()))
        })
        .await;

        assert_eq!(result, Err(CouncilError { role: AgentRole::Architect, reason: "model unavailable".to_string() }));
        assert_eq!(*calls.borrow(), vec![AgentRole::Architect], "Critic and Judge must never be called after Architect fails");
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Failed);
        assert_eq!(
            core.agent_registry.current_state("task-1", AgentRole::Critic),
            Err(crate::agent_core::service::AgentError::TaskNotFound),
            "Critic must never be registered if Architect failed - no silent partial result"
        );
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Judge), Err(crate::agent_core::service::AgentError::TaskNotFound));
    }

    #[tokio::test]
    async fn critic_failure_halts_before_judge_but_architect_already_succeeded() {
        let core = AgentCoreState::default();
        let calls = std::cell::RefCell::new(Vec::new());

        let result = run_council_pass_with(&core, "task-1", "objective", |role, _s, _u| {
            calls.borrow_mut().push(role);
            let response = match role {
                AgentRole::Architect => Ok("proposal".to_string()),
                AgentRole::Critic => Err("critic model down".to_string()),
                _ => unreachable!("Judge must never be called after Critic fails"),
            };
            std::future::ready(response)
        })
        .await;

        assert_eq!(result, Err(CouncilError { role: AgentRole::Critic, reason: "critic model down".to_string() }));
        assert_eq!(*calls.borrow(), vec![AgentRole::Architect, AgentRole::Critic]);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Completed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Critic).unwrap(), AgentLifecycleState::Failed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Judge), Err(crate::agent_core::service::AgentError::TaskNotFound));
    }

    #[tokio::test]
    async fn judge_failure_reports_judge_as_the_failing_role_with_both_earlier_roles_completed() {
        let core = AgentCoreState::default();

        let result = run_council_pass_with(&core, "task-1", "objective", |role, _s, _u| {
            let response = match role {
                AgentRole::Architect => Ok("proposal".to_string()),
                AgentRole::Critic => Ok("review".to_string()),
                AgentRole::Judge => Err("judge model down".to_string()),
                _ => unreachable!(),
            };
            std::future::ready(response)
        })
        .await;

        assert_eq!(result, Err(CouncilError { role: AgentRole::Judge, reason: "judge model down".to_string() }));
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Completed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Critic).unwrap(), AgentLifecycleState::Completed);
        assert_eq!(core.agent_registry.current_state("task-1", AgentRole::Judge).unwrap(), AgentLifecycleState::Failed);
    }

    /// Genuinely calls `ai::provider_router::generate_for_task` against a
    /// real local Ollama instance - no live `AppHandle`/Tauri app needed,
    /// since `run_council_pass_with` (unlike `run_council_pass`) doesn't
    /// require one; this test constructs the real `HealthRegistry`/provider
    /// list directly, same pattern as every other live-model test this
    /// session. Ignored by default; run explicitly via
    /// `cargo test live_council_pass -- --ignored`.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn live_council_pass_produces_a_real_verdict_from_ollama() {
        let core = AgentCoreState::default();
        let health = HealthRegistry::default();
        let providers = vec![provider_registry::default_ollama_provider()];

        let result = run_council_pass_with(
            &core,
            "live-task-1",
            "Write a one-line Rust function that adds two i32 numbers together.",
            |_role, system_prompt, user_prompt| {
                let health = &health;
                let providers = &providers;
                async move {
                    provider_router::generate_for_task(providers, health, TaskCapability::Reasoning, &system_prompt, &user_prompt)
                        .await
                        .map_err(|e| e.to_string())
                }
            },
        )
        .await;

        let pass = result.expect("a real Architect -> Critic -> Judge pass against local Ollama should succeed");
        assert!(!pass.architect_output.trim().is_empty());
        assert!(!pass.critic_output.trim().is_empty());
        assert!(!pass.judge_output.trim().is_empty());
        assert_ne!(
            pass.judge_verdict,
            CouncilVerdict::Unclear,
            "a real model explicitly instructed to open with ACCEPT/REJECT/REVISE should commit to one"
        );
    }
}
