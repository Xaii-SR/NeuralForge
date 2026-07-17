//! Events that drive AgentCore's advisory lifecycle view (see
//! `lifecycle::AgentLifecycleState`'s doc comment for what "advisory"
//! means here). Deliberately scoped to only the transitions
//! `reducer::reduce` actually implements - not a speculative full event
//! catalog for either backend's real state machine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEventType {
    PlanningStarted,
    ApprovalGranted,
    Failed,
}
