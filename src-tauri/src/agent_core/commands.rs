//! Future Tauri command bodies for AgentCore.
//!
//! Deliberately NOT `#[tauri::command]`-annotated and NOT registered in
//! `lib.rs`'s `generate_handler!` yet. Phase 6A's only sanctioned `lib.rs`
//! change is the `mod agent_core;` declaration - adding these to the
//! invoke handler is a real, reviewable change to the app's IPC surface
//! (even though additive, not breaking) and belongs in a later phase after
//! this shell's integration diff has been reviewed, per the stated stop
//! condition. Until then, these are plain, real, callable, tested
//! functions - not placeholders - that simply have no `invoke()` entry
//! point wired to them yet. `AgentPanel.tsx` continues to call
//! `agent::*`/`agent_v2::*` commands directly and is completely unaffected
//! by this file's existence.

use crate::agent_core::orchestrator;
use crate::agent_core::AgentCoreState;
use crate::agent_v2::ApprovalRegistry;
use crate::core::errors::AppResult;
use crate::core::state::AppState;
use crate::database::DbState;
use tauri::{AppHandle, State};

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
