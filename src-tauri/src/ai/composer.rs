use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposerMessage {
    pub role: String,
    pub content: String,
    pub file_paths: Vec<String>,
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

#[tauri::command]
pub fn send_composer_message(state: State<ComposerSessionState>, session_id: String, content: String) -> Result<Vec<ComposerMessage>, String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let session = sessions.iter_mut().find(|s| s.session_id == session_id).ok_or_else(|| format!("Session {} not found", session_id))?;
    let user_msg = ComposerMessage { role: "user".to_string(), content: content.clone(), file_paths: session.active_files.clone() };
    session.message_history.push(user_msg);
    let assistant_msg = ComposerMessage { role: "assistant".to_string(), content: format!("[Simulated response for: {}]", content), file_paths: session.active_files.clone() };
    session.message_history.push(assistant_msg);
    Ok(session.message_history.clone())
}

#[tauri::command]
pub fn get_composer_session(state: State<ComposerSessionState>, session_id: String) -> Result<ComposerSession, String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    sessions.iter().find(|s| s.session_id == session_id).cloned().ok_or_else(|| format!("Session {} not found", session_id))
}