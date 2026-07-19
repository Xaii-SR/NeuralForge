//! Tauri command bodies for AgentCore.
//!
//! Most functions here are deliberately NOT `#[tauri::command]`-annotated
//! and NOT registered in `lib.rs`'s `generate_handler!` yet - adding them
//! to the invoke handler is a real, reviewable change to the app's IPC
//! surface and belongs in a later phase after this shell's integration
//! diff has been reviewed, per the stated stop condition. Until then,
//! they are plain, real, callable, tested functions - not placeholders -
//! that simply have no `invoke()` entry point wired to them yet.
//! `AgentPanel.tsx` continues to call `agent::*`/`agent_v2::*` commands
//! directly and is unaffected by this file's existence.
//!
//! `agent_lifecycle_transition` (Phase 6B Phase 2) and `run_council_pass`
//! (Council v1) are the two exceptions: both are registered in `lib.rs`.
//! `agent_lifecycle_transition` is narrowly scoped to `AgentCoreState::
//! agent_registry`'s per-`(task, role)` advisory lifecycle view only and
//! does not touch `agent::*`/`agent_v2::*` execution. `run_council_pass`
//! genuinely calls `ai::provider_router::generate_for_task` (see
//! `orchestrator::run_council_pass`'s doc comment) but still does not touch
//! `agent::*`/`agent_v2::*` execution, persistence, or approval state.
//! Neither has a frontend caller yet.

use crate::agent_core::orchestrator;
use crate::agent_core::service::AgentError;
use crate::agent_core::types::{AgentEventType, AgentRole, CouncilError, CouncilPassResult};
use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::AgentCoreState;
use crate::agent_v2::ApprovalRegistry;
use crate::core::errors::AppResult;
use crate::core::state::AppState;
use crate::database::DbState;
use tauri::{AppHandle, State};

/// Advances the named `(task_id, role)` pair's advisory lifecycle view by
/// one event. Purely in-memory bookkeeping via `core.agent_registry` - does
/// not touch `agent::` or `agent_v2::` execution, persistence, or approval
/// state. Fails if that exact `(task_id, role)` pair was never registered
/// (see `orchestrator::register_lifecycle`, called from task creation with
/// `AgentRole::Architect` today).
#[tauri::command]
pub fn agent_lifecycle_transition(
    core: State<'_, AgentCoreState>,
    task_id: String,
    role: AgentRole,
    event: AgentEventType,
) -> Result<AgentLifecycleState, String> {
    core.agent_registry.transition(&task_id, role, event).map_err(|e: AgentError| format!("{e:?}"))
}

/// Runs one real, sequential Architect -> Critic -> Judge Council v1 pass
/// against `objective`, under `task_id` (caller-supplied - AgentCore does
/// not own or require a `task_orchestrator::OrchestratorTask`; pass its
/// `.id` if one exists, or any fresh id). See `orchestrator::
/// run_council_pass`/`run_council_pass_with` for the real sequencing and
/// failure-handling contract. No frontend caller yet - proving the backend
/// pass works is this mission's whole scope.
#[tauri::command]
pub async fn run_council_pass(
    core: State<'_, AgentCoreState>,
    app_handle: AppHandle,
    task_id: String,
    objective: String,
) -> Result<CouncilPassResult, String> {
    orchestrator::run_council_pass(&core, app_handle, &task_id, &objective)
        .await
        .map_err(|e: CouncilError| e.to_string())
}

pub async fn create_and_plan_task(
    core: State<'_, AgentCoreState>,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    requirement_id: String,
    file_path: String,
) -> AppResult<crate::agent::AgentTask> {
    orchestrator::create_and_plan_task(&core, state, db, requirement_id, file_path).await
}

pub async fn create_and_plan_code_task(
    core: State<'_, AgentCoreState>,
    db: State<'_, DbState>,
    objective: String,
) -> AppResult<crate::agent::AgentTask> {
    orchestrator::create_and_plan_code_task(&core, db, objective).await
}

pub async fn approve_task(state: State<'_, AppState>, db: State<'_, DbState>, task_id: String) -> AppResult<crate::agent::AgentTask> {
    orchestrator::approve_task(state, db, task_id).await
}

pub fn reject_task(db: State<'_, DbState>, task_id: String) -> AppResult<()> {
    orchestrator::reject_task(db, task_id)
}

pub fn list_agent_tasks(db: State<'_, DbState>) -> AppResult<Vec<crate::agent::AgentTask>> {
    orchestrator::list_agent_tasks(db)
}

pub async fn start_v2_task(
    core: State<'_, AgentCoreState>,
    app_handle: AppHandle,
    registry: State<'_, ApprovalRegistry>,
    description: String,
) -> Result<String, String> {
    orchestrator::start_v2_task(&core, app_handle, registry, description).await
}

pub async fn approve_v2_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    orchestrator::approve_v2_task(id, registry).await
}

pub async fn reject_v2_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    orchestrator::reject_v2_task(id, registry).await
}
