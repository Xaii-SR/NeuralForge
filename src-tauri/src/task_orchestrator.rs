use crate::agent_controller::{AgentContext, AgentController, AgentPhase};
use crate::change_executor::{ChangeGenerator, DiffGenerator};
use crate::context_retrieval::{self, RankedFile};
use crate::error_analyzer::{ErrorAnalyzer, FailureReport, RetryState};
use crate::knowledge_store::{KnowledgeEntry, KnowledgeCategory, KnowledgeStore};
use crate::planning_engine::{self, TaskPlan, Subtask};
use crate::terminal_executor::{ExecutionRequest, ExecutionResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskLifecycle {
    Created,
    Analyzing,
    Planning,
    AwaitingApproval,
    Executing { current_step: usize, total_steps: usize },
    Observing,
    Recovering { attempt: u32, max_attempts: u32 },
    Verifying,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorTask {
    pub id: String,
    pub user_goal: String,
    pub phase: TaskLifecycle,
    pub created_at: i64,
    pub updated_at: i64,
    pub workspace_root: PathBuf,
    pub child_tasks: Vec<String>,
    pub dependencies: HashMap<usize, Vec<usize>>,
    pub execution_history: Vec<TaskPhaseRecord>,
    pub failure_reports: Vec<FailureReport>,
    pub recovery_attempts: u32,
    pub max_recovery_attempts: u32,
    pub agent_context: Option<AgentContext>,
    pub current_plan: Option<TaskPlan>,
    pub completion_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPhaseRecord {
    pub phase: String,
    pub entered_at: i64,
    pub exited_at: Option<i64>,
    pub summary: String,
    pub success: Option<bool>,
}

pub struct TaskOrchestrator;

impl TaskOrchestrator {
    pub fn create_task(user_goal: &str, workspace_root: PathBuf) -> OrchestratorTask {
        let id = format!(
            "orchestrator-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        OrchestratorTask {
            id: id.clone(),
            user_goal: user_goal.to_string(),
            phase: TaskLifecycle::Created,
            created_at: epoch_ms(),
            updated_at: epoch_ms(),
            workspace_root,
            child_tasks: Vec::new(),
            dependencies: HashMap::new(),
            execution_history: vec![TaskPhaseRecord {
                phase: "Created".to_string(),
                entered_at: epoch_ms(),
                exited_at: None,
                summary: format!("Task created: {}", user_goal),
                success: None,
            }],
            failure_reports: Vec::new(),
            recovery_attempts: 0,
            max_recovery_attempts: 3,
            agent_context: None,
            current_plan: None,
            completion_criteria: Self::default_completion_criteria(),
        }
    }

    pub fn transition(task: &mut OrchestratorTask, new_phase: TaskLifecycle) {
        // Close the previous phase record
        if let Some(last) = task.execution_history.last_mut() {
            if last.exited_at.is_none() {
                last.exited_at = Some(epoch_ms());
            }
        }

        let summary = match &new_phase {
            TaskLifecycle::Analyzing => "Analyzing workspace context".to_string(),
            TaskLifecycle::Planning => "Generating implementation plan".to_string(),
            TaskLifecycle::AwaitingApproval => "Waiting for human approval".to_string(),
            TaskLifecycle::Executing { current_step, total_steps } => {
                format!("Executing step {}/{}", current_step + 1, total_steps)
            }
            TaskLifecycle::Observing => "Observing execution results".to_string(),
            TaskLifecycle::Recovering { attempt, max_attempts } => {
                format!("Recovery attempt {}/{}", attempt + 1, max_attempts)
            }
            TaskLifecycle::Verifying => "Verifying completion".to_string(),
            TaskLifecycle::Completed => "Task completed successfully".to_string(),
            TaskLifecycle::Failed(reason) => format!("Task failed: {}", reason),
            TaskLifecycle::Cancelled => "Task cancelled".to_string(),
            TaskLifecycle::Created => "Task created".to_string(),
        };

        task.execution_history.push(TaskPhaseRecord {
            phase: format!("{:?}", new_phase),
            entered_at: epoch_ms(),
            exited_at: None,
            summary,
            success: None,
        });

        task.phase = new_phase;
        task.updated_at = epoch_ms();
    }

    /// Phase 1: Analyze — scan workspace and build context
    pub fn analyze(task: &mut OrchestratorTask, ranked_files: Vec<RankedFile>) {
        let mut ctx = AgentContext::new(&task.id, &task.user_goal, task.workspace_root.clone());
        let file_paths: Vec<String> = ranked_files.iter().map(|f| f.path.clone()).collect();
        AgentController::analyze(&mut ctx, file_paths);
        task.agent_context = Some(ctx);

        // Mark last phase as successful
        if let Some(last) = task.execution_history.last_mut() {
            last.success = Some(true);
        }

        Self::transition(task, TaskLifecycle::Planning);
    }

    /// Phase 2: Plan — generate a task plan from the agent context
    pub fn plan(task: &mut OrchestratorTask) -> Result<TaskPlan, String> {
        let ctx = task
            .agent_context
            .as_mut()
            .ok_or_else(|| "No agent context — run analyze first".to_string())?;

        let plan = planning_engine::plan_task(ctx)?;
        task.current_plan = Some(plan.clone());

        if let Some(last) = task.execution_history.last_mut() {
            last.success = Some(true);
        }

        Self::transition(task, TaskLifecycle::AwaitingApproval);
        Ok(plan)
    }

    /// Phase 3: Execute — run multi-step plan through agent pipeline
    pub fn execute_step(
        task: &mut OrchestratorTask,
        step_index: usize,
        _workspace_root: &std::path::Path,
    ) -> Result<(), String> {
        let plan = task
            .current_plan
            .as_ref()
            .ok_or_else(|| "No plan available".to_string())?;

        if step_index >= plan.subtasks.len() {
            return Err(format!(
                "Step {} out of range ({} subtasks)",
                step_index,
                plan.subtasks.len()
            ));
        }

        Self::transition(
            task,
            TaskLifecycle::Executing {
                current_step: step_index,
                total_steps: plan.subtasks.len(),
            },
        );

        Ok(())
    }

    /// Phase 4: Observe — record execution results and detect failures
    pub fn observe(task: &mut OrchestratorTask, exec_result: &ExecutionResult) {
        let report = ErrorAnalyzer::analyze(exec_result);

        if !report.failures.is_empty() {
            task.failure_reports.push(report.clone());

            if task.recovery_attempts < task.max_recovery_attempts {
                Self::transition(
                    task,
                    TaskLifecycle::Recovering {
                        attempt: task.recovery_attempts,
                        max_attempts: task.max_recovery_attempts,
                    },
                );
                task.recovery_attempts += 1;
            } else {
                Self::transition(
                    task,
                    TaskLifecycle::Failed(format!(
                        "Max recovery attempts ({}) exceeded",
                        task.max_recovery_attempts
                    )),
                );
            }
        } else {
            // All checks passed
            if let Some(last) = task.execution_history.last_mut() {
                last.success = Some(true);
            }
            Self::transition(task, TaskLifecycle::Verifying);
        }
    }

    /// Phase 5: Recover — trigger error analyzer and regenerate plan
    pub fn recover(task: &mut OrchestratorTask, exec_result: &ExecutionResult) -> Result<TaskPlan, String> {
        let ctx = task
            .agent_context
            .as_mut()
            .ok_or_else(|| "No agent context".to_string())?;

        let mut retry_state = RetryState::new(&task.id, task.max_recovery_attempts);
        for _ in 0..task.recovery_attempts {
            retry_state.record_failure(&FailureReport {
                failures: vec![],
                analysis_summary: "previous failure".into(),
                retry_suggested: true,
                max_retries_reached: false,
            });
        }

        let original_plan = task
            .current_plan
            .as_ref()
            .ok_or_else(|| "No original plan".to_string())?;

        let repair_plan = crate::error_analyzer::RepairContextBuilder::build_repair_plan(
            original_plan,
            exec_result,
            &retry_state,
        );

        task.current_plan = Some(repair_plan.clone());

        // Re-plan through the AgentController
        let steps: Vec<String> = repair_plan
            .subtasks
            .iter()
            .map(|s| s.description.clone())
            .collect();
        AgentController::plan(ctx, steps);

        if let Some(last) = task.execution_history.last_mut() {
            last.success = Some(true);
        }

        Self::transition(task, TaskLifecycle::AwaitingApproval);
        Ok(repair_plan)
    }

    /// Phase 6: Verify — confirm task completion
    pub fn verify(task: &mut OrchestratorTask, exec_result: &ExecutionResult) -> bool {
        let report = ErrorAnalyzer::analyze(exec_result);
        let passed = report.failures.is_empty();

        if let Some(last) = task.execution_history.last_mut() {
            last.success = Some(passed);
        }

        if passed {
            // Check completion criteria
            let criteria_met = Self::check_completion_criteria(task);
            if criteria_met {
                Self::transition(task, TaskLifecycle::Completed);
            } else {
                Self::transition(
                    task,
                    TaskLifecycle::Failed("Completion criteria not met".to_string()),
                );
                return false;
            }
        } else {
            Self::transition(
                task,
                TaskLifecycle::Failed(format!(
                    "Verification failed: {}",
                    report.analysis_summary
                )),
            );
        }

        passed
    }

    /// Cancel a running task
    pub fn cancel(task: &mut OrchestratorTask) {
        Self::transition(task, TaskLifecycle::Cancelled);
    }

    /// Check whether all completion criteria have been met
    fn check_completion_criteria(task: &OrchestratorTask) -> bool {
        if task.completion_criteria.is_empty() {
            return true;
        }

        // For now: all execution phases with success markers count
        let successful_phases = task
            .execution_history
            .iter()
            .filter(|r| r.success == Some(true))
            .count();

        successful_phases >= task.completion_criteria.len()
    }

    fn default_completion_criteria() -> Vec<String> {
        vec![
            "Workspace analysis completed".to_string(),
            "Plan generated".to_string(),
            "All steps executed".to_string(),
            "Verification passed".to_string(),
        ]
    }

    /// Persist the current task state to the Knowledge Store
    pub fn persist(conn: &rusqlite::Connection, task: &OrchestratorTask) -> Result<(), String> {
        let entry = KnowledgeEntry {
            id: format!("orchestrator-{}", task.id),
            category: KnowledgeCategory::Plan,
            tags: vec![
                "orchestrator-task".to_string(),
                format!("{:?}", task.phase).to_lowercase(),
            ],
            summary: format!("Task: {} [{}]", task.user_goal, task.phase_name()),
            content: serde_json::to_string_pretty(task).unwrap_or_default(),
            created_at: task.created_at,
            updated_at: task.updated_at,
            access_count: 0,
            version: 1,
        };

        KnowledgeStore::upsert(conn, &entry).map_err(|e| e.to_string())
    }

    /// Restore a task from the Knowledge Store
    pub fn restore(conn: &rusqlite::Connection, task_id: &str) -> Result<OrchestratorTask, String> {
        let entry = KnowledgeStore::get_by_id(conn, &format!("orchestrator-{}", task_id))
            .map_err(|e| e.to_string())?;

        serde_json::from_str(&entry.content).map_err(|e| format!("Failed to deserialize task: {}", e))
    }
}

impl OrchestratorTask {
    pub fn phase_name(&self) -> &str {
        match self.phase {
            TaskLifecycle::Created => "Created",
            TaskLifecycle::Analyzing => "Analyzing",
            TaskLifecycle::Planning => "Planning",
            TaskLifecycle::AwaitingApproval => "Awaiting Approval",
            TaskLifecycle::Executing { .. } => "Executing",
            TaskLifecycle::Observing => "Observing",
            TaskLifecycle::Recovering { .. } => "Recovering",
            TaskLifecycle::Verifying => "Verifying",
            TaskLifecycle::Completed => "Completed",
            TaskLifecycle::Failed(_) => "Failed",
            TaskLifecycle::Cancelled => "Cancelled",
        }
    }

    pub fn progress_percent(&self) -> f64 {
        let total_phases = 8.0;
        match &self.phase {
            TaskLifecycle::Created => 0.0,
            TaskLifecycle::Analyzing => 1.0 / total_phases * 100.0,
            TaskLifecycle::Planning => 2.0 / total_phases * 100.0,
            TaskLifecycle::AwaitingApproval => 3.0 / total_phases * 100.0,
            TaskLifecycle::Executing { current_step, total_steps } => {
                let base = 3.0;
                let exec_progress =
                    if *total_steps > 0 {
                        *current_step as f64 / *total_steps as f64
                    } else {
                        0.0
                    };
                ((base + exec_progress * 3.0) / total_phases) * 100.0
            }
            TaskLifecycle::Observing => 7.0 / total_phases * 100.0,
            TaskLifecycle::Recovering { .. } => 6.0 / total_phases * 100.0,
            TaskLifecycle::Verifying => 7.5 / total_phases * 100.0,
            TaskLifecycle::Completed => 100.0,
            TaskLifecycle::Failed(_) => 100.0,
            TaskLifecycle::Cancelled => 100.0,
        }
    }
}

fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_retrieval::RankedFile;
    use crate::planning_engine::{TaskPlan, Subtask};
    use crate::terminal_executor::{ExecutionRequest, ExecutionResult};

    #[test]
    fn task_creation() {
        let task = TaskOrchestrator::create_task("Fix auth bug", PathBuf::from("/tmp"));
        assert_eq!(task.phase, TaskLifecycle::Created);
        assert!(task.execution_history.len() >= 1);
        assert_eq!(task.max_recovery_attempts, 3);
    }

    #[test]
    fn state_transitions() {
        let mut task = TaskOrchestrator::create_task("Test task", PathBuf::from("/tmp"));
        TaskOrchestrator::transition(&mut task, TaskLifecycle::Analyzing);
        assert_eq!(task.phase, TaskLifecycle::Analyzing);
        assert!(task.execution_history.len() >= 2);

        TaskOrchestrator::transition(&mut task, TaskLifecycle::Planning);
        assert_eq!(task.phase, TaskLifecycle::Planning);

        // Verify phase records track exits
        let analyzing_record = &task.execution_history[task.execution_history.len() - 2];
        assert!(analyzing_record.exited_at.is_some());
    }

    #[test]
    fn analyze_and_plan() {
        let mut task = TaskOrchestrator::create_task("Add logging", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile {
            path: "src/main.rs".into(),
            language: "Rust".into(),
            priority: 100,
            reason: "exact match".into(),
            matched_symbols: vec![],
            snippet: String::new(),
        }];

        TaskOrchestrator::analyze(&mut task, ranked);
        assert_eq!(task.phase, TaskLifecycle::Planning);
        assert!(task.agent_context.is_some());

        let plan_result = TaskOrchestrator::plan(&mut task);
        assert!(plan_result.is_ok());
        assert!(task.current_plan.is_some());
        assert_eq!(task.phase, TaskLifecycle::AwaitingApproval);
    }

    #[test]
    fn execute_and_observe() {
        let mut task = TaskOrchestrator::create_task("Test execution", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile {
            path: "src/main.rs".into(),
            language: "Rust".into(),
            priority: 100,
            reason: "match".into(),
            matched_symbols: vec![],
            snippet: String::new(),
        }];
        TaskOrchestrator::analyze(&mut task, ranked);
        TaskOrchestrator::plan(&mut task).unwrap();

        let result = TaskOrchestrator::execute_step(&mut task, 0, &PathBuf::from("/tmp"));
        assert!(result.is_ok());

        if let TaskLifecycle::Executing { .. } = task.phase {
            // observe successful execution
            let exec_result = ExecutionResult {
                request: ExecutionRequest {
                    command: "cargo".into(),
                    arguments: vec!["check".into()],
                    working_directory: "/tmp".into(),
                    timeout_seconds: 30,
                },
                exit_code: 0,
                stdout: "Compiled successfully".into(),
                stderr: String::new(),
                started_at: 0,
                finished_at: 1000,
                duration_ms: 1000,
                was_cancelled: false,
            };
            TaskOrchestrator::observe(&mut task, &exec_result);
            assert_eq!(task.phase, TaskLifecycle::Verifying);
        }
    }

    #[test]
    fn recovery_triggers_on_failure() {
        let mut task = TaskOrchestrator::create_task("Will fail", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile {
            path: "src/main.rs".into(),
            language: "Rust".into(),
            priority: 100,
            reason: "match".into(),
            matched_symbols: vec![],
            snippet: String::new(),
        }];
        TaskOrchestrator::analyze(&mut task, ranked);
        TaskOrchestrator::plan(&mut task).unwrap();
        TaskOrchestrator::execute_step(&mut task, 0, &PathBuf::from("/tmp")).unwrap();

        let exec_result = ExecutionResult {
            request: ExecutionRequest {
                command: "cargo".into(),
                arguments: vec!["check".into()],
                working_directory: "/tmp".into(),
                timeout_seconds: 30,
            },
            exit_code: 101,
            stdout: String::new(),
            stderr: "error[E0308]: mismatched types".into(),
            started_at: 0,
            finished_at: 1000,
            duration_ms: 1000,
            was_cancelled: false,
        };

        TaskOrchestrator::observe(&mut task, &exec_result);
        assert!(matches!(task.phase, TaskLifecycle::Recovering { .. }));
    }

    #[test]
    fn max_recovery_triggers_failure() {
        let mut task = TaskOrchestrator::create_task("Will fail", PathBuf::from("/tmp"));
        task.max_recovery_attempts = 0;
        let ranked = vec![RankedFile {
            path: "src/main.rs".into(),
            language: "Rust".into(),
            priority: 100,
            reason: "match".into(),
            matched_symbols: vec![],
            snippet: String::new(),
        }];
        TaskOrchestrator::analyze(&mut task, ranked);
        TaskOrchestrator::plan(&mut task).unwrap();
        TaskOrchestrator::execute_step(&mut task, 0, &PathBuf::from("/tmp")).unwrap();

        let exec_result = ExecutionResult {
            request: ExecutionRequest {
                command: "cargo".into(),
                arguments: vec!["check".into()],
                working_directory: "/tmp".into(),
                timeout_seconds: 30,
            },
            exit_code: 101,
            stdout: String::new(),
            stderr: "error: compilation failed".into(),
            started_at: 0,
            finished_at: 1000,
            duration_ms: 1000,
            was_cancelled: false,
        };

        TaskOrchestrator::observe(&mut task, &exec_result);
        assert!(matches!(task.phase, TaskLifecycle::Failed(_)));
    }

    #[test]
    fn cancellation() {
        let mut task = TaskOrchestrator::create_task("Cancel me", PathBuf::from("/tmp"));
        TaskOrchestrator::transition(&mut task, TaskLifecycle::Analyzing);
        TaskOrchestrator::cancel(&mut task);
        assert_eq!(task.phase, TaskLifecycle::Cancelled);
    }

    #[test]
    fn progress_tracking() {
        let mut task = TaskOrchestrator::create_task("Progress test", PathBuf::from("/tmp"));
        assert_eq!(task.progress_percent(), 0.0);

        TaskOrchestrator::transition(&mut task, TaskLifecycle::Analyzing);
        assert!(task.progress_percent() > 0.0);

        TaskOrchestrator::transition(&mut task, TaskLifecycle::Completed);
        assert_eq!(task.progress_percent(), 100.0);
    }

    #[test]
    fn persistence_roundtrip() {
        let task = TaskOrchestrator::create_task("Persist me", PathBuf::from("/tmp"));
        let serialized = serde_json::to_string(&task).unwrap();
        let restored: OrchestratorTask = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.id, task.id);
        assert_eq!(restored.user_goal, task.user_goal);
        assert_eq!(restored.phase, task.phase);
    }
}