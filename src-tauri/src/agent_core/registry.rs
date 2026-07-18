//! Per-(task, role) isolation for AgentCore's advisory lifecycle view.
//!
//! `AgentService` alone is a single shared state machine - fine for one
//! agent, but two agents transitioning concurrently through one
//! `AgentService` would interleave their events into a single, meaningless
//! state. This registry gives each `(task_id, role)` pair its own
//! `AgentService`, nested by task id then role, so agents cannot observe or
//! corrupt each other's advisory state - including two different-role
//! agents working the same task (the Council case: one task, several
//! concurrently active roles).
//!
//! Still headless: no `tauri`, no `tokio::spawn`, no event emitters. Still
//! advisory-only (see `service.rs`'s doc comment) - callers needing the
//! real state go to `agent::get_task`/`agent_v2::AgentState` as before.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::service::{AgentError, AgentService};
use crate::agent_core::types::{AgentEventType, AgentRole};

#[derive(Default)]
pub struct AgentRegistry {
    services: RwLock<HashMap<String, HashMap<AgentRole, AgentService>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `role` under `task_id` with its own `AgentService` starting
    /// at `initial_state`. If that exact `(task_id, role)` pair is already
    /// registered, this is a no-op that returns the existing state rather
    /// than resetting it - registration is idempotent, not a reset. A
    /// different `role` under the same `task_id` is a distinct entry, not a
    /// collision.
    pub fn register(&self, task_id: String, role: AgentRole, initial_state: AgentLifecycleState) -> Result<AgentLifecycleState, AgentError> {
        let mut services = self.services.write().map_err(|_| AgentError::LockPoisoned)?;
        let roles = services.entry(task_id).or_default();
        if let Some(existing) = roles.get(&role) {
            return existing.current_state();
        }
        roles.insert(role, AgentService::new(initial_state));
        Ok(initial_state)
    }

    /// Advances `role`'s lifecycle under `task_id` by one event. Fails with
    /// `AgentError::TaskNotFound` if that exact `(task_id, role)` pair was
    /// never registered - callers must `register` before transitioning.
    /// Never affects any other role registered under the same `task_id`.
    pub fn transition(&self, task_id: &str, role: AgentRole, event: AgentEventType) -> Result<AgentLifecycleState, AgentError> {
        let services = self.services.read().map_err(|_| AgentError::LockPoisoned)?;
        let service = services
            .get(task_id)
            .and_then(|roles| roles.get(&role))
            .ok_or(AgentError::TaskNotFound)?;
        service.transition(event)
    }

    pub fn current_state(&self, task_id: &str, role: AgentRole) -> Result<AgentLifecycleState, AgentError> {
        let services = self.services.read().map_err(|_| AgentError::LockPoisoned)?;
        let service = services
            .get(task_id)
            .and_then(|roles| roles.get(&role))
            .ok_or(AgentError::TaskNotFound)?;
        service.current_state()
    }

    /// Evicts `role`'s `AgentService` under `task_id`, e.g. after its
    /// internal lock has been poisoned by a panic mid-`transition`. A
    /// poisoned `AgentService` cannot be un-poisoned in place; the only
    /// recovery is to drop it and let a future `register` create a fresh
    /// one for that same `(task_id, role)` pair, which resets that agent's
    /// advisory view to whatever `initial_state` the caller passes next -
    /// callers should treat that as a real gap in this agent's history, not
    /// a seamless recovery. Only evicts the named role; other roles under
    /// the same `task_id` are untouched. If `task_id` has no entries left
    /// after eviction, the outer entry is removed too, matching the
    /// pre-role-keyed behavior of never leaking empty task entries. Only
    /// this registry's own `services` lock is at risk here; if that outer
    /// lock is itself poisoned, this call fails with `LockPoisoned` too,
    /// same as every other method here.
    pub fn recover_task(&self, task_id: &str, role: AgentRole) -> Result<(), AgentError> {
        let mut services = self.services.write().map_err(|_| AgentError::LockPoisoned)?;
        let Some(roles) = services.get_mut(task_id) else {
            return Err(AgentError::TaskNotFound);
        };
        if roles.remove(&role).is_none() {
            return Err(AgentError::TaskNotFound);
        }
        if roles.is_empty() {
            services.remove(task_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_starts_a_new_task_at_the_given_initial_state() {
        let registry = AgentRegistry::new();
        let result = registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        assert_eq!(result, AgentLifecycleState::Created);
        assert_eq!(registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Created);
    }

    #[test]
    fn register_is_idempotent_and_does_not_reset_an_existing_task() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.transition("task-1", AgentRole::Architect, AgentEventType::PlanningStarted).unwrap();

        let result = registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        assert_eq!(result, AgentLifecycleState::Planning, "re-registering must not reset in-progress state");
    }

    #[test]
    fn transition_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(
            registry.transition("missing", AgentRole::Architect, AgentEventType::PlanningStarted),
            Err(AgentError::TaskNotFound)
        );
    }

    #[test]
    fn current_state_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.current_state("missing", AgentRole::Architect), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn recover_task_evicts_a_registered_task() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        registry.recover_task("task-1", AgentRole::Architect).unwrap();

        assert_eq!(registry.current_state("task-1", AgentRole::Architect), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn recover_task_on_an_unregistered_task_returns_task_not_found() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.recover_task("missing", AgentRole::Architect), Err(AgentError::TaskNotFound));
    }

