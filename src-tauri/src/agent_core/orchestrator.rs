//! AgentCore's coordination layer: forwards requests to the existing
//! execution authorities (`agent::`, `agent_v2`) and records which backend
//! handled each task. Owns no execution logic of its own - every function
//! here is a direct call into an existing public function, per the "No God
//! Object" rule. Nothing in this file touches `provider_router`,
//! `std::fs`, rollback, or verification directly; those remain entirely
//! inside the execution authorities being forwarded to.

use crate::agent_core::lifecycle::{AgentLifecycleState, ExecutionBackend};
use crate::agent_core::types::AgentRole;
use crate::agent_core::AgentCoreState;
use crate::agent_v2::ApprovalRegistry;
use crate::core::errors::AppResult;
use crate::core::state::AppState;
use crate::database::DbState;
use tauri::{AppHandle, State};

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
}
