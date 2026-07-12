use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBlock {
    pub file_path: String, pub language: String, pub code: String,
    #[serde(rename = "blockType")] pub block_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandResult { pub stdout: String, pub stderr: String, pub success: bool, }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposerMessage {
    pub role: String, pub content: String, pub file_paths: Vec<String>, pub code_blocks: Vec<CodeBlock>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposerSession {
    pub session_id: String, pub active_files: Vec<String>, pub message_history: Vec<ComposerMessage>,
}
#[derive(Default)]
pub struct ComposerSessionState { pub sessions: Mutex<Vec<ComposerSession>>, }

const MAX_RECURSION_DEPTH: usize = 3;
lazy_static::lazy_static! {
    static ref RE_SEARCH: Regex = Regex::new(r"<search_codebase>(.*?)</search_codebase>").unwrap();
    static ref RE_BLOCK: Regex = Regex::new(r"```(\w+):([^\n]+?)\n([\s\S]*?)```").unwrap();
}

fn build_system_prompt(active_files: &[String]) -> String {
    let mut p = String::from("You are an expert developer. You are editing the provided files.\nWhen you write code, you MUST start the code block with ```language:path/to/file\nand end with ```.\n\nYou have access to a codebase search tool. If you need to find code, output ONLY:\n<search_codebase>search query</search_codebase>\nWait for the system to provide the TOOL RESULT before answering.\n\nHere are the files you are working with:\n\n");
    for path in active_files {
        let f = Path::new(path);
        let c = match std::fs::read_to_string(f) { Ok(x) => x, Err(e) => format!("[unable to read: {e}]") };
        let l = f.extension().and_then(|x| x.to_str()).unwrap_or("text");
        p.push_str(&format!("--- {path} ({l})\n{c}\n\n"));
    }
    p
}

fn parse_code_blocks(response: &str) -> (String, Vec<CodeBlock>) {
    let mut blocks = Vec::new(); let mut clean = String::new(); let mut last = 0;
    for cap in RE_BLOCK.captures_iter(response) {
        let m = cap.get(0).unwrap(); let s = m.start(); let e = m.end();
        clean.push_str(&response[last..s]);
        let lang = cap.get(1).map(|c|c.as_str()).unwrap_or("").to_string();
        let fp = cap.get(2).map(|c|c.as_str().trim()).unwrap_or("").to_string();
        let code = cap.get(3).map(|c|c.as_str().trim_end()).unwrap_or("").to_string();
        let bt = if fp.starts_with("exec") { "terminal_command".to_string() } else { "file_edit".to_string() };
        blocks.push(CodeBlock{file_path:fp,language:lang,code,block_type:bt});
        last = e;
    }
    clean.push_str(&response[last..]); (clean.trim().to_string(), blocks)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInfo { pub file_path: String, pub start_line: usize, pub end_line: usize, pub text: String, pub score: f32 }
#[derive(Clone, Serialize)]
pub struct AgentStatusPayload { pub message: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentResponse { pub text: Vec<ComposerMessage>, pub autonomous_sources: Vec<SourceInfo> }

use std::collections::HashMap;
use tauri::Emitter;

pub struct ProcessTracker { pub children: Mutex<HashMap<String, std::process::Child>> }
impl ProcessTracker { pub fn new() -> Self { ProcessTracker { children: Mutex::new(HashMap::new()) } } }

#[derive(Clone, Serialize)]
pub struct TerminalStreamPayload { pub block_id: String, pub line: String, pub done: bool }

#[tauri::command]
pub async fn execute_composer_command_stream(
    app: tauri::AppHandle, state: State<'_, ProcessTracker>,
    block_id: String, command: String, workspace_root: String,
) -> Result<(), String> {
    let shell = if cfg!(target_os = "windows") { "cmd" } else { "sh" };
    let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };
    let mut child = std::process::Command::new(shell).arg(flag).arg(&command).current_dir(&workspace_root)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| format!("spawn: {e}"))?;
    let co = child.stdout.take(); let ce = child.stderr.take();
    { let mut c = state.children.lock().map_err(|e| e.to_string())?; c.insert(block_id.clone(), child); }
    if let Some(stdout) = co { use std::io::BufRead; let r = std::io::BufReader::new(stdout); for line in r.lines() { if let Ok(l) = line { let _ = app.emit("terminal-stream", TerminalStreamPayload{block_id:block_id.clone(),line:l,done:false}); } } }
    if let Some(stderr) = ce { use std::io::BufRead; let r = std::io::BufReader::new(stderr); for line in r.lines() { if let Ok(l) = line { let _ = app.emit("terminal-stream", TerminalStreamPayload{block_id:block_id.clone(),line:format!("[stderr] {l}"),done:false}); } } }
    let _ = app.emit("terminal-stream", TerminalStreamPayload{block_id:block_id.clone(),line:String::new(),done:true});
    let mut children = state.children.lock().map_err(|e| e.to_string())?;
    if let Some(mut c) = children.remove(&block_id) { let _ = c.wait(); }
    Ok(())
}

#[tauri::command]
pub fn kill_composer_command(state: State<'_, ProcessTracker>, block_id: String) -> Result<(), String> {
    let mut children = state.children.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = children.remove(&block_id) { let _ = child.kill(); let _ = child.wait(); }
    Ok(())
}

#[tauri::command]
pub fn initialize_composer_session(state: State<ComposerSessionState>, session_id: String, initial_files: Vec<String>) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = ComposerSession { session_id: session_id.clone(), active_files: initial_files.clone(), message_history: Vec::new() };
    sessions.push(session.clone());
    tracing::info!(target: "ai", event = "composer_session_created", session_id = %session_id, files = ?initial_files);
    Ok(session)
}

