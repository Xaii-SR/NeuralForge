use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry;
use crate::ai::provider_router::{self, TaskCapability};
use crate::database::DbState;

const MAX_RETRIES: u8 = 3;

/// System prompt for the planning/architect call - unchanged from the prior
/// direct-Ollama implementation (see git history for `intelligence::router::
/// route_through_gateway`, now removed). Only the AI transport changed.
const ARCHITECT_SYSTEM_PROMPT: &str = "You are an expert software engineer embedded in the NeuralForge IDE. Provide concise, accurate code solutions.";

/// Generates a complete response for one agent node (architect/coder/
/// reviewer), routed through `ai::provider_router::generate_for_task` -
/// Neural Forge's single sanctioned AI entry point. No direct HTTP client,
/// no hardcoded model: the router picks a real installed/configured model
/// by task capability. This replaces the old `intelligence::gateway::
/// OllamaGateway` pathway, which duplicated the Ollama adapter and always
/// hardcoded "deepseek-coder:latest".
async fn generate(
    app_handle: &AppHandle,
    task: TaskCapability,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let health = app_handle.state::<HealthRegistry>();
    let providers = {
        let db = app_handle.state::<DbState>();
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        guard.as_ref().map(provider_registry::load_providers).unwrap_or_default()
        // guard dropped here, before the .await below - a held MutexGuard
        // can't cross an await point (rusqlite::Connection is Send, not Sync)
    };

    provider_router::generate_for_task(&providers, &health, task, system_prompt, user_prompt)
        .await
        .map_err(|e| e.to_string())
}

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
    pub retries: u8,
}

