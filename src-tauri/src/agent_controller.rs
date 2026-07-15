use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Five-phase autonomous agent state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentPhase {
    Idle,
    Analyzing { task: String },
    Planning { plan: Vec<String> },
    Executing { current_step: usize },
    Observing { observation: Option<String> },
    Verifying { passed: bool, reason: Option<String> },
    Completed,
    Failed(String),
}

/// Structured context the agent maintains throughout a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub task_id: String,
    pub user_task: String,
    pub workspace_root: PathBuf,
    pub relevant_files: Vec<String>,
    pub analysis_notes: String,
    pub plan_steps: Vec<String>,
    pub phase: AgentPhase,
    pub started_at: i64,
}

impl AgentContext {
    pub fn new(task_id: &str, user_task: &str, workspace_root: PathBuf) -> Self {
        Self {
            task_id: task_id.to_string(),
            user_task: user_task.to_string(),
            workspace_root,
            relevant_files: Vec::new(),
            analysis_notes: String::new(),
            plan_steps: Vec::new(),
            phase: AgentPhase::Idle,
            started_at: chrono_now(),
        }
    }
}

/// The Agent Controller orchestrates the autonomous workflow.
pub struct AgentController;

impl AgentController {
    /// Phase 1: Analyze — scan workspace for relevant files.
    pub fn analyze(ctx: &mut AgentContext, relevant_files: Vec<String>) {
        ctx.relevant_files = relevant_files;
        ctx.analysis_notes = format!(
            "Workspace has {} relevant files for task: {}",
            ctx.relevant_files.len(),
            ctx.user_task
        );
        ctx.phase = AgentPhase::Analyzing {
            task: ctx.user_task.clone(),
        };
    }

    /// Phase 2: Plan — decompose task into ordered steps.
    pub fn plan(ctx: &mut AgentContext, steps: Vec<String>) {
        ctx.plan_steps = steps.clone();
        ctx.phase = AgentPhase::Planning { plan: steps };
    }

    /// Phase 3: Execute — advance through the plan steps.
    pub fn execute_step(ctx: &mut AgentContext, step_index: usize) {
        ctx.phase = AgentPhase::Executing {
            current_step: step_index,
        };
    }

    /// Phase 4: Observe — record the result of a step.
    pub fn observe(ctx: &mut AgentContext, observation: String) {
        ctx.phase = AgentPhase::Observing {
            observation: Some(observation),
        };
    }

    /// Phase 5: Verify — validate the outcome.
    pub fn verify(ctx: &mut AgentContext, passed: bool, reason: Option<String>) {
        ctx.phase = AgentPhase::Verifying { passed, reason };
    }

    /// Transition to terminal state.
    pub fn complete(ctx: &mut AgentContext) {
        ctx.phase = AgentPhase::Completed;
    }

    pub fn fail(ctx: &mut AgentContext, reason: String) {
        ctx.phase = AgentPhase::Failed(reason);
    }
}

/// Simple epoch millisecond helper (avoids chrono dependency).
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_lifecycle() {
        let mut ctx = AgentContext::new("test-1", "Fix auth bug", PathBuf::from("/tmp"));
        assert_eq!(ctx.phase, AgentPhase::Idle);

        AgentController::analyze(&mut ctx, vec!["auth.rs".into()]);
        assert!(matches!(ctx.phase, AgentPhase::Analyzing { .. }));

        AgentController::plan(&mut ctx, vec!["Edit auth.rs".into()]);
        assert!(matches!(ctx.phase, AgentPhase::Planning { .. }));

        AgentController::execute_step(&mut ctx, 0);
        assert!(matches!(ctx.phase, AgentPhase::Executing { .. }));

        AgentController::observe(&mut ctx, "File written successfully".into());
        assert!(matches!(ctx.phase, AgentPhase::Observing { .. }));

        AgentController::verify(&mut ctx, true, None);
        assert!(matches!(ctx.phase, AgentPhase::Verifying { .. }));

        AgentController::complete(&mut ctx);
        assert_eq!(ctx.phase, AgentPhase::Completed);
    }

    #[test]
    fn failure_path() {
        let mut ctx = AgentContext::new("test-2", "Break things", PathBuf::from("/tmp"));
        AgentController::fail(&mut ctx, "Compiler error".into());
        assert!(matches!(ctx.phase, AgentPhase::Failed { .. }));
    }
}