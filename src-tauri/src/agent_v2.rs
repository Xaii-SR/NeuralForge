use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};
use crate::intelligence::router;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AgentState {
    Initialized,
    Planning,
    AwaitingApproval,
    ExecutingCoder,
    ExecutingReviewer,
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

    pub fn transition_to(&mut self, new_state: AgentState, app_handle: Option<&AppHandle>) {
        println!(
            "[AGENT:{}] State Transition: {:?} -> {:?}",
            self.id, self.state, new_state
        );
        self.state = new_state.clone();

        if let Some(app) = app_handle {
            if let Err(e) = app.emit("agent-state-changed", self.clone()) {
                eprintln!("[AGENT:{}] Failed to emit state telemetry: {}", self.id, e);
            }
        }
    }
}

pub struct WorkerPrompts;

impl WorkerPrompts {
    pub fn coder_system() -> &'static str {
        "You are the Neural Forge Coder Agent. You must propose modifications to files in the workspace.\n\
        Your output MUST wrap the code in tags exactly like this. You may output multiple tags if the task requires editing multiple files:\n\
        <write_file path=\"relative/path/to/file1.rs\">\n// code here\n</write_file>\n\
        <write_file path=\"relative/path/to/file2.rs\">\n// code here\n</write_file>\n\
        Do not output markdown code blocks outside of these tags."
    }

    pub fn reviewer_system() -> &'static str {
        "You are the Neural Forge Reviewer Agent. Your job is to audit proposed implementation paths for structural flaws, security risks, or redundant logic. Output a clear LGTM or list faults."
    }
}

pub struct PayloadParser;

impl PayloadParser {
    /// Parses multiple code modifications wrapped in <write_file path="...">...</write_file>
    pub fn parse_write_payloads(input: &str) -> Vec<(String, String)> {
        let mut results = Vec::new();
        let mut search_text = input;

        let start_marker = "<write_file path=\"";
        let end_marker = "\">";
        let close_marker = "</write_file>";

        while let Some(start_idx) = search_text.find(start_marker) {
            let path_start = start_idx + start_marker.len();

            if let Some(path_end) = search_text[path_start..].find(end_marker) {
                let target_path = search_text[path_start..path_start + path_end].to_string();
                let content_start = path_start + path_end + end_marker.len();

                if let Some(content_end) = search_text[content_start..].find(close_marker) {
                    let content = search_text[content_start..content_start + content_end].to_string();
                    results.push((target_path, content));

                    search_text = &search_text[content_start + content_end + close_marker.len()..];
                    continue;
                }
            }
            break;
        }

        results
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
    pub async fn process_task(app_handle: AppHandle, mut task: AgentTask) -> Result<AgentTask, String> {
        task.transition_to(AgentState::Planning, Some(&app_handle));

        let prompt = format!(
            "You are the Neural Forge Architect. Create a concise, 3-step execution plan for the user's request. Do not write code yet, only the plan.\n\nUser Request: {}",
            task.description
        );

        match router::route_through_gateway(prompt).await {
            Ok(response) => {
                println!("[AGENT:{}] Planning successful.", task.id);
                task.plan_output = Some(response);
                task.transition_to(AgentState::AwaitingApproval, Some(&app_handle));
            }
            Err(e) => {
                let err_msg = format!("Planning failed: {}", e);
                task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                return Err(err_msg);
            }
        }

        println!("[AGENT:{}] Auto-approving plan for system validation...", task.id);
        task.transition_to(AgentState::ExecutingCoder, Some(&app_handle));

        println!("[AGENT:{}] Dispatching instruction set to Coder Agent Node...", task.id);

        let coder_response = match router::route_with_system(
            WorkerPrompts::coder_system(),
            &task.description,
        )
        .await
        {
            Ok(res) => res,
            Err(e) => {
                let err_msg = format!("Coder node failed: {}", e);
                task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                return Err(err_msg);
            }
        };

        let payloads = PayloadParser::parse_write_payloads(&coder_response);
        if payloads.is_empty() {
            let err_msg = "Coder failed to generate any valid structured tags (<write_file path=\"...\">)".to_string();
            task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
            return Err(err_msg);
        }

        task.transition_to(AgentState::ExecutingReviewer, Some(&app_handle));
        println!("[AGENT:{}] Routing Coder output to Reviewer Agent Node for verification...", task.id);

        let mut review_payload = format!("Original Task: {}\nProposed Code:\n", task.description);
        for (path, code) in &payloads {
            review_payload.push_str(&format!("--- TARGET: {} ---\n{}\n", path, code));
        }

        match router::route_with_system(WorkerPrompts::reviewer_system(), &review_payload).await {
            Ok(_) => println!("[AGENT:{}] Review complete.", task.id),
            Err(e) => {
                let err_msg = format!("Reviewer node failed: {}", e);
                task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                return Err(err_msg);
            }
        }

        task.transition_to(AgentState::Verifying, Some(&app_handle));
        let executor = FileExecutor::new(".");
        let mut backups: Vec<(String, Option<String>)> = Vec::new();

        println!("[AGENT:{}] Committing {} files to workspace...", task.id, payloads.len());

        for (relative_path, new_content) in &payloads {
            match executor.safe_write(relative_path, new_content) {
                Ok(backup) => backups.push((relative_path.clone(), backup)),
                Err(e) => {
                    println!(
                        "[AGENT:{}] Write failure on {}. Initiating atomic rollback of previously written files...",
                        task.id, relative_path
                    );
                    for (p, b) in backups.into_iter().rev() {
                        let _ = executor.rollback(&p, b);
                    }
                    task.transition_to(AgentState::Failed(e.clone()), Some(&app_handle));
                    return Err(e);
                }
            }
        }

        let verifier = WorkspaceVerifier;
        if let Err(e) = verifier.verify_cargo(Path::new(".")) {
            println!(
                "[AGENT:{}] Compiler rejected changes! Initiating atomic rollback of {} files...",
                task.id,
                backups.len()
            );
            for (p, b) in backups.into_iter().rev() {
                let _ = executor.rollback(&p, b);
            }
            task.transition_to(AgentState::Failed(e.clone()), Some(&app_handle));
            return Err(e);
        }

        println!(
            "[AGENT:{}] Verification passed successfully for {} files.",
            task.id,
            payloads.len()
        );
        task.transition_to(AgentState::Completed, Some(&app_handle));

        Ok(task)
    }
}

#[tauri::command]
pub async fn start_agent_task(app_handle: AppHandle, description: String) -> Result<String, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task = AgentTask::new(&task_id, &description);

    let worker_app_handle = app_handle.clone();

    tauri::async_runtime::spawn(async move {
        let _ = AgentRunner::process_task(worker_app_handle, task).await;
    });

    Ok(task_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_parser_multi_file() {
        let sample_output = "Multi-file edit:\n\
                             <write_file path=\"src/file1.rs\">\nfn one() {}\n</write_file>\n\
                             Some text\n\
                             <write_file path=\"src/file2.rs\">\nfn two() {}\n</write_file>";

        let parsed = PayloadParser::parse_write_payloads(sample_output);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "src/file1.rs");
        assert_eq!(parsed[0].1.trim(), "fn one() {}");
        assert_eq!(parsed[1].0, "src/file2.rs");
        assert_eq!(parsed[1].1.trim(), "fn two() {}");
    }

    #[test]
    fn test_payload_parser_missing_tags() {
        let bad_output = "This does not have tags\npub fn fail() {}";
        let parsed = PayloadParser::parse_write_payloads(bad_output);
        assert!(parsed.is_empty());
    }
}