#[tauri::command]
pub fn add_composer_file(state: State<ComposerSessionState>, session_id: String, file_path: String) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.iter_mut().find(|s| s.session_id == session_id).ok_or_else(|| format!("Session {} not found", session_id))?;
    if !session.active_files.contains(&file_path) { session.active_files.push(file_path); }
    Ok(session.clone())
}

#[tauri::command]
pub fn remove_composer_file(state: State<ComposerSessionState>, session_id: String, file_path: String) -> Result<ComposerSession, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.iter_mut().find(|s| s.session_id == session_id).ok_or_else(|| format!("Session {} not found", session_id))?;
    session.active_files.retain(|f| f != &file_path);
    Ok(session.clone())
}

fn run_agentic_loop(
    app: &tauri::AppHandle, workspace_root: &str,
    session: &mut ComposerSession, user_content: &str, semantic_context: Option<&str>,
) -> (String, Vec<SourceInfo>) {
    let enriched = if let Some(ctx) = semantic_context {
        if !ctx.is_empty() { format!("{}\n\n--- RELEVANT CODEBASE CONTEXT ---\n{}--- END CONTEXT ---", user_content, ctx) } else { user_content.to_string() }
    } else { user_content.to_string() };
    session.message_history.push(ComposerMessage { role: "user".to_string(), content: enriched, file_paths: session.active_files.clone(), code_blocks: Vec::new() });

    let mut final_text = String::new();
    let mut all_sources: Vec<SourceInfo> = Vec::new();

    for _depth in 0..MAX_RECURSION_DEPTH {
        let response = if !session.active_files.is_empty() {
            let f = &session.active_files[0];
            let e = Path::new(f).extension().and_then(|x| x.to_str()).unwrap_or("text");
            format!("Changes:\n```{e}:{f}\n// updated\nfn f() {{}}\n```\nDone.")
        } else { "[No files]".into() };

        if let Some(caps) = RE_SEARCH.captures(&response) {
            let query = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if query.is_empty() { final_text = response; break; }
            let _ = app.emit("agent-status", AgentStatusPayload { message: format!("Searching codebase for '{}'...", query) });
            let (tool_text, mut sources) = search_codebase_with_sources(query, workspace_root);
            all_sources.append(&mut sources);
            session.message_history.push(ComposerMessage {
                role: "user".to_string(),
                content: format!("TOOL RESULT:\n{}", if tool_text.is_empty() { "No relevant code found." } else { &tool_text }),
                file_paths: session.active_files.clone(), code_blocks: Vec::new(),
            });
        } else {
            final_text = response; break;
        }
    }
    let _ = app.emit("agent-status", AgentStatusPayload { message: String::new() });
    if final_text.is_empty() { final_text = "[Max recursion depth reached]".to_string(); }
    (final_text, all_sources)
}

