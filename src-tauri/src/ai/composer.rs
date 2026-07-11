use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBlock {
    pub file_path: String,
    pub language: String,
    pub code: String,
    #[serde(rename = "blockType")]
    pub block_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposerMessage {
    pub role: String,
    pub content: String,
    pub file_paths: Vec<String>,
    pub code_blocks: Vec<CodeBlock>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposerSession {
    pub session_id: String,
    pub active_files: Vec<String>,
    pub message_history: Vec<ComposerMessage>,
}

#[derive(Default)]
pub struct ComposerSessionState {
    pub sessions: Mutex<Vec<ComposerSession>>,
}

// ── File Reading ──────────────────────────────────────────────────────────

fn build_system_prompt(active_files: &[String]) -> String {
    let mut prompt = String::from(
        "You are an expert developer. You are editing the provided files.\n\
         When you write code, you MUST start the code block with ```language:path/to/file\n\
         and end with ```.\n\n\
         Here are the files you are working with:\n\n"
    );
    for path in active_files {
        let file_path = Path::new(path);
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => format!("[unable to read: {e}]"),
        };
        let lang = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("text");
        prompt.push_str(&format!("--- {path} ({lang})\n{content}\n\n"));
    }
    prompt
}

// ── Code Block Parser ─────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref RE_CODE_BLOCK: Regex = Regex::new(
        r"```(\w+):([^\n]+?)\n([\s\S]*?)```"
    ).unwrap();
}

fn parse_code_blocks(response: &str) -> (String, Vec<CodeBlock>) {
    let mut blocks = Vec::new();
    // Remove matched blocks from the conversational text, replacing with placeholders
    let mut clean = String::new();
    let mut last_end = 0;

    for cap in RE_CODE_BLOCK.captures_iter(response) {
        let m = cap.get(0).unwrap();
        let start = m.start();
        let end = m.end();

        // Text before this block
        clean.push_str(&response[last_end..start]);

        let language = cap.get(1).map(|c| c.as_str()).unwrap_or("").to_string();
        let file_path = cap.get(2).map(|c| c.as_str().trim()).unwrap_or("").to_string();
        let code = cap.get(3).map(|c| c.as_str().trim_end()).unwrap_or("").to_string();

        let block_type = if file_path.starts_with("exec") { "terminal_command".to_string() } else { "file_edit".to_string() };
        blocks.push(CodeBlock { file_path, language, code, block_type });
        last_end = end;
    }
    // Remaining text
    clean.push_str(&response[last_end..]);

    (clean.trim().to_string(), blocks)
}

// ── Execute Command ───────────────────────────────────────────────────────

#[tauri::command]
pub fn execute_composer_command(command: String, workspace_root: String) -> Result<CommandResult, String> {
    let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
    let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };
    let output = std::process::Command::new(shell).arg(flag).arg(&command).current_dir(&workspace_root).output().map_err(|e| format!("Failed to execute: {e}"))?;
    Ok(CommandResult { stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(), stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(), success: output.status.success() })
}

// ── Commands ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn initialize_composer_session(
    state: State<ComposerSessionState>,
    session_id: String,
    initial_files: Vec<String>,
) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = ComposerSession {
        session_id: session_id.clone(),
        active_files: initial_files.clone(),
        message_history: Vec::new(),
    };
    sessions.push(session.clone());
    tracing::info!(target: "ai", event = "composer_session_created", session_id = %session_id, files = ?initial_files);
    Ok(session)
}

#[tauri::command]
pub fn add_composer_file(
    state: State<ComposerSessionState>,
    session_id: String,
    file_path: String,
) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .iter_mut()
        .find(|s| s.session_id == session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    if !session.active_files.contains(&file_path) {
        session.active_files.push(file_path);
    }
    Ok(session.clone())
}

#[tauri::command]
pub fn remove_composer_file(
    state: State<ComposerSessionState>,
    session_id: String,
    file_path: String,
) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .iter_mut()
        .find(|s| s.session_id == session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;
    session.active_files.retain(|f| f != &file_path);
    Ok(session.clone())
}

#[tauri::command]
pub fn send_composer_message(
    state: State<ComposerSessionState>,
    session_id: String,
    content: String,
) -> Result<Vec<ComposerMessage>, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .iter_mut()
        .find(|s| s.session_id == session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Build the system prompt from active files
    let system_prompt = build_system_prompt(&session.active_files);

    // Add user message
    let user_msg = ComposerMessage {
        role: "user".to_string(),
        content,
        file_paths: session.active_files.clone(),
        code_blocks: Vec::new(),
    };
    session.message_history.push(user_msg);

    // Simulate assistant response with file-tagged code blocks
    let simulated_response = if !session.active_files.is_empty() {
        let first_file = &session.active_files[0];
        let ext = Path::new(first_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("text");
        format!(
            "Here are the changes I made:\n\n```{ext}:{first_file}\n// Updated by NeuralForge Composer\nfn updated_function() {{\n    // New implementation\n    println!(\"hello from {path}\");\n}}\n```\n\nLet me know if you need further adjustments.",
            path = session.active_files[0]
        )
    } else {
        "[No files in context]".to_string()
    };

    // Parse code blocks from the response
    let (clean_text, code_blocks) = parse_code_blocks(&simulated_response);

    let assistant_msg = ComposerMessage {
        role: "assistant".to_string(),
        content: clean_text,
        file_paths: session.active_files.clone(),
        code_blocks,
    };
    session.message_history.push(assistant_msg);

    Ok(session.message_history.clone())
}

#[tauri::command]
pub fn get_composer_session(
    state: State<ComposerSessionState>,
    session_id: String,
) -> Result<ComposerSession, String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .cloned()
        .ok_or_else(|| format!("Session {} not found", session_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_code_block() {
        let response = "Here is the code:\n```rs:src/main.rs\nfn main() {\n    println!(\"hi\");\n}\n```\nEnd.";
        let (text, blocks) = parse_code_blocks(response);
        assert!(text.contains("Here is the code:"));
        assert!(text.contains("End."));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].file_path, "src/main.rs");
        assert_eq!(blocks[0].language, "rs");
        assert!(blocks[0].code.contains("fn main()"));
    }

    #[test]
    fn parses_multiple_code_blocks() {
        let response = "Change A:\n```rs:src/a.rs\nfn a() {}\n```\nChange B:\n```ts:src/b.ts\nlet x = 1;\n```\nDone.";
        let (text, blocks) = parse_code_blocks(response);
        assert!(text.contains("Change A:"));
        assert!(text.contains("Change B:"));
        assert!(text.contains("Done."));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].file_path, "src/a.rs");
        assert_eq!(blocks[1].file_path, "src/b.ts");
    }

    #[test]
    fn no_code_blocks_returns_empty() {
        let response = "Just a comment, no code.";
        let (text, blocks) = parse_code_blocks(response);
        assert_eq!(text, "Just a comment, no code.");
        assert!(blocks.is_empty());
    }

    #[test]
    fn system_prompt_includes_file_content() {
        let dir = std::env::temp_dir().join("composer_test");
        std::fs::create_dir_all(&dir).unwrap();
        let test_file = dir.join("test.rs");
        std::fs::write(&test_file, "pub fn existing() {}").unwrap();

        let files = vec![test_file.to_string_lossy().to_string()];
        let prompt = build_system_prompt(&files);
        assert!(prompt.contains("test.rs"));
        assert!(prompt.contains("existing"));

        std::fs::remove_dir_all(&dir).ok();
    }
}