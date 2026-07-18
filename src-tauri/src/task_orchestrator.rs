use crate::agent_controller::{AgentContext, AgentController};
use crate::context_retrieval::{RankedFile, ContextRetrieval};
use crate::error_analyzer::{ErrorAnalyzer, FailureReport, RetryState};
use crate::knowledge_store::{KnowledgeEntry, KnowledgeCategory, KnowledgeStore};
use crate::planning_engine::{self, TaskPlan};
use crate::terminal_executor::ExecutionResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

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

/// IPC event payload emitted on every state change.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestratorStatePayload {
    pub task_id: String,
    pub phase: TaskLifecycle,
    pub phase_name: String,
    pub progress_percent: f64,
    pub recovery_attempts: u32,
    pub elapsed_ms: i64,
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

    pub fn analyze(task: &mut OrchestratorTask, ranked_files: Vec<RankedFile>) {
        let mut ctx = AgentContext::new(&task.id, &task.user_goal, task.workspace_root.clone());
        let file_paths: Vec<String> = ranked_files.iter().map(|f| f.path.clone()).collect();
        AgentController::analyze(&mut ctx, file_paths);
        task.agent_context = Some(ctx);
        if let Some(last) = task.execution_history.last_mut() { last.success = Some(true); }
        Self::transition(task, TaskLifecycle::Planning);
    }

    pub fn plan(task: &mut OrchestratorTask) -> Result<TaskPlan, String> {
        let ctx = task.agent_context.as_mut().ok_or_else(|| "No agent context".to_string())?;
        let plan = planning_engine::plan_task(ctx)?;
        task.current_plan = Some(plan.clone());
        if let Some(last) = task.execution_history.last_mut() { last.success = Some(true); }
        Self::transition(task, TaskLifecycle::AwaitingApproval);
        Ok(plan)
    }

    /// Plan with full semantic graph from database.
    pub fn plan_with_db(task: &mut OrchestratorTask, conn: &rusqlite::Connection) -> Result<TaskPlan, String> {
        let ctx = task.agent_context.as_mut().ok_or_else(|| "No agent context".to_string())?;
        let dep_graph = ContextRetrieval::build_repository_graph(conn)
            .unwrap_or_default();
        let plan = planning_engine::plan_task_with_graph(ctx, &dep_graph)
            .unwrap_or_else(|_| planning_engine::plan_task(ctx).unwrap());
        task.current_plan = Some(plan.clone());
        if let Some(last) = task.execution_history.last_mut() { last.success = Some(true); }
        Self::transition(task, TaskLifecycle::AwaitingApproval);
        Ok(plan)
    }

    pub fn execute_step(task: &mut OrchestratorTask, step_index: usize, _workspace_root: &std::path::Path) -> Result<(), String> {
        let plan = task.current_plan.as_ref().ok_or_else(|| "No plan available".to_string())?;
        if step_index >= plan.subtasks.len() {
            return Err(format!("Step {} out of range", step_index));
        }
        Self::transition(task, TaskLifecycle::Executing { current_step: step_index, total_steps: plan.subtasks.len() });
        Ok(())
    }

    pub fn observe(task: &mut OrchestratorTask, exec_result: &ExecutionResult) {
        let report = ErrorAnalyzer::analyze(exec_result);
        if !report.failures.is_empty() {
            task.failure_reports.push(report.clone());
            if task.recovery_attempts < task.max_recovery_attempts {
                Self::transition(task, TaskLifecycle::Recovering { attempt: task.recovery_attempts, max_attempts: task.max_recovery_attempts });
                task.recovery_attempts += 1;
            } else {
                Self::transition(task, TaskLifecycle::Failed(format!("Max recovery attempts ({}) exceeded", task.max_recovery_attempts)));
            }
        } else {
            if let Some(last) = task.execution_history.last_mut() { last.success = Some(true); }
            Self::transition(task, TaskLifecycle::Verifying);
        }
    }

    pub fn recover(task: &mut OrchestratorTask, exec_result: &ExecutionResult) -> Result<TaskPlan, String> {
        let ctx = task.agent_context.as_mut().ok_or_else(|| "No agent context".to_string())?;
        let mut retry_state = RetryState::new(&task.id, task.max_recovery_attempts);
        for _ in 0..task.recovery_attempts {
            retry_state.record_failure(&FailureReport { failures: vec![], analysis_summary: "previous".into(), retry_suggested: true, max_retries_reached: false });
        }
        let original = task.current_plan.as_ref().ok_or_else(|| "No original plan".to_string())?;
        let repair = crate::error_analyzer::RepairContextBuilder::build_repair_plan(original, exec_result, &retry_state);
        task.current_plan = Some(repair.clone());
        let steps: Vec<String> = repair.subtasks.iter().map(|s| s.description.clone()).collect();
        AgentController::plan(ctx, steps);
        if let Some(last) = task.execution_history.last_mut() { last.success = Some(true); }
        Self::transition(task, TaskLifecycle::AwaitingApproval);
        Ok(repair)
    }

    pub fn verify(task: &mut OrchestratorTask, exec_result: &ExecutionResult) -> bool {
        let report = ErrorAnalyzer::analyze(exec_result);
        let passed = report.failures.is_empty();
        if let Some(last) = task.execution_history.last_mut() { last.success = Some(passed); }
        if passed {
            if Self::check_completion_criteria(task) {
                Self::transition(task, TaskLifecycle::Completed);
            } else {
                Self::transition(task, TaskLifecycle::Failed("Completion criteria not met".to_string()));
                return false;
            }
        } else {
            Self::transition(task, TaskLifecycle::Failed(format!("Verification failed: {}", report.analysis_summary)));
        }
        passed
    }

    pub fn cancel(task: &mut OrchestratorTask) { Self::transition(task, TaskLifecycle::Cancelled); }

    fn check_completion_criteria(task: &OrchestratorTask) -> bool {
        if task.completion_criteria.is_empty() { return true; }
        task.execution_history.iter().filter(|r| r.success == Some(true)).count() >= task.completion_criteria.len()
    }

    fn default_completion_criteria() -> Vec<String> {
        vec!["Workspace analysis completed".into(), "Plan generated".into(), "All steps executed".into(), "Verification passed".into()]
    }

    pub fn persist(conn: &rusqlite::Connection, task: &OrchestratorTask) -> Result<(), String> {
        let entry = KnowledgeEntry {
            id: format!("orchestrator-{}", task.id),
            category: KnowledgeCategory::Plan,
            tags: vec!["orchestrator-task".into(), format!("{:?}", task.phase).to_lowercase()],
            summary: format!("Task: {} [{}]", task.user_goal, task.phase_name()),
            content: serde_json::to_string_pretty(task).unwrap_or_default(),
            created_at: task.created_at, updated_at: task.updated_at,
            access_count: 0, version: 1,
        };
        KnowledgeStore::upsert(conn, &entry).map_err(|e| e.to_string())
    }

    pub fn restore(conn: &rusqlite::Connection, task_id: &str) -> Result<OrchestratorTask, String> {
        let entry = KnowledgeStore::get_by_id(conn, &format!("orchestrator-{}", task_id)).map_err(|e| e.to_string())?;
        serde_json::from_str(&entry.content).map_err(|e| format!("deserialize: {}", e))
    }
}