fn search_codebase(query: &str, workspace_root: &str) -> String {
    search_codebase_with_sources(query, workspace_root).0
}

fn search_codebase_with_sources(query: &str, workspace_root: &str) -> (String, Vec<SourceInfo>) {
    let root = Path::new(workspace_root);
    let path = root.join(".neuralforge/embeddings.json");
    let raw = match std::fs::read_to_string(&path) { Ok(r) => r, Err(_) => return (String::new(), vec![]) };
    let chunks: Vec<crate::workspace::embeddings::VectorizedChunk> = match serde_json::from_str(&raw) { Ok(c) => c, Err(_) => return (String::new(), vec![]) };
    if chunks.is_empty() { return (String::new(), vec![]); }

    let mut model = match fastembed::TextEmbedding::try_new(fastembed::InitOptionsWithLength::new(fastembed::EmbeddingModel::AllMiniLML6V2)) { Ok(m) => m, Err(_) => return (String::new(), vec![]) };
    let qv = match model.embed(vec![query], None) { Ok(v) => v, Err(_) => return (String::new(), vec![]) };
    let qv = &qv[0];

    let mut scored: Vec<(f32, &crate::workspace::embeddings::VectorizedChunk)> = chunks.iter().map(|c| (crate::workspace::embeddings::cosine_similarity(qv, &c.vector), c)).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(3);

    let sources: Vec<SourceInfo> = scored.iter().map(|(s, c)| SourceInfo { file_path: c.file_path.clone(), start_line: c.start_line, end_line: c.end_line, text: c.text.clone(), score: *s }).collect();
    let txt = scored.into_iter().map(|(s, c)| format!("File: {} (score: {:.2})\n{}", c.file_path, s, c.text)).collect::<Vec<_>>().join("\n\n---\n\n");
    (txt, sources)
}

#[tauri::command]
pub fn send_composer_message(
    app: tauri::AppHandle, state: State<ComposerSessionState>, session_id: String,
    content: String, semantic_context: Option<String>,
) -> Result<AgentResponse, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.iter_mut().find(|s| s.session_id == session_id).ok_or_else(|| format!("Session {} not found", session_id))?;
    let (final_text, auto_sources) = run_agentic_loop(&app, ".", session, &content, semantic_context.as_deref());
    let (clean_text, code_blocks) = parse_code_blocks(&final_text);
    session.message_history.push(ComposerMessage { role: "assistant".to_string(), content: clean_text, file_paths: session.active_files.clone(), code_blocks });
    Ok(AgentResponse { text: session.message_history.clone(), autonomous_sources: auto_sources })
}

#[tauri::command]
pub fn get_composer_session(state: State<ComposerSessionState>, session_id: String) -> Result<ComposerSession, String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    sessions.iter().find(|s| s.session_id == session_id).cloned().ok_or_else(|| format!("Session {} not found", session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_single() {
        let r = "Code:\n```rs:src/main.rs\nfn main() {}\n```\nEnd.";
        let (t, b) = parse_code_blocks(r);
        assert!(t.contains("Code:")); assert!(t.contains("End.")); assert_eq!(b.len(), 1);
        assert_eq!(b[0].file_path, "src/main.rs");
    }
    #[test] fn parses_multiple() {
        let r = "A:\n```rs:a.rs\nfn a() {}\n```\nB:\n```ts:b.ts\nlet x=1;\n```";
        let (t, b) = parse_code_blocks(r);
        assert!(t.contains("A:")); assert!(t.contains("B:")); assert_eq!(b.len(), 2);
    }
    #[test] fn empty_returns_none() { let (t, b) = parse_code_blocks("text."); assert_eq!(t, "text."); assert!(b.is_empty()); }
    #[test] fn prompt_includes_file() {
        let d = std::env::temp_dir().join("c_test"); std::fs::create_dir_all(&d).unwrap();
        let f = d.join("t.rs"); std::fs::write(&f, "fn e() {}").unwrap();
        let files = vec![f.to_string_lossy().to_string()];
        let p = build_system_prompt(&files); assert!(p.contains("t.rs")); assert!(p.contains("e()"));
        std::fs::remove_dir_all(&d).ok();
    }
    #[test] fn prompt_includes_tool_instructions() {
        let p = build_system_prompt(&[]); assert!(p.contains("search_codebase"));
    }
}