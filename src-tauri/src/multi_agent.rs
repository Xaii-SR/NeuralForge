/// Multi-Agent Orchestration Layer for Neural Forge v1.2
///
/// Supervisor + specialized agents (Research, Coding, Testing, Review)
/// built atop the existing AgentController, TaskOrchestrator, and Knowledge Store.
use crate::context_retrieval::RankedFile;
use crate::knowledge_store::{KnowledgeEntry, KnowledgeCategory, KnowledgeStore};
use crate::task_orchestrator::{OrchestratorTask, TaskLifecycle, TaskOrchestrator};
use crate::terminal_executor::ExecutionResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════════
// Agent Identity & Communication
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentRole {
    Supervisor,
    Research,
    Coding,
    Testing,
    Review,
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Supervisor => "Supervisor",
            AgentRole::Research => "Research",
            AgentRole::Coding => "Coding",
            AgentRole::Testing => "Testing",
            AgentRole::Review => "Review",
        }
    }

    pub fn capabilities(&self) -> Vec<&'static str> {
        match self {
            AgentRole::Supervisor => vec!["coordination", "task_decomposition", "result_aggregation"],
            AgentRole::Research => vec!["context_retrieval", "symbol_lookup", "dependency_analysis"],
            AgentRole::Coding => vec!["code_generation", "patch_creation", "refactoring"],
            AgentRole::Testing => vec!["verification", "failure_analysis", "regression_detection"],
            AgentRole::Review => vec!["quality_check", "security_audit", "architecture_compliance"],
        }
    }
}

/// Inter-agent message for task coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from: AgentRole,
    pub to: AgentRole,
    pub task_id: String,
    pub content: String,
    pub data: Option<String>, // serialized payload
    pub timestamp: i64,
}

// ═══════════════════════════════════════════════════════════════
// Shared Multi-Agent State
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentSession {
    pub session_id: String,
    pub user_goal: String,
    pub workspace_root: PathBuf,
    pub phase: MultiAgentPhase,
    pub agents: HashMap<AgentRole, AgentState>,
    pub message_queue: Vec<AgentMessage>,
    pub execution_history: Vec<AgentMessage>,
    pub sub_tasks: Vec<OrchestratorTask>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MultiAgentPhase {
    Initializing,
    Researching,
    Planning,
    Coding,
    Testing,
    Reviewing,
    Aggregating,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub role: AgentRole,
    pub status: AgentStatus,
    pub current_task: Option<String>,
    pub last_result: Option<String>,
    pub messages_sent: u32,
    pub messages_received: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Idle,
    Working,
    AwaitingInput,
    Completed,
    Failed(String),
}

// ═══════════════════════════════════════════════════════════════
// Multi-Agent Supervisor
// ═══════════════════════════════════════════════════════════════

pub struct MultiAgentSupervisor;

impl MultiAgentSupervisor {
    /// Create a new multi-agent session for a user goal.
    pub fn create_session(user_goal: &str, workspace_root: PathBuf) -> MultiAgentSession {
        let session_id = format!("ma-{}", epoch_ms());
        let now = epoch_ms();

        let mut agents = HashMap::new();
        for role in [AgentRole::Supervisor, AgentRole::Research, AgentRole::Coding, AgentRole::Testing, AgentRole::Review] {
            agents.insert(role.clone(), AgentState {
                role: role.clone(),
                status: AgentStatus::Idle,
                current_task: None,
                last_result: None,
                messages_sent: 0,
                messages_received: 0,
            });
        }

        // Activate supervisor immediately
        if let Some(sup) = agents.get_mut(&AgentRole::Supervisor) {
            sup.status = AgentStatus::Working;
            sup.current_task = Some(format!("Coordinate: {}", user_goal));
        }

        MultiAgentSession {
            session_id: session_id.clone(),
            user_goal: user_goal.to_string(),
            workspace_root,
            phase: MultiAgentPhase::Initializing,
            agents,
            message_queue: Vec::new(),
            execution_history: vec![AgentMessage {
                id: format!("msg-{}", epoch_ms()),
                from: AgentRole::Supervisor,
                to: AgentRole::Supervisor,
                task_id: session_id,
                content: format!("Session initialized for goal: {}", user_goal),
                data: None,
                timestamp: now,
            }],
            sub_tasks: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Phase 1: Research — dispatch Research agent to gather context.
    pub fn research(
        session: &mut MultiAgentSession,
        ranked_files: Vec<RankedFile>,
    ) -> Result<String, String> {
        session.phase = MultiAgentPhase::Researching;

        // Activate Research agent
        if let Some(research) = session.agents.get_mut(&AgentRole::Research) {
            research.status = AgentStatus::Working;
            research.current_task = Some("Gathering workspace context".to_string());
        }

        let research_summary = if ranked_files.is_empty() {
            "No relevant files found in workspace.".to_string()
        } else {
            let files: Vec<String> = ranked_files
                .iter()
                .take(5)
                .map(|f| format!("- {} ({}): {}", f.path, f.language, f.reason))
                .collect();
            format!(
                "Workspace analysis complete. {} relevant files identified:\n{}",
                ranked_files.len(),
                files.join("\n")
            )
        };

        // Store result
        if let Some(research) = session.agents.get_mut(&AgentRole::Research) {
            research.status = AgentStatus::Completed;
            research.last_result = Some(research_summary.clone());
        }

        // Enqueue plan request to Supervisor
        session.message_queue.push(AgentMessage {
            id: format!("msg-{}", epoch_ms()),
            from: AgentRole::Research,
            to: AgentRole::Supervisor,
            task_id: session.session_id.clone(),
            content: format!("Research complete: {}", research_summary),
            data: Some(serde_json::to_string(&ranked_files).unwrap_or_default()),
            timestamp: epoch_ms(),
        });

        session.phase = MultiAgentPhase::Planning;
        session.updated_at = epoch_ms();

        Ok(research_summary)
    }

    /// Phase 2: Planning — Supervisor decomposes goal into subtasks and delegates.
    pub fn plan(session: &mut MultiAgentSession) -> Result<Vec<OrchestratorTask>, String> {
        let workspace_root = session.workspace_root.clone();

        // Create subtasks for each specialized agent
        let coding_task = TaskOrchestrator::create_task(
            &format!("[Coding] Implement: {}", session.user_goal),
            workspace_root.clone(),
        );
        let testing_task = TaskOrchestrator::create_task(
            &format!("[Testing] Verify: {}", session.user_goal),
            workspace_root.clone(),
        );
        let review_task = TaskOrchestrator::create_task(
            &format!("[Review] Audit: {}", session.user_goal),
            workspace_root.clone(),
        );

        session.sub_tasks = vec![coding_task.clone(), testing_task.clone(), review_task.clone()];

        // Update agent states
        for task in &session.sub_tasks {
            let role = if task.user_goal.contains("[Coding]") {
                AgentRole::Coding
            } else if task.user_goal.contains("[Testing]") {
                AgentRole::Testing
            } else {
                AgentRole::Review
            };

            if let Some(agent) = session.agents.get_mut(&role) {
                agent.status = AgentStatus::Working;
                agent.current_task = Some(task.user_goal.clone());
            }
        }

        session.phase = MultiAgentPhase::Coding;
        session.updated_at = epoch_ms();

        Ok(session.sub_tasks.clone())
    }

    /// Phase 3: Execute — run a subtask through the existing agent pipeline.
    pub fn execute_subtask(
        session: &mut MultiAgentSession,
        conn: &rusqlite::Connection,
        subtask_index: usize,
    ) -> Result<(), String> {
        if subtask_index >= session.sub_tasks.len() {
            return Err("Subtask index out of range".to_string());
        }

        let task = &mut session.sub_tasks[subtask_index];

        // Use the existing task orchestrator pipeline
        TaskOrchestrator::transition(task, TaskLifecycle::Executing {
            current_step: 0,
            total_steps: 1,
        });

        // Record as inter-agent message
        session.message_queue.push(AgentMessage {
            id: format!("msg-{}", epoch_ms()),
            from: AgentRole::Supervisor,
            to: if task.user_goal.contains("[Coding]") { AgentRole::Coding }
                else if task.user_goal.contains("[Testing]") { AgentRole::Testing }
                else { AgentRole::Review },
            task_id: task.id.clone(),
            content: format!("Executing subtask: {}", task.user_goal),
            data: None,
            timestamp: epoch_ms(),
        });

        session.updated_at = epoch_ms();
        Ok(())
    }

    /// Phase 4: Observe — collect results from a completed subtask.
    pub fn observe_subtask(
        session: &mut MultiAgentSession,
        exec_result: &ExecutionResult,
        subtask_index: usize,
    ) {
        if subtask_index >= session.sub_tasks.len() { return; }
        let task = &mut session.sub_tasks[subtask_index];

        TaskOrchestrator::observe(task, exec_result);

        // Update agent state for the role that executed this subtask
        let role = if task.user_goal.contains("[Coding]") { AgentRole::Coding }
            else if task.user_goal.contains("[Testing]") { AgentRole::Testing }
            else { AgentRole::Review };

        if let Some(agent) = session.agents.get_mut(&role) {
            if exec_result.exit_code == 0 {
                agent.status = AgentStatus::Completed;
                agent.last_result = Some(format!("Success: exit code 0 in {}ms", exec_result.duration_ms));
            } else {
                agent.status = AgentStatus::Failed(
                    format!("Exit code {} — {} failures detected", exec_result.exit_code, exec_result.stderr.lines().count())
                );
                agent.last_result = Some(exec_result.stderr.clone());
            }
            agent.messages_sent += 1;
        }

        // Report back to Supervisor
        session.message_queue.push(AgentMessage {
            id: format!("msg-{}", epoch_ms()),
            from: role,
            to: AgentRole::Supervisor,
            task_id: task.id.clone(),
            content: format!("Subtask {} completed with exit code {}", subtask_index, exec_result.exit_code),
            data: Some(exec_result.stdout.clone()),
            timestamp: epoch_ms(),
        });

        session.updated_at = epoch_ms();
    }

    /// Phase 5: Aggregate — Supervisor collects all results and produces final summary.
    pub fn aggregate(session: &mut MultiAgentSession) -> String {
        session.phase = MultiAgentPhase::Aggregating;

        let completed: Vec<&OrchestratorTask> = session.sub_tasks.iter()
            .filter(|t| matches!(t.phase, TaskLifecycle::Completed | TaskLifecycle::Verifying))
            .collect();

        let failed: Vec<&OrchestratorTask> = session.sub_tasks.iter()
            .filter(|t| matches!(t.phase, TaskLifecycle::Failed(_)))
            .collect();

        let summary = format!(
            "Multi-Agent Session {} Complete:\n- {} subtasks succeeded\n- {} subtasks failed\n- Goal: {}",
            session.session_id,
            completed.len(),
            failed.len(),
            session.user_goal
        );

        // Persist the aggregated result
        if let Some(sup) = session.agents.get_mut(&AgentRole::Supervisor) {
            sup.status = AgentStatus::Completed;
            sup.last_result = Some(summary.clone());
        }

        session.phase = MultiAgentPhase::Completed;
        session.updated_at = epoch_ms();

        summary
    }

    /// Persist session to Knowledge Store.
    pub fn persist(conn: &rusqlite::Connection, session: &MultiAgentSession) -> Result<(), String> {
        let entry = KnowledgeEntry {
            id: format!("multi-agent-{}", session.session_id),
            category: KnowledgeCategory::Plan,
            tags: vec!["multi-agent".to_string(), "supervisor".to_string()],
            summary: format!("Multi-agent session: {}", session.user_goal),
            content: serde_json::to_string_pretty(session).unwrap_or_default(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            access_count: 0,
            version: 1,
        };
        KnowledgeStore::upsert(conn, &entry).map_err(|e| e.to_string())
    }
}

fn epoch_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_retrieval::RankedFile;
    use crate::terminal_executor::{ExecutionRequest, ExecutionResult};

    #[test]
    fn create_session_initializes_all_agents() {
        let session = MultiAgentSupervisor::create_session("Fix auth bug", PathBuf::from("/tmp"));
        assert_eq!(session.agents.len(), 5);
        assert!(session.agents.contains_key(&AgentRole::Supervisor));
        assert!(session.agents.contains_key(&AgentRole::Research));
        assert!(session.agents.contains_key(&AgentRole::Coding));
        assert!(session.agents.contains_key(&AgentRole::Testing));
        assert!(session.agents.contains_key(&AgentRole::Review));
        assert_eq!(session.phase, MultiAgentPhase::Initializing);
    }

    #[test]
    fn research_phase_produces_summary() {
        let mut session = MultiAgentSupervisor::create_session("Investigate project", PathBuf::from("/tmp"));
        let ranked = vec![
            RankedFile { path: "src/main.rs".into(), language: "Rust".into(), priority: 100, reason: "exact match".into(), matched_symbols: vec![], snippet: String::new() },
            RankedFile { path: "src/lib.rs".into(), language: "Rust".into(), priority: 90, reason: "path match".into(), matched_symbols: vec![], snippet: String::new() },
        ];
        let summary = MultiAgentSupervisor::research(&mut session, ranked).unwrap();
        assert!(summary.contains("main.rs"));
        assert!(summary.contains("lib.rs"));
        assert_eq!(session.phase, MultiAgentPhase::Planning);
        assert_eq!(session.agents[&AgentRole::Research].status, AgentStatus::Completed);
    }

    #[test]
    fn planning_decomposes_into_subtasks() {
        let mut session = MultiAgentSupervisor::create_session("Add auth system", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile { path: "src/auth.rs".into(), language: "Rust".into(), priority: 100, reason: "match".into(), matched_symbols: vec!["authenticate".into()], snippet: String::new() }];
        MultiAgentSupervisor::research(&mut session, ranked).unwrap();
        let subtasks = MultiAgentSupervisor::plan(&mut session).unwrap();
        assert_eq!(subtasks.len(), 3, "Should generate 3 subtasks (coding, testing, review)");
        assert!(subtasks.iter().any(|t| t.user_goal.contains("[Coding]")));
        assert!(subtasks.iter().any(|t| t.user_goal.contains("[Testing]")));
        assert!(subtasks.iter().any(|t| t.user_goal.contains("[Review]")));
    }

    #[test]
    fn execute_and_observe_subtask() {
        let mut session = MultiAgentSupervisor::create_session("Test feature", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile { path: "src/lib.rs".into(), language: "Rust".into(), priority: 100, reason: "match".into(), matched_symbols: vec![], snippet: String::new() }];
        MultiAgentSupervisor::research(&mut session, ranked).unwrap();
        MultiAgentSupervisor::plan(&mut session).unwrap();

        let conn = crate::database::open_for_workspace(&session.workspace_root).unwrap();
        MultiAgentSupervisor::execute_subtask(&mut session, &conn, 0).unwrap();

        let exec_result = ExecutionResult {
            request: ExecutionRequest { command: "cargo".into(), arguments: vec!["check".into()], working_directory: ".".into(), timeout_seconds: 30 },
            exit_code: 0,
            stdout: "Compiled successfully".into(),
            stderr: String::new(),
            started_at: 0, finished_at: 1000, duration_ms: 1000, was_cancelled: false,
        };

        MultiAgentSupervisor::observe_subtask(&mut session, &exec_result, 0);
        assert!(session.message_queue.iter().any(|m| m.content.contains("exit code 0")));
    }

    #[test]
    fn aggregation_produces_summary() {
        let mut session = MultiAgentSupervisor::create_session("Complete auth", PathBuf::from("/tmp"));
        let ranked = vec![RankedFile { path: "src/auth.rs".into(), language: "Rust".into(), priority: 100, reason: "match".into(), matched_symbols: vec![], snippet: String::new() }];
        MultiAgentSupervisor::research(&mut session, ranked).unwrap();
        MultiAgentSupervisor::plan(&mut session).unwrap();

        let summary = MultiAgentSupervisor::aggregate(&mut session);
        assert!(summary.contains("Multi-Agent Session"));
        assert!(summary.contains("subtasks succeeded"));
        assert_eq!(session.phase, MultiAgentPhase::Completed);
    }

    #[test]
    fn agent_roles_have_capabilities() {
        assert!(AgentRole::Research.capabilities().contains(&"context_retrieval"));
        assert!(AgentRole::Coding.capabilities().contains(&"code_generation"));
        assert!(AgentRole::Testing.capabilities().contains(&"verification"));
        assert!(AgentRole::Review.capabilities().contains(&"security_audit"));
        assert!(AgentRole::Supervisor.capabilities().contains(&"coordination"));
    }

    #[test]
    fn message_passing_between_agents() {
        let mut session = MultiAgentSupervisor::create_session("Message test", PathBuf::from("/tmp"));
        session.message_queue.push(AgentMessage {
            id: "msg-1".into(),
            from: AgentRole::Research,
            to: AgentRole::Supervisor,
            task_id: session.session_id.clone(),
            content: "Research findings ready".into(),
            data: Some(r#"{"files":3}"#.into()),
            timestamp: epoch_ms(),
        });

        assert_eq!(session.message_queue.len(), 1, "pushed message should be in the queue");
        assert!(session.message_queue.iter().any(|m| m.content.contains("Research findings")));
    }
}