// ═══════════════════════════════════════════════════════════════
// Tauri IPC Commands
// ═══════════════════════════════════════════════════════════════

pub struct OrchestratorState {
    pub active_task: StdMutex<Option<OrchestratorTask>>,
}

impl Default for OrchestratorState {
    fn default() -> Self { Self { active_task: StdMutex::new(None) } }
}

fn emit_state(app: &AppHandle, task: &OrchestratorTask) {
    let _ = app.emit("orchestrator-state-changed", OrchestratorStatePayload {
        task_id: task.id.clone(),
        phase: task.phase.clone(),
        phase_name: task.phase_name().to_string(),
        progress_percent: task.progress_percent(),
        recovery_attempts: task.recovery_attempts,
        elapsed_ms: epoch_ms() - task.created_at,
    });
}

#[tauri::command]
pub fn orchestrator_create_task(
    app: AppHandle,
    state: State<'_, OrchestratorState>,
    app_state: State<'_, crate::core::state::AppState>,
    goal: String,
) -> Result<OrchestratorTask, String> {
    let workspace_root = app_state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no workspace open - cannot start a task without a real workspace root".to_string())?;
    let task = TaskOrchestrator::create_task(&goal, workspace_root);
    let result = task.clone();
    *state.active_task.lock().unwrap() = Some(task.clone());
    emit_state(&app, &task);
    Ok(result)
}

