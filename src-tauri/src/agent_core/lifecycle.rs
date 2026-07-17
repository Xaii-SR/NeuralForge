//! Execution backend selection - the one lifecycle concept AgentCore owns
//! at this stage.
//!
//! This is deliberately NOT a unified task-lifecycle model. `agent::status`
//! (DB-backed string constants), `agent_v2::AgentState` (in-memory enum),
//! and `task_orchestrator::TaskLifecycle` (in-memory enum, currently on a
//! dead execution path) remain three separate representations, each owned
//! by its existing module. Merging them is an explicit non-goal of this
//! phase - see the Phase 6 audit's Migration Boundary Definition ("MOVE:
//! task lifecycle state machine... unify"), which is future work, not this
//! scaffold.
//!
//! What AgentCore genuinely needs to coordinate today is narrower: given a
//! request, which existing execution authority handles it. That's the only
//! thing this module models.

use serde::{Deserialize, Serialize};

/// Which existing execution authority owns a given task. AgentCore routes
/// to one of these; it does not itself execute anything (see this crate's
/// "No God Object" rule - `agent_core` coordinates, `agent` and `agent_v2`
/// remain the execution authorities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionBackend {
    /// The governed, requirement-gated, ledger-integrated pipeline in
    /// `agent::` (`agent/mod.rs`, `agent/executor.rs`, `agent/planner.rs`).
    /// Frozen per `.clinerules` - AgentCore may only call its public
    /// commands, never modify or inline its logic.
    Governed,
    /// The HITL retry-loop pipeline in `agent_v2.rs`, migrated onto
    /// `ai::provider_router` in Phase 2.
    V2,
}

impl ExecutionBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionBackend::Governed => "governed",
            ExecutionBackend::V2 => "v2",
        }
    }
}

/// An advisory, coordination-layer view of a task's progress - NOT the
/// source of truth. The real state lives in `agent::status` (DB-backed
/// string constants) for the Governed backend and `agent_v2::AgentState`
/// for the V2 backend; both continue to own persistence and drive their
/// own execution exactly as before. This enum exists only so AgentCore can
/// answer "roughly where is this task" for cross-backend
/// observability/telemetry without querying two differently-shaped
/// systems. If this ever drifts from what a backend actually did, the
/// backend is right and this is stale - callers needing an authoritative
/// answer must go to `agent::get_task`/the `agent-state-changed` event,
/// not this enum.
///
/// Intentionally covers only the states reachable by the transitions this
/// phase implements (see `reducer.rs`) - not a full mirror of either
/// backend's real state machine. Extend deliberately, not speculatively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLifecycleState {
    Created,
    Planning,
    AwaitingApproval,
    Approved,
    Executing,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_names_are_stable() {
        // Pinned: AgentCoreState persists these as map values/logs - a
        // silent rename here would be a breaking change to anything that
        // reads them back.
        assert_eq!(ExecutionBackend::Governed.as_str(), "governed");
        assert_eq!(ExecutionBackend::V2.as_str(), "v2");
    }
}
