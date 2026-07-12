use std::process::Command;

fn run_git(path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .map_err(|e| format!("Git is not installed or not found: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let msg = if stderr.is_empty() { stdout.clone() } else { stderr };
        if msg.contains("not a git repository") || msg.contains("fatal:") {
            return Err(format!("Repository not initialized: {msg}"));
        }
        return Err(format!("Git error: {msg}"));
    }

    Ok(if stdout.is_empty() { "(no changes)".to_string() } else { stdout })
}

/// Returns the short-form git status (equivalent to `git status --short`).
#[tauri::command]
pub async fn get_git_status(path: String) -> Result<String, String> {
    run_git(&path, &["status", "--short"])
}

/// Returns the full diff of uncommitted changes (both staged and unstaged).
#[tauri::command]
pub async fn get_git_diff(path: String) -> Result<String, String> {
    run_git(&path, &["diff", "HEAD"])
}