#[tauri::command]
pub fn orchestrator_approve_task(app: AppHandle, state: State<'_, OrchestratorState>) -> Result<OrchestratorTask, String> {
    let mut guard = state.active_task.lock().unwrap();
    let task = guard.as_mut().ok_or("No active task")?;
    let total = task.current_plan.as_ref().map(|p| p.subtasks.len()).unwrap_or(1);
    TaskOrchestrator::transition(task, TaskLifecycle::Executing { current_step: 0, total_steps: total });
    emit_state(&app, task);
    Ok(task.clone())
}

#[tauri::command]
pub fn orchestrator_reject_task(app: AppHandle, state: State<'_, OrchestratorState>) -> Result<OrchestratorTask, String> {
    let mut guard = state.active_task.lock().unwrap();
    let task = guard.as_mut().ok_or("No active task")?;
    TaskOrchestrator::cancel(task);
    emit_state(&app, task);
    Ok(task.clone())
}

#[tauri::command]
pub fn orchestrator_cancel_task(app: AppHandle, state: State<'_, OrchestratorState>) -> Result<(), String> {
    let mut guard = state.active_task.lock().unwrap();
    if let Some(task) = guard.as_mut() {
        TaskOrchestrator::cancel(task);
        emit_state(&app, task);
    }
    Ok(())
}

#[tauri::command]
pub fn orchestrator_get_state(state: State<'_, OrchestratorState>) -> Result<OrchestratorTask, String> {
    state.active_task.lock().unwrap().as_ref().cloned().ok_or("No active task".into())
}