impl AgentTask {
    pub fn new(id: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            state: AgentState::Initialized,
            plan_output: None,
            retries: 0,
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

// ── HITL Approval Registry ───────────────────────────────────────────────

pub struct ApprovalRegistry {
    pub channels: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
}

impl ApprovalRegistry {
    pub fn new() -> Self {
        Self { channels: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[tauri::command]
pub async fn approve_agent_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    let sender = {
        let mut map = registry.channels.lock().map_err(|e| e.to_string())?;
        map.remove(&id)
    };
    if let Some(tx) = sender {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
pub async fn reject_agent_task(id: String, registry: State<'_, ApprovalRegistry>) -> Result<(), String> {
    let sender = {
        let mut map = registry.channels.lock().map_err(|e| e.to_string())?;
        map.remove(&id)
    };
    if let Some(tx) = sender {
        let _ = tx.send(false);
    }
    Ok(())
}

// ── Worker Prompts ────────────────────────────────────────────────────────

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

// ── Payload Parser ────────────────────────────────────────────────────────

pub struct PayloadParser;

impl PayloadParser {
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

// ── File Executor ─────────────────────────────────────────────────────────

pub struct FileExecutor { workspace_root: PathBuf }

impl FileExecutor {
    pub fn new(root: &str) -> Self { Self { workspace_root: PathBuf::from(root) } }

    pub fn safe_write(&self, relative_path: &str, content: &str) -> Result<Option<String>, String> {
        if relative_path.contains("..") || relative_path.starts_with('/') || relative_path.starts_with('\\') {
            return Err("SECURITY BREACH: Path traversal detected".to_string());
        }
        let target = self.workspace_root.join(relative_path);
        let backup = if target.exists() { fs::read_to_string(&target).ok() } else { None };
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

// ── Workspace Verifier ────────────────────────────────────────────────────

pub struct WorkspaceVerifier;

impl WorkspaceVerifier {
    pub fn verify_cargo_with_stderr(&self, workspace_root: &Path) -> Result<(), String> {
        let output = Command::new("cargo").arg("check").current_dir(workspace_root)
            .output().map_err(|e| format!("Failed to spawn cargo check: {}", e))?;
        if output.status.success() { Ok(()) }
        else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(stderr)
        }
    }
}

// ── Agent Runner (Retry Loop + HITL) ──────────────────────────────────────

pub struct AgentRunner;

impl AgentRunner {
    pub async fn process_task(
        app_handle: AppHandle,
        registry: State<'_, ApprovalRegistry>,
        mut task: AgentTask,
    ) -> Result<AgentTask, String> {
        task.transition_to(AgentState::Planning, Some(&app_handle));

        let prompt = format!(
            "You are the Neural Forge Architect. Create a concise, 3-step execution plan for the user's request. Do not write code yet, only the plan.\n\nUser Request: {}",
            task.description
        );

        match generate(&app_handle, TaskCapability::Reasoning, ARCHITECT_SYSTEM_PROMPT, &prompt).await {
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

        // ── HITL Approval Gate ──
        {
            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            {
                let mut map = registry.channels.lock().map_err(|e| e.to_string())?;
                map.insert(task.id.clone(), tx);
            }

            match rx.await {
                Ok(true) => {
                    println!("[AGENT:{}] Task approved by user.", task.id);
                }
                Ok(false) | Err(_) => {
                    let err_msg = "Task rejected by user".to_string();
                    {
                        let mut map = registry.channels.lock().map_err(|e| e.to_string())?;
                        map.remove(&task.id);
                    }
                    task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                    return Err(err_msg);
                }
            }
        }

        // ── Self-Healing Execution Loop ──
        let mut coder_prompt = task.description.clone();
        loop {
            task.transition_to(AgentState::ExecutingCoder, Some(&app_handle));
            println!("[AGENT:{}] Dispatching instruction set to Coder Agent Node (retry {}/{})...", task.id, task.retries, MAX_RETRIES);

            let coder_response = match generate(&app_handle, TaskCapability::Coding, WorkerPrompts::coder_system(), &coder_prompt).await {
                Ok(res) => res,
                Err(e) => {
                    let err_msg = format!("Coder node failed: {}", e);
                    task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                    return Err(err_msg);
                }
            };

            let payloads = PayloadParser::parse_write_payloads(&coder_response);
            if payloads.is_empty() {
                let err_msg = "Coder failed to generate any valid structured tags".to_string();
                task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                return Err(err_msg);
            }

            task.transition_to(AgentState::ExecutingReviewer, Some(&app_handle));

            let mut review_payload = format!("Original Task: {}\nProposed Code:\n", task.description);
            for (path, code) in &payloads {
                review_payload.push_str(&format!("--- TARGET: {} ---\n{}\n", path, code));
            }

            match generate(&app_handle, TaskCapability::Coding, WorkerPrompts::reviewer_system(), &review_payload).await {
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
                        for (p, b) in backups.into_iter().rev() { let _ = executor.rollback(&p, b); }
                        task.transition_to(AgentState::Failed(e.clone()), Some(&app_handle));
                        return Err(e);
                    }
                }
            }

            let verifier = WorkspaceVerifier;
            match verifier.verify_cargo_with_stderr(Path::new(".")) {
                Ok(()) => {
                    println!("[AGENT:{}] Verification passed for {} files.", task.id, payloads.len());
                    task.transition_to(AgentState::Completed, Some(&app_handle));
                    return Ok(task);
                }
                Err(stderr) if task.retries < MAX_RETRIES => {
                    println!("[AGENT:{}] Compiler rejected changes (retry {}/{}). Rolling back and retrying...", task.id, task.retries + 1, MAX_RETRIES);
                    for (p, b) in backups.into_iter().rev() { let _ = executor.rollback(&p, b); }
                    task.retries += 1;
                    coder_prompt = format!(
                        "{}\n\nThe previous attempt failed with this compiler error:\n{}\nFix the code.",
                        task.description, stderr
                    );
                    // Continue loop — re-invoke coder with error context
                    continue;
                }
                Err(stderr) => {
                    for (p, b) in backups.into_iter().rev() { let _ = executor.rollback(&p, b); }
                    let err_msg = format!("Compiler failed after {} retries. Final error:\n{}", task.retries, stderr);
                    task.transition_to(AgentState::Failed(err_msg.clone()), Some(&app_handle));
                    return Err(err_msg);
                }
            }
        }
    }
}

// ── Tauri Command ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_agent_task(
    app_handle: AppHandle,
    registry: State<'_, ApprovalRegistry>,
    description: String,
) -> Result<String, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task = AgentTask::new(&task_id, &description);
    let worker_app_handle = app_handle.clone();
    // Clone the Arc so the spawned task owns it
    let registry_arc = registry.channels.clone();

    tauri::async_runtime::spawn(async move {
        // Build a minimal State-like wrapper for the spawned context
        struct DummyState<T>(T);
        impl<T> std::ops::Deref for DummyState<T> {
            type Target = T;
            fn deref(&self) -> &Self::Target { &self.0 }
        }
        let dummy_registry = DummyState(ApprovalRegistry { channels: registry_arc });
        // SAFETY: DummyState wraps a persistent Arc; lifetime is bound to the spawned task's duration.
        let registry_ref: State<'_, ApprovalRegistry> = unsafe { std::mem::transmute(&dummy_registry) };
        let _ = AgentRunner::process_task(worker_app_handle, registry_ref, task).await;
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