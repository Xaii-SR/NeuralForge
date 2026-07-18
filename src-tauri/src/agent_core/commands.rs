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
//! `agent_lifecycle_transition` (Phase 6B Phase 2) is the sole exception:
//! it is registered in `lib.rs`, narrowly scoped to `AgentCoreState::
//! agent_registry`'s per-task advisory lifecycle view only. It does not
//! touch `agent::*`/`agent_v2::*` execution and has no frontend caller yet.

use crate::agent_core::orchestrator;
use crate::agent_core::service::AgentError;
use crate::agent_core::types::AgentEventType;
use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::AgentCoreState;
use crate::agent_v2::ApprovalRegistry;
use crate::core::errors::AppResult;
use crate::core::state::AppState;
use crate::database::DbState;
use tauri::{AppHandle, State};

/// Advances the named task's advisory lifecycle view by one event. Purely
/// in-memory bookkeeping via `core.agent_registry` - does not touch
/// `agent::` or `agent_v2::` execution, persistence, or approval state.
/// Fails if `task_id` was never registered (see `orchestrator::
/// register_lifecycle`, called from task creation).
#[tauri::command]
pub fn agent_lifecycle_transition(
    core: State<'_, AgentCoreState>,
    task_id: String,
    event: AgentEventType,
) -> Result<AgentLifecycleState, String> {
    core.agent_registry.transition(&task_id, event).map_err(|e: AgentError| format!("{e:?}"))
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