#[tauri::command]
pub fn orchestrator_reset(app: AppHandle, state: State<'_, OrchestratorState>) -> Result<(), String> {
    *state.active_task.lock().unwrap() = None;
    let _ = app.emit("orchestrator-state-changed", serde_json::json!({"task_id": null, "phase": "Idle", "phase_name": "Idle", "progress_percent": 0.0, "recovery_attempts": 0, "elapsed_ms": 0}));
    Ok(())
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
                let exec = if *total_steps > 0 { *current_step as f64 / *total_steps as f64 } else { 0.0 };
                ((3.0 + exec * 3.0) / total_phases) * 100.0
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

fn epoch_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_retrieval::RankedFile;
    use crate::terminal_executor::{ExecutionRequest, ExecutionResult};
    #[test] fn task_creation() { let t = TaskOrchestrator::create_task("Fix auth bug", PathBuf::from("/tmp")); assert_eq!(t.phase, TaskLifecycle::Created); assert_eq!(t.max_recovery_attempts, 3); }
    /// orchestrator_create_task now resolves this argument from AppState's
    /// real workspace_root instead of hardcoding PathBuf::from(".") - this
    /// pins the underlying contract that create_task stores whatever root
    /// it's given verbatim, so a real configured workspace root actually
    /// takes effect rather than being silently discarded. The Tauri command
    /// itself (State<AppState> extraction) isn't unit-testable without a
    /// live app context, same constraint every other #[tauri::command] in
    /// this codebase already has - see orchestrator_create_task's real
    /// State<AppState> wiring in this file for the fix itself.
    #[test] fn create_task_preserves_the_given_workspace_root_verbatim() {
        let root = PathBuf::from("/some/real/workspace");
        let t = TaskOrchestrator::create_task("goal", root.clone());
        assert_eq!(t.workspace_root, root, "must propagate the real workspace root, not silently substitute \".\"");
    }
    #[test] fn state_transitions() { let mut t = TaskOrchestrator::create_task("Test", PathBuf::from("/tmp")); TaskOrchestrator::transition(&mut t, TaskLifecycle::Analyzing); assert_eq!(t.phase, TaskLifecycle::Analyzing); TaskOrchestrator::transition(&mut t, TaskLifecycle::Planning); assert!(t.execution_history.len() >= 2); }
    #[test] fn analyze_and_plan() { let mut t = TaskOrchestrator::create_task("Add logging", PathBuf::from("/tmp")); let ranked = vec![RankedFile { path:"src/main.rs".into(),language:"Rust".into(),priority:100,reason:"match".into(),matched_symbols:vec![],snippet:String::new()}]; TaskOrchestrator::analyze(&mut t, ranked); assert_eq!(t.phase, TaskLifecycle::Planning); assert!(t.agent_context.is_some()); assert!(TaskOrchestrator::plan(&mut t).is_ok()); }
    #[test] fn execute_and_observe() { let mut t = TaskOrchestrator::create_task("Test", PathBuf::from("/tmp")); let ranked = vec![RankedFile { path:"a.rs".into(),language:"Rust".into(),priority:100,reason:"m".into(),matched_symbols:vec![],snippet:String::new()}]; TaskOrchestrator::analyze(&mut t, ranked); TaskOrchestrator::plan(&mut t).unwrap(); TaskOrchestrator::execute_step(&mut t, 0, &PathBuf::from("/tmp")).unwrap(); let r = ExecutionResult { request:ExecutionRequest{command:"cargo".into(),arguments:vec!["check".into()],working_directory:".".into(),timeout_seconds:30}, exit_code:0,stdout:"ok".into(),stderr:String::new(),started_at:0,finished_at:1000,duration_ms:1000,was_cancelled:false }; TaskOrchestrator::observe(&mut t, &r); assert_eq!(t.phase, TaskLifecycle::Verifying); }
    #[test] fn recovery_on_failure() { let mut t = TaskOrchestrator::create_task("fail", PathBuf::from("/tmp")); let ranked = vec![RankedFile { path:"a.rs".into(),language:"Rust".into(),priority:100,reason:"m".into(),matched_symbols:vec![],snippet:String::new()}]; TaskOrchestrator::analyze(&mut t, ranked); TaskOrchestrator::plan(&mut t).unwrap(); TaskOrchestrator::execute_step(&mut t, 0, &PathBuf::from("/tmp")).unwrap(); let r = ExecutionResult { request:ExecutionRequest{command:"cargo".into(),arguments:vec![],working_directory:".".into(),timeout_seconds:30}, exit_code:101,stdout:String::new(),stderr:"error[E0308]: mismatched types".into(),started_at:0,finished_at:1000,duration_ms:1000,was_cancelled:false }; TaskOrchestrator::observe(&mut t, &r); assert!(matches!(t.phase, TaskLifecycle::Recovering{..})); }
    #[test] fn max_recovery_fails() { let mut t = TaskOrchestrator::create_task("fail", PathBuf::from("/tmp")); t.max_recovery_attempts = 0; let ranked = vec![RankedFile { path:"a.rs".into(),language:"Rust".into(),priority:100,reason:"m".into(),matched_symbols:vec![],snippet:String::new()}]; TaskOrchestrator::analyze(&mut t, ranked); TaskOrchestrator::plan(&mut t).unwrap(); TaskOrchestrator::execute_step(&mut t, 0, &PathBuf::from("/tmp")).unwrap(); let r = ExecutionResult { request:ExecutionRequest{command:"cargo".into(),arguments:vec![],working_directory:".".into(),timeout_seconds:30}, exit_code:101,stdout:String::new(),stderr:"error".into(),started_at:0,finished_at:1000,duration_ms:1000,was_cancelled:false }; TaskOrchestrator::observe(&mut t, &r); assert!(matches!(t.phase, TaskLifecycle::Failed(_))); }
    #[test] fn cancels() { let mut t = TaskOrchestrator::create_task("cancel", PathBuf::from("/tmp")); TaskOrchestrator::transition(&mut t, TaskLifecycle::Analyzing); TaskOrchestrator::cancel(&mut t); assert_eq!(t.phase, TaskLifecycle::Cancelled); }
    #[test] fn progress() { let mut t = TaskOrchestrator::create_task("p", PathBuf::from("/tmp")); assert_eq!(t.progress_percent(), 0.0); TaskOrchestrator::transition(&mut t, TaskLifecycle::Completed); assert_eq!(t.progress_percent(), 100.0); }
    #[test] fn persistence() { let t = TaskOrchestrator::create_task("persist", PathBuf::from("/tmp")); let s = serde_json::to_string(&t).unwrap(); let r: OrchestratorTask = serde_json::from_str(&s).unwrap(); assert_eq!(r.id, t.id); }
}