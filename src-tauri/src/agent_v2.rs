use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::intelligence::router;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AgentState {
    Initialized,
    Planning,
    AwaitingApproval,
    Executing,
    Verifying,
    Completed,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentTask {
    pub id: String,
    pub description: String,
    pub state: AgentState,
    pub plan_output: Option<String>,
}

impl AgentTask {
    pub fn new(id: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            state: AgentState::Initialized,
            plan_output: None,
        }
    }

    pub fn transition_to(&mut self, new_state: AgentState) {
        println!(
            "[AGENT:{}] State Transition: {:?} -> {:?}",
            self.id, self.state, new_state
        );
        self.state = new_state;
    }
}

pub struct FileExecutor {
    workspace_root: PathBuf,
}

impl FileExecutor {
    pub fn new(root: &str) -> Self {
        Self { workspace_root: PathBuf::from(root) }
    }

    pub fn safe_write(&self, relative_path: &str, content: &str) -> Result<Option<String>, String> {
        if relative_path.contains("..") || relative_path.starts_with('/') || relative_path.starts_with('\\') {
            return Err("SECURITY BREACH: Path traversal detected in AI execution plan".to_string());
        }

        let target = self.workspace_root.join(relative_path);

        let backup = if target.exists() {
            fs::read_to_string(&target).ok()
        } else {
            None
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directories: {}", e))?;
        }

        fs::write(&target, content).map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(backup)
    }

    pub fn rollback(&self, relative_path: &str, backup: Option<String>) -> Result<(), String> {
        let target = self.workspace_root.join(relative_path);
        match backup {
            Some(content) => fs::write(&target, content).map_err(|e| format!("Rollback write failed: {}", e)),
            None => fs::remove_file(&target).map_err(|e| format!("Rollback delete failed: {}", e)),
        }
    }
}

pub struct WorkspaceVerifier;

impl WorkspaceVerifier {
    pub fn verify_cargo(&self, workspace_root: &Path) -> Result<(), String> {
        let output = Command::new("cargo")
            .arg("check")
            .current_dir(workspace_root)
            .output()
            .map_err(|e| format!("Failed to spawn cargo check: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Cargo check failed:\n{}", stderr))
        }
    }
}

pub struct AgentRunner;

impl AgentRunner {
    pub async fn process_task(mut task: AgentTask) -> Result<AgentTask, String> {
        // 1. PLANNING PHASE
        task.transition_to(AgentState::Planning);

        let prompt = format!(
            "You are the Neural Forge Architect. Create a concise, 3-step execution plan for the user's request. Do not write code yet, only the plan.\n\nUser Request: {}",
            task.description
        );

        match router::route_through_gateway(prompt).await {
            Ok(response) => {
                println!("[AGENT:{}] Planning successful.", task.id);
                task.plan_output = Some(response);
                task.transition_to(AgentState::AwaitingApproval);
            }
            Err(e) => {
                let err_msg = format!("Planning failed: {}", e);
                task.transition_to(AgentState::Failed(err_msg.clone()));
                return Err(err_msg);
            }
        }

        // 2. AWAITING APPROVAL PHASE (Auto-approved for Phase C validation)
        println!("[AGENT:{}] Auto-approving plan for system validation...", task.id);
        task.transition_to(AgentState::Executing);

        // 3. EXECUTING PHASE
        println!("[AGENT:{}] Executing task instructions...", task.id);

        let executor = FileExecutor::new(".");
        let test_file = "src/agent_v2_test_artifact.rs";
        let test_content = "pub fn ai_generated() { println!(\"AI execution verified\"); }";

        let backup = match executor.safe_write(test_file, test_content) {
            Ok(b) => b,
            Err(e) => {
                task.transition_to(AgentState::Failed(e.clone()));
                return Err(e);
            }
        };

        task.transition_to(AgentState::Verifying);

        // 4. VERIFYING PHASE
        println!("[AGENT:{}] Validating execution outcomes via compiler...", task.id);

        let verifier = WorkspaceVerifier;
        if let Err(e) = verifier.verify_cargo(Path::new(".")) {
            println!("[AGENT:{}] Verification failed! Initiating auto-rollback...", task.id);
            let _ = executor.rollback(test_file, backup);
            task.transition_to(AgentState::Failed(e.clone()));
            return Err(e);
        }

        println!("[AGENT:{}] Verification passed. Cleaning up test artifact...", task.id);
        let _ = executor.rollback(test_file, backup);

        task.transition_to(AgentState::Completed);

        Ok(task)
    }
}