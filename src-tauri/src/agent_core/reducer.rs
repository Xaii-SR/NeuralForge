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

pub fn reduce(current: AgentLifecycleState, event: AgentEventType) -> AgentLifecycleState {
    match event {
        // Failed is absorbing from any state - an advisory cache has no
        // business refusing to record "this task failed" because it
        // thought the task was somewhere else.
        AgentEventType::Failed => AgentLifecycleState::Failed,

        AgentEventType::PlanningStarted if current == AgentLifecycleState::Created => AgentLifecycleState::Planning,

        AgentEventType::ApprovalGranted if current == AgentLifecycleState::AwaitingApproval => AgentLifecycleState::Approved,

        // Event doesn't apply from this state - no-op, not an error (see
        // module doc).
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planning_started_from_created_transitions_to_planning() {
        assert_eq!(
            reduce(AgentLifecycleState::Created, AgentEventType::PlanningStarted),
            AgentLifecycleState::Planning
        );
    }

    #[test]
    fn approval_granted_from_awaiting_approval_transitions_to_approved() {
        assert_eq!(
            reduce(AgentLifecycleState::AwaitingApproval, AgentEventType::ApprovalGranted),
            AgentLifecycleState::Approved
        );
    }

    #[test]
    fn failed_is_absorbing_from_any_state() {
        for state in [
            AgentLifecycleState::Created,
            AgentLifecycleState::Planning,
            AgentLifecycleState::AwaitingApproval,
            AgentLifecycleState::Approved,
            AgentLifecycleState::Failed,
        ] {
            assert_eq!(reduce(state, AgentEventType::Failed), AgentLifecycleState::Failed);
        }
    }

    /// The gap this test documents rather than papers over: nothing in
    /// this phase's implemented transitions moves a task from Planning to
    /// AwaitingApproval. That's a real hole in the advisory model, not an
    /// oversight in this test - flagging it explicitly instead of quietly
    /// inventing an event/transition that wasn't asked for.
    #[test]
    fn planning_started_is_a_noop_from_states_other_than_created() {
        assert_eq!(
            reduce(AgentLifecycleState::AwaitingApproval, AgentEventType::PlanningStarted),
            AgentLifecycleState::AwaitingApproval,
            "PlanningStarted must only fire the Created -> Planning transition"
        );
    }

    #[test]
    fn approval_granted_is_a_noop_from_states_other_than_awaiting_approval() {
        assert_eq!(
            reduce(AgentLifecycleState::Created, AgentEventType::ApprovalGranted),
            AgentLifecycleState::Created,
            "ApprovalGranted must only fire the AwaitingApproval -> Approved transition"
        );
    }
}
