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

/// The Judge's final call on a Council v1 pass, parsed from the first word
/// of its real model output (see `orchestrator::parse_verdict`). `Unclear`
/// is a real, valid outcome - not an error - for when the model doesn't
/// commit to one of the three explicitly requested words; the pass still
/// succeeded and returned real output, it's just not a clean verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CouncilVerdict {
    Accept,
    Reject,
    Revise,
    Unclear,
}

/// The real, sequential Architect -> Critic -> Judge output from one
/// Council v1 pass (see `orchestrator::run_council_pass`). Every field is
/// genuine model output, never a mocked/placeholder string in the real
/// (non-test) path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CouncilPassResult {
    pub architect_output: String,
    pub critic_output: String,
    pub judge_output: String,
    pub judge_verdict: CouncilVerdict,
}

/// Reports exactly which role's real LLM call failed and why - the
/// "real failure handling" contract for `run_council_pass`: a role's
/// failure halts the pass immediately (later roles are never called, never
/// registered), never silently continues with a partial/placeholder result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CouncilError {
    pub role: AgentRole,
    pub reason: String,
}

impl std::fmt::Display for CouncilError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} failed: {}", self.role, self.reason)
    }
}
