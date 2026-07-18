//! Events that drive AgentCore's advisory lifecycle view (see
//! `lifecycle::AgentLifecycleState`'s doc comment for what "advisory"
//! means here). Deliberately scoped to only the transitions
//! `reducer::reduce` actually implements - not a speculative full event
//! catalog for either backend's real state machine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEventType {
    PlanningStarted,
    ApprovalRequested,
    ApprovalGranted,
    ExecutionStarted,
    VerificationStarted,
    Completed,
    Failed,
    Cancelled,
}

/// Identity for a named agent within a task's advisory lifecycle view - the
/// key `registry::AgentRegistry` uses alongside a task id so one task can
/// track multiple concurrent role lifecycles (e.g. a future Council pass)
/// instead of one shared state machine per task. Deliberately NOT
/// `multi_agent::AgentRole` (Supervisor/Research/Coding/Testing/Review):
/// that enum is unregistered, unreferenced dead code with different role
/// names than the roadmap's Architect/Critic/Specialist/Judge council, and
/// `agent_core` (actively developed) has no reason to depend on a fully
/// dead module for a type it needs live today. `Architect` is also the
/// default role for today's single-agent call sites (`agent_core::
/// orchestrator::register_lifecycle`, `commands::agent_lifecycle_transition`)
/// until a real multi-role Council pass registers other roles too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    Architect,
    Critic,
    Specialist,
    Judge,
}
