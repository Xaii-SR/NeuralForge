//! Pure state transition function for AgentCore's advisory lifecycle view.
//! No async, no I/O, no DB access - by construction, not just convention:
//! nothing in this file's signature could reach outside this process even
//! if someone tried to add a side effect later without also changing the
//! signature. Unknown-transition inputs (e.g. `ApprovalGranted` while not
//! `AwaitingApproval`) are a documented no-op, not a panic - callers get an
//! unchanged state back rather than a synthetic error, since misordered
//! events reaching an advisory cache are expected under real concurrency
//! (the authoritative backend may have already moved on) and must never
//! crash the coordination layer over it.

use crate::agent_core::lifecycle::AgentLifecycleState;
use crate::agent_core::types::AgentEventType;

pub fn reduce(state: &AgentLifecycleState, event: &AgentEventType) -> AgentLifecycleState {
    match event {
        // Failed and Cancelled are absorbing from any state - an advisory
        // cache has no business refusing to record a terminal outcome
        // because it thought the task was somewhere else.
        AgentEventType::Failed => AgentLifecycleState::Failed,
        AgentEventType::Cancelled => AgentLifecycleState::Cancelled,

        AgentEventType::PlanningStarted if *state == AgentLifecycleState::Created => AgentLifecycleState::Planning,

        AgentEventType::ApprovalRequested if *state == AgentLifecycleState::Planning => AgentLifecycleState::AwaitingApproval,

        AgentEventType::ApprovalGranted if *state == AgentLifecycleState::AwaitingApproval => AgentLifecycleState::Approved,

        AgentEventType::ExecutionStarted if *state == AgentLifecycleState::Approved => AgentLifecycleState::Executing,

        AgentEventType::VerificationStarted if *state == AgentLifecycleState::Executing => AgentLifecycleState::Verifying,

        AgentEventType::Completed if *state == AgentLifecycleState::Verifying => AgentLifecycleState::Completed,

        // Event doesn't apply from this state - no-op, not an error (see
        // module doc).
        _ => *state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planning_started_from_created_transitions_to_planning() {
        assert_eq!(
            reduce(&AgentLifecycleState::Created, &AgentEventType::PlanningStarted),
            AgentLifecycleState::Planning
        );
    }

    #[test]
    fn approval_requested_from_planning_transitions_to_awaiting_approval() {
        assert_eq!(
            reduce(&AgentLifecycleState::Planning, &AgentEventType::ApprovalRequested),
            AgentLifecycleState::AwaitingApproval
        );
    }

    #[test]
    fn approval_granted_from_awaiting_approval_transitions_to_approved() {
        assert_eq!(
            reduce(&AgentLifecycleState::AwaitingApproval, &AgentEventType::ApprovalGranted),
            AgentLifecycleState::Approved
        );
    }

    #[test]
    fn execution_started_from_approved_transitions_to_executing() {
        assert_eq!(
            reduce(&AgentLifecycleState::Approved, &AgentEventType::ExecutionStarted),
            AgentLifecycleState::Executing
        );
    }

    #[test]
    fn verification_started_from_executing_transitions_to_verifying() {
        assert_eq!(
            reduce(&AgentLifecycleState::Executing, &AgentEventType::VerificationStarted),
            AgentLifecycleState::Verifying
        );
    }

    #[test]
    fn completed_from_verifying_transitions_to_completed() {
        assert_eq!(
            reduce(&AgentLifecycleState::Verifying, &AgentEventType::Completed),
            AgentLifecycleState::Completed
        );
    }

    #[test]
    fn failed_is_absorbing_from_any_state() {
        for state in [
            AgentLifecycleState::Created,
            AgentLifecycleState::Planning,
            AgentLifecycleState::AwaitingApproval,
            AgentLifecycleState::Approved,
            AgentLifecycleState::Executing,
            AgentLifecycleState::Verifying,
            AgentLifecycleState::Completed,
            AgentLifecycleState::Failed,
            AgentLifecycleState::Cancelled,
        ] {
            assert_eq!(reduce(&state, &AgentEventType::Failed), AgentLifecycleState::Failed);
        }
    }

    #[test]
    fn cancelled_is_absorbing_from_any_state() {
        for state in [
            AgentLifecycleState::Created,
            AgentLifecycleState::Planning,
            AgentLifecycleState::AwaitingApproval,
            AgentLifecycleState::Approved,
            AgentLifecycleState::Executing,
            AgentLifecycleState::Verifying,
            AgentLifecycleState::Completed,
            AgentLifecycleState::Failed,
            AgentLifecycleState::Cancelled,
        ] {
            assert_eq!(reduce(&state, &AgentEventType::Cancelled), AgentLifecycleState::Cancelled);
        }
    }

    #[test]
    fn planning_started_is_a_noop_from_states_other_than_created() {
        assert_eq!(
            reduce(&AgentLifecycleState::AwaitingApproval, &AgentEventType::PlanningStarted),
            AgentLifecycleState::AwaitingApproval,
            "PlanningStarted must only fire the Created -> Planning transition"
        );
    }

    #[test]
    fn approval_granted_is_a_noop_from_states_other_than_awaiting_approval() {
        assert_eq!(
            reduce(&AgentLifecycleState::Created, &AgentEventType::ApprovalGranted),
            AgentLifecycleState::Created,
            "ApprovalGranted must only fire the AwaitingApproval -> Approved transition"
        );
    }

    #[test]
    fn created_plus_approval_granted_is_a_noop() {
        assert_eq!(
            reduce(&AgentLifecycleState::Created, &AgentEventType::ApprovalGranted),
            AgentLifecycleState::Created
        );
    }

    #[test]
    fn planning_plus_execution_started_is_a_noop() {
        assert_eq!(
            reduce(&AgentLifecycleState::Planning, &AgentEventType::ExecutionStarted),
            AgentLifecycleState::Planning
        );
    }

    #[test]
    fn approved_plus_planning_started_is_a_noop() {
        assert_eq!(
            reduce(&AgentLifecycleState::Approved, &AgentEventType::PlanningStarted),
            AgentLifecycleState::Approved
        );
    }

    #[test]
    fn completed_plus_planning_started_is_a_noop() {
        assert_eq!(
            reduce(&AgentLifecycleState::Completed, &AgentEventType::PlanningStarted),
            AgentLifecycleState::Completed
        );
    }
}
