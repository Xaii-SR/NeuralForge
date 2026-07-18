//! Thread-safe wrapper around `reducer::reduce` for AgentCore's advisory
//! lifecycle view. Headless by construction: no `tauri`, no `tokio::spawn`,
//! no event emitters. Owns only the in-memory `AgentLifecycleState`; still
//! advisory, not authoritative (see `lifecycle::AgentLifecycleState`'s doc
//! comment) - callers needing the real state go to `agent::get_task`/
//! `agent_v2::AgentState` as before.

use std::sync::RwLock;

use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::reducer;
use crate::agent_core::types::AgentEventType;

#[derive(Debug, PartialEq, Eq)]
pub enum AgentError {
    LockPoisoned,
    TaskNotFound,
}

pub struct AgentService {
    state: RwLock<AgentLifecycleState>,
}

impl AgentService {
    pub fn new(initial_state: AgentLifecycleState) -> Self {
        Self {
            state: RwLock::new(initial_state),
        }
    }

    pub fn transition(&self, event: AgentEventType) -> Result<AgentLifecycleState, AgentError> {
        let mut state = self.state.write().map_err(|_| AgentError::LockPoisoned)?;
        let next = reducer::reduce(&state, &event);
        *state = next;
        Ok(next)
    }

    pub fn current_state(&self) -> Result<AgentLifecycleState, AgentError> {
        let state = self.state.read().map_err(|_| AgentError::LockPoisoned)?;
        Ok(*state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_the_given_initial_state() {
        let service = AgentService::new(AgentLifecycleState::Created);
        assert_eq!(service.current_state().unwrap(), AgentLifecycleState::Created);
    }

    #[test]
    fn transition_applies_the_reducer_and_updates_internal_state() {
        let service = AgentService::new(AgentLifecycleState::Created);

        let result = service.transition(AgentEventType::PlanningStarted).unwrap();

        assert_eq!(result, AgentLifecycleState::Planning);
        assert_eq!(service.current_state().unwrap(), AgentLifecycleState::Planning);
    }

    #[test]
    fn transition_is_a_noop_when_the_event_does_not_apply_from_the_current_state() {
        let service = AgentService::new(AgentLifecycleState::Created);

        let result = service.transition(AgentEventType::ApprovalGranted).unwrap();

        assert_eq!(result, AgentLifecycleState::Created);
        assert_eq!(service.current_state().unwrap(), AgentLifecycleState::Created);
    }

    #[test]
    fn sequential_transitions_walk_the_full_happy_path() {
        let service = AgentService::new(AgentLifecycleState::Created);

        assert_eq!(service.transition(AgentEventType::PlanningStarted).unwrap(), AgentLifecycleState::Planning);
        assert_eq!(service.transition(AgentEventType::ApprovalRequested).unwrap(), AgentLifecycleState::AwaitingApproval);
        assert_eq!(service.transition(AgentEventType::ApprovalGranted).unwrap(), AgentLifecycleState::Approved);
        assert_eq!(service.transition(AgentEventType::ExecutionStarted).unwrap(), AgentLifecycleState::Executing);
        assert_eq!(service.transition(AgentEventType::VerificationStarted).unwrap(), AgentLifecycleState::Verifying);
        assert_eq!(service.transition(AgentEventType::Completed).unwrap(), AgentLifecycleState::Completed);
    }

    #[test]
    fn failed_overrides_from_any_state() {
        let service = AgentService::new(AgentLifecycleState::Executing);
        assert_eq!(service.transition(AgentEventType::Failed).unwrap(), AgentLifecycleState::Failed);
    }
}