    #[test]
    fn re_registering_after_recover_task_starts_fresh() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.transition("task-1", AgentRole::Architect, AgentEventType::PlanningStarted).unwrap();

        registry.recover_task("task-1", AgentRole::Architect).unwrap();
        let result = registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        assert_eq!(result, AgentLifecycleState::Created, "recovered task must not retain pre-recovery state");
    }

    #[test]
    fn two_tasks_transition_independently_without_interfering() {
        let registry = AgentRegistry::new();
        registry.register("task-1".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.register("task-2".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        registry.transition("task-1", AgentRole::Architect, AgentEventType::PlanningStarted).unwrap();

        assert_eq!(registry.current_state("task-1", AgentRole::Architect).unwrap(), AgentLifecycleState::Planning);
        assert_eq!(
            registry.current_state("task-2", AgentRole::Architect).unwrap(),
            AgentLifecycleState::Created,
            "task-2 must be unaffected by task-1's transition"
        );
    }

    /// The Council case: one task, multiple concurrently active roles.
    /// Registering Critic under a task_id that already has an Architect
    /// entry must not disturb the Architect's state, and each role
    /// transitions independently.
    #[test]
    fn registry_can_hold_multiple_roles_under_the_same_task_id_independently() {
        let registry = AgentRegistry::new();
        registry.register("council-task".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.register("council-task".to_string(), AgentRole::Critic, AgentLifecycleState::Created).unwrap();
        registry.register("council-task".to_string(), AgentRole::Judge, AgentLifecycleState::Created).unwrap();

        registry.transition("council-task", AgentRole::Architect, AgentEventType::PlanningStarted).unwrap();

        assert_eq!(registry.current_state("council-task", AgentRole::Architect).unwrap(), AgentLifecycleState::Planning);
        assert_eq!(
            registry.current_state("council-task", AgentRole::Critic).unwrap(),
            AgentLifecycleState::Created,
            "Critic must be unaffected by Architect's transition under the same task_id"
        );
        assert_eq!(
            registry.current_state("council-task", AgentRole::Judge).unwrap(),
            AgentLifecycleState::Created,
            "Judge must be unaffected by Architect's transition under the same task_id"
        );
    }

    /// transition/recover_task must key on the exact (task_id, role) pair,
    /// not cross-contaminate a different role under the same task_id or the
    /// same role under a different task_id.
    #[test]
    fn transition_and_recover_task_operate_on_the_correct_task_role_pair_only() {
        let registry = AgentRegistry::new();
        registry.register("task-a".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.register("task-a".to_string(), AgentRole::Critic, AgentLifecycleState::Created).unwrap();
        registry.register("task-b".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        registry.recover_task("task-a", AgentRole::Critic).unwrap();

        // Evicted pair is gone.
        assert_eq!(registry.current_state("task-a", AgentRole::Critic), Err(AgentError::TaskNotFound));
        // Same task_id, different role: untouched.
        assert_eq!(registry.current_state("task-a", AgentRole::Architect).unwrap(), AgentLifecycleState::Created);
        // Same role, different task_id: untouched.
        assert_eq!(registry.current_state("task-b", AgentRole::Architect).unwrap(), AgentLifecycleState::Created);
    }

    /// Evicting the last role under a task_id must not leak an empty inner
    /// map entry, and must not affect other task_ids.
    #[test]
    fn recover_task_removes_the_outer_task_entry_once_its_last_role_is_evicted() {
        let registry = AgentRegistry::new();
        registry.register("task-a".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();
        registry.register("task-b".to_string(), AgentRole::Architect, AgentLifecycleState::Created).unwrap();

        registry.recover_task("task-a", AgentRole::Architect).unwrap();

        assert_eq!(registry.recover_task("task-a", AgentRole::Architect), Err(AgentError::TaskNotFound));
        assert_eq!(registry.current_state("task-b", AgentRole::Architect).unwrap(), AgentLifecycleState::Created);
    }
}
