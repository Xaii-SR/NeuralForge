use tokio::io::AsyncBufReadExt;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

use crate::core::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub allowlist: HashSet<String>,
    pub denylist: HashSet<String>,
    pub max_timeout_seconds: u64,
    pub require_approval: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        let mut allowlist = HashSet::new();
        allowlist.insert("cargo".into());
        allowlist.insert("npm".into());
        allowlist.insert("pnpm".into());
        allowlist.insert("node".into());
        allowlist.insert("rustc".into());
        allowlist.insert("git".into());
        allowlist.insert("echo".into());
        let mut denylist = HashSet::new();
        denylist.insert("rm -rf /".into());
        denylist.insert("rm -rf ~".into());
        Self { allowlist, denylist, max_timeout_seconds: 300, require_approval: true }
    }
}

pub struct SandboxState {
    pub config: Mutex<SandboxConfig>,
}

impl Default for SandboxState {
    fn default() -> Self { Self { config: Mutex::new(SandboxConfig::default()) } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub command: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub request: ExecutionRequest,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub duration_ms: u64,
    pub was_cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionStreamPayload {
    pub execution_id: String,
    pub chunk: String,
    pub stream: String,
    pub done: bool,
}

fn validate_request(req: &ExecutionRequest, config: &SandboxConfig) -> AppResult<()> {
    for blocked in &config.denylist {
        if denylist_matches(req, blocked) {
            return Err(AppError::CommandRejected(format!("blocked by denylist '{}'", blocked)));
        }
    }
    if !config.allowlist.contains(&req.command) {
        return Err(AppError::CommandRejected(format!("'{}' not in allowlist", req.command)));
    }
    if req.timeout_seconds > config.max_timeout_seconds {
        return Err(AppError::CommandRejected("timeout exceeds maximum".into()));
    }
    let cwd = PathBuf::from(&req.working_directory);
    if !cwd.exists() || !cwd.is_dir() {
        return Err(AppError::InvalidPath(req.working_directory.clone()));
    }
    Ok(())
}

fn denylist_matches(req: &ExecutionRequest, blocked: &str) -> bool {
    let command = req.command.to_lowercase();
    let args: Vec<String> = req.arguments.iter().map(|a| a.to_lowercase()).collect();
    let full = std::iter::once(command.as_str())
        .chain(args.iter().map(|s| s.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let pattern = blocked.to_lowercase();

    if full.contains(&pattern) {
        return true;
    }

    match command.as_str() {
        "rm" => {
            let delete_root = args.iter().any(|a| a == "/" || a == "~" || a == "/root" || a == "c:\\");
            let dangerous_flag = args.iter().any(|a| a == "-r" || a == "-rf" || a.contains("-r") || a.contains("-f"));
            delete_root && dangerous_flag
        }
        "find" => args.iter().any(|a| a == "-delete" || a == "-exec") && args.iter().any(|a| a == "/" || a == "."),
        "sh" | "bash" | "zsh" | "powershell" | "pwsh" => {
            full.contains("rm -rf /")
                || full.contains("rm -rf ~")
                || full.contains("find / -delete")
                || full.contains("find . -delete")
        }
        "cmd" => full.contains("del /s") || full.contains("rd /s") || full.contains("format "),
        _ => false,
    }
}

pub async fn execute_command(app: AppHandle, sandbox: &SandboxState, req: ExecutionRequest) -> AppResult<ExecutionResult> {
    let config = sandbox.config.lock().unwrap().clone();
    validate_request(&req, &config)?;

    let eid = format!("exec-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos());
    let started_at = epoch_ms();
    let td = Duration::from_secs(req.timeout_seconds);

    let mut child = TokioCommand::new(&req.command)
        .args(&req.arguments)
        .current_dir(PathBuf::from(&req.working_directory))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Terminal(format!("spawn: {}", e)))?;

    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");

    let a1 = app.clone(); let a2 = app.clone();
    let e1 = eid.clone(); let e2 = eid.clone();
    let stdout_task = tokio::spawn(read_stream(stdout, e1, "stdout", a1));
    let stderr_task = tokio::spawn(read_stream(stderr, e2, "stderr", a2));

    let exit = match timeout(td, child.wait()).await {
        Ok(Ok(s)) => (s.code().unwrap_or(-1), false),
        Ok(Err(e)) => return Err(AppError::Terminal(format!("wait: {}", e))),
        Err(_) => { let _ = child.kill().await; let _ = child.wait().await; return Err(AppError::CommandTimeout(format!("timed out after {}s", req.timeout_seconds))); }
    };

    let out = stdout_task.await.unwrap_or_default();
    let err = stderr_task.await.unwrap_or_default();
    let finished_at = epoch_ms();

    Ok(ExecutionResult { request: req, exit_code: exit.0, stdout: out, stderr: err, started_at, finished_at, duration_ms: (finished_at - started_at) as u64, was_cancelled: exit.1 })
}

async fn read_stream<R: tokio::io::AsyncRead + Unpin + Send + 'static>(reader: R, eid: String, stream: &str, app: AppHandle) -> String {
    let buf = tokio::io::BufReader::new(reader);
    let mut lines = buf.lines();
    let mut acc = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        acc.push_str(&line); acc.push('\n');
        let _ = app.emit("execution-stream", ExecutionStreamPayload { execution_id: eid.clone(), chunk: line, stream: stream.to_string(), done: false });
    }
    let _ = app.emit("execution-stream", ExecutionStreamPayload { execution_id: eid, chunk: String::new(), stream: stream.to_string(), done: true });
    acc
}

fn epoch_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }

#[tauri::command]
pub async fn execute_sandboxed_command(app: AppHandle, state: State<'_, SandboxState>, command: String, arguments: Vec<String>, working_directory: String, timeout_seconds: u64) -> Result<ExecutionResult, String> {
    execute_command(app, &state, ExecutionRequest { command, arguments, working_directory, timeout_seconds }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn allowlist_add(state: State<'_, SandboxState>, command: String) -> Result<(), String> { state.config.lock().unwrap().allowlist.insert(command); Ok(()) }

#[tauri::command]
pub fn denylist_add(state: State<'_, SandboxState>, pattern: String) -> Result<(), String> { state.config.lock().unwrap().denylist.insert(pattern); Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn validate_blocks_denied() {
        let c = SandboxConfig { denylist: ["rm -rf /".into()].into_iter().collect(), ..Default::default() };
        let r = ExecutionRequest { command: "rm".into(), arguments: vec!["-rf".into(), "/".into()], working_directory: "/tmp".into(), timeout_seconds: 30 };
        assert!(validate_request(&r, &c).is_err());
    }
    #[test] fn validate_blocks_disallowed() {
        assert!(validate_request(&ExecutionRequest { command: "evil".into(), arguments: vec![], working_directory: ".".into(), timeout_seconds: 10 }, &SandboxConfig::default()).is_err());
    }
    #[test] fn validate_blocks_timeout() {
        assert!(validate_request(&ExecutionRequest { command: "cargo".into(), arguments: vec!["build".into()], working_directory: ".".into(), timeout_seconds: 9999 }, &SandboxConfig::default()).is_err());
    }
    #[test] fn validate_rejects_missing_dir() {
        assert!(validate_request(&ExecutionRequest { command: "cargo".into(), arguments: vec![], working_directory: "/no/such".into(), timeout_seconds: 30 }, &SandboxConfig::default()).is_err());
    }
    #[test] fn validate_accepts() {
        let d = std::env::temp_dir().to_string_lossy().to_string();
        assert!(validate_request(&ExecutionRequest { command: "cargo".into(), arguments: vec!["--version".into()], working_directory: d, timeout_seconds: 30 }, &SandboxConfig::default()).is_ok());
    }
    #[test] fn allowlist_mod() {
        let s = SandboxState::default();
        s.config.lock().unwrap().allowlist.insert("make".into());
        assert!(s.config.lock().unwrap().allowlist.contains("make"));
    }
    #[test] fn denylist_substr() {
        let c = SandboxConfig { denylist: ["DROP TABLE".into()].into_iter().collect(), ..Default::default() };
        let r = ExecutionRequest { command: "echo".into(), arguments: vec!["DROP TABLE users".into()], working_directory: ".".into(), timeout_seconds: 10 };
        assert!(validate_request(&r, &c).is_err());
    }
}
