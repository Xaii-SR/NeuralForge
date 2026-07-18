//! Per-task isolation for AgentCore's advisory lifecycle view.
//!
//! `AgentService` alone is a single shared state machine - fine for one
//! task, but two tasks transitioning concurrently through one `AgentService`
//! would interleave their events into a single, meaningless state. This
//! registry gives each task id its own `AgentService`, keyed by task id,
//! so tasks cannot observe or corrupt each other's advisory state.
//!
//! Still headless: no `tauri`, no `tokio::spawn`, no event emitters. Still
//! advisory-only (see `service.rs`'s doc comment) - callers needing the
//! real state go to `agent::get_task`/`agent_v2::AgentState` as before.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::service::{AgentError, AgentService};
use crate::agent_core::types::AgentEventType;

#[derive(Default)]
pub struct AgentRegistry {
    services: RwLock<HashMap<String, AgentService>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `task_id` with its own `AgentService` starting at
    /// `initial_state`. If `task_id` is already registered, this is a
    /// no-op that returns the existing state rather than resetting it -
    /// registration is idempotent, not a reset.
    pub fn register(&self, task_id: String, initial_state: AgentLifecycleState) -> Result<AgentLifecycleState, AgentError> {
        let mut services = self.services.write().map_err(|_| AgentError::LockPoisoned)?;
        if let Some(existing) = services.get(&task_id) {
            return existing.current_state();
        }
        services.insert(task_id, AgentService::new(initial_state));
        Ok(initial_state)
    }

    /// Advances the named task's lifecycle by one event. Fails with
    /// `AgentError::TaskNotFound` if `task_id` was never registered -
    /// callers must `register` before transitioning.
    pub fn transition(&self, task_id: &str, event: AgentEventType) -> Result<AgentLifecycleState, AgentError> {
        let services = self.services.read().map_err(|_| AgentError::LockPoisoned)?;
        let service = services.get(task_id).ok_or(AgentError::TaskNotFound)?;
        service.transition(event)
    }

    pub fn current_state(&self, task_id: &str) -> Result<AgentLifecycleState, AgentError> {
        let services = self.services.read().map_err(|_| AgentError::LockPoisoned)?;
        let service = services.get(task_id).ok_or(AgentError::TaskNotFound)?;
        service.current_state()
    }

    /// Evicts `task_id`'s `AgentService`, e.g. after its internal lock has
    /// been poisoned by a panic mid-`transition`. A poisoned `AgentService`
    /// cannot be un-poisoned in place; the only recovery is to drop it and
    /// let a future `register` create a fresh one, which resets that task's
    /// advisory view to whatever `initial_state` the caller passes next -
    /// callers should treat that as a real gap in this task's history, not
    /// a seamless recovery. Only recovers this registry's own `services`
    /// lock is unaffected; if that outer lock is itself poisoned, this call
    /// fails with `LockPoisoned` too, same as every other method here.
    pub fn recover_task(&self, task_id: &str) -> Result<(), AgentError> {
        let mut services = self.services.write().map_err(|_| AgentError::LockPoisoned)?;
        if services.remove(task_id).is_some() {
            Ok(())
        } else {
            Err(AgentError::TaskNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_starts_a_new_task_at_the_given_initial_state() {
        let registry = AgentRegistry::new();
        let result = registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();
        assert_eq!(result, AgentLifecycleState::Created);
        assert_eq!(registry.current_state("task-1").unwrap(), AgentLifecycleState::Created);
    }

    #[test]
    fn register_is_idempotent_and_does_not_reset_an_existing_task() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();
        registry.transition("task-1", AgentEventType::PlanningStarted).unwrap();

        let result = registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();

        assert_eq!(result, AgentLifecycleState::Planning, "re-registering must not reset in-progress state");
    }

    #[test]
    fn transition_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.transition("missing", AgentEventType::PlanningStarted), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn current_state_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.current_state("missing"), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn recover_task_evicts_a_registered_task() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();

        registry.recover_task("task-1").unwrap();

        assert_eq!(registry.current_state("task-1"), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn recover_task_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.recover_task("missing"), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn re_registering_after_recover_task_starts_fresh() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();
        registry.transition("task-1", AgentEventType::PlanningStarted).unwrap();

        registry.recover_task("task-1").unwrap();
        let result = registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();

        assert_eq!(result, AgentLifecycleState::Created, "recovered task must not retain pre-recovery state");
    }

    #[test]
    fn two_tasks_transition_independently_without_interfering() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentLifecycleState::Created).unwrap();
        registry.register("task-2".to_string(), AgentLifecycleState::Created).unwrap();

        registry.transition("task-1", AgentEventType::PlanningStarted).unwrap();

        assert_eq!(registry.current_state("task-1").unwrap(), AgentLifecycleState::Planning);
        assert_eq!(
            registry.current_state("task-2").unwrap(),
            AgentLifecycleState::Created,
            "task-2 must be unaffected by task-1's transition"
        );
    }
}
