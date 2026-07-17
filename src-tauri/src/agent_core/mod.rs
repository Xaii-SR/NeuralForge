//! AgentCore: a coordination boundary sitting above the existing agent
//! execution authorities.
//!
//! ```text
//! Tauri Commands
//!       |
//!       v
//! AgentCore Commands Layer     (commands.rs - not yet wired, see its doc)
//!       |
//!       v
//! AgentCore Orchestrator       (orchestrator.rs)
//!       |
//!       +----------------------+
//!       |                      |
//!       v                      v
//! agent/ Public APIs      agent_v2 Public APIs
//!       |                      |
//!       v                      v
//! Existing Execution     Existing Execution
//! ```
//!
//! AgentCore coordinates; it does not execute. It must never absorb
//! provider selection (that's `ai::provider_router`'s job, untouched),
//! filesystem writes, rollback, or verification logic - those remain
//! entirely inside `agent::executor` (frozen) and `agent_v2`'s
//! `FileExecutor`/`WorkspaceVerifier`. See the Phase 6 architecture audit
//! for the full rationale and the two execution authorities' current
//! capabilities.
//!
//! This module owns an advisory lifecycle view
//! (`lifecycle::AgentLifecycleState`, driven by the pure `reducer::reduce`
//! function) for cross-backend observability. This is explicitly NOT the
//! "unify agent::status / agent_v2::AgentState / task_orchestrator::
//! TaskLifecycle into one authoritative lifecycle model" migration flagged
//! in the Phase 6 audit as future work - that would mean AgentCore
//! *replacing* those systems' state ownership, which it does not do. Each
//! backend continues to own and persist its own real state exactly as
//! before; see `lifecycle::AgentLifecycleState`'s doc comment for the
//! precise distinction.
//!
//! This module still does NOT:
//! - get wired into `lib.rs`'s Tauri command registration (see
//!   `commands.rs`'s doc comment)
//! - change any existing `agent::*`/`agent_v2::*` public function
//!   signature or behavior
//! - give the reducer/lifecycle state any I/O, async, or DB access (see
//!   `reducer.rs`'s doc comment)

pub mod commands;
pub mod lifecycle;
pub mod orchestrator;
pub mod reducer;
pub mod service;
pub mod types;

use lifecycle::ExecutionBackend;
use std::collections::HashMap;
use std::sync::Mutex;

/// AgentCore's own coordination state. Intentionally minimal: the only
/// thing AgentCore tracks today is which execution backend handled each
/// task id, for future cross-backend observability (e.g. a unified task
/// list). It owns no task content, no approval channels, no file state -
/// those remain in `agent`'s SQLite-backed rows and `agent_v2::
/// ApprovalRegistry` respectively.
///
/// Not yet `.manage()`-registered in `lib.rs` - see `commands.rs`'s doc
/// comment for why. Constructible and directly testable regardless.
#[derive(Default)]
pub struct AgentCoreState {
    pub(crate) task_backends: Mutex<HashMap<String, ExecutionBackend>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_starts_empty() {
        let core = AgentCoreState::default();
        assert!(core.task_backends.lock().unwrap().is_empty());
    }
}
