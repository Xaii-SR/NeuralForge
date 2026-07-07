use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const FILE_CHANGED: &str = "FILE_CHANGED";
pub const TERMINAL_OUTPUT: &str = "TERMINAL_OUTPUT";
pub const AI_RESPONSE_TOKEN: &str = "AI_RESPONSE_TOKEN";
pub const MODEL_LOADED: &str = "MODEL_LOADED";
pub const MODEL_FAILED: &str = "MODEL_FAILED";
pub const TASK_STARTED: &str = "TASK_STARTED";

#[derive(Clone, Serialize)]
pub struct FileChangedPayload {
    pub path: String,
    pub kind: String,
}

#[derive(Clone, Serialize)]
pub struct TerminalOutputPayload {
    pub session_id: String,
    pub data: String,
}

pub fn emit_file_changed(app: &AppHandle, path: &str, kind: &str) -> tauri::Result<()> {
    app.emit(
        FILE_CHANGED,
        FileChangedPayload {
            path: path.to_string(),
            kind: kind.to_string(),
        },
    )
}

pub fn emit_terminal_output(app: &AppHandle, session_id: &str, data: &str) -> tauri::Result<()> {
    app.emit(
        TERMINAL_OUTPUT,
        TerminalOutputPayload {
            session_id: session_id.to_string(),
            data: data.to_string(),
        },
    )
}
