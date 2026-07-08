use super::manifest::{interpreter_for, InstalledExtension};
use crate::core::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Serialize, Clone)]
pub struct ExtensionResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct RawExtensionResult {
    success: bool,
    output: serde_json::Value,
    #[serde(default)]
    error: Option<String>,
}

const EXTENSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Runs an extension as an isolated child process: writes `request` as one
/// line of JSON to its stdin, closes stdin, waits for exit, and parses its
/// stdout as JSON. The extension never runs inside the host process and
/// never receives any capability beyond what's in `request` - it cannot,
/// for example, ask the host for an arbitrary file's contents; if a plugin
/// needs file data, the host must have already decided to include it in
/// the request (see FileSearch, which receives a pre-validated file list
/// rather than filesystem access).
pub async fn invoke_extension(ext: &InstalledExtension, request: serde_json::Value) -> AppResult<ExtensionResult> {
    let interpreter = interpreter_for(&ext.manifest.runtime)
        .ok_or_else(|| AppError::Provider(format!("unsupported extension runtime: {}", ext.manifest.runtime)))?;

    let entry_path = Path::new(&ext.dir).join(&ext.manifest.entry_point);
    if !entry_path.exists() {
        return Err(AppError::NotFound(format!("{} (entry point for {})", entry_path.display(), ext.manifest.name)));
    }

    let mut child = Command::new(interpreter)
        .arg(&entry_path)
        .current_dir(&ext.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Provider(format!("failed to spawn extension '{}': {e}", ext.manifest.name)))?;

    let request_bytes = serde_json::to_vec(&request).map_err(|e| AppError::Provider(e.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&request_bytes)
            .await
            .map_err(|e| AppError::Provider(format!("failed to write to extension stdin: {e}")))?;
        // Dropping stdin closes it, signaling EOF to the child.
    }

    let output = tokio::time::timeout(EXTENSION_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| AppError::Provider(format!("extension '{}' timed out after {}s", ext.manifest.name, EXTENSION_TIMEOUT.as_secs())))?
        .map_err(|e| AppError::Provider(format!("extension '{}' process error: {e}", ext.manifest.name)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(ExtensionResult {
            success: false,
            output: serde_json::Value::Null,
            error: Some(format!("extension exited with {}: {}", output.status, stderr.chars().take(500).collect::<String>())),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_line = stdout.lines().last().unwrap_or("").trim();

    match serde_json::from_str::<RawExtensionResult>(last_line) {
        Ok(parsed) => Ok(ExtensionResult {
            success: parsed.success,
            output: parsed.output,
            error: parsed.error,
        }),
        Err(_) => Ok(ExtensionResult {
            success: false,
            output: serde_json::Value::String(stdout.to_string()),
            error: Some("extension did not return valid JSON on its last stdout line".to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::loader;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_extensions_dir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_ext_api_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn python_available() -> bool {
        std::process::Command::new("python").arg("--version").output().is_ok()
    }

    #[tokio::test]
    async fn python_repl_executes_real_code_and_captures_output() {
        if !python_available() {
            eprintln!("skipping: python not on PATH");
            return;
        }
        let dir = temp_extensions_dir();
        loader::ensure_bundled_extensions(&dir).unwrap();
        let extensions = loader::scan(&dir).unwrap();
        let repl = extensions.iter().find(|e| e.manifest.name == "python-repl").unwrap();

        let result = invoke_extension(repl, serde_json::json!({ "code": "print(2 + 2)" })).await.unwrap();

        assert!(result.success, "expected success, got error: {:?}", result.error);
        assert_eq!(result.output.as_str().unwrap().trim(), "4");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn python_repl_reports_real_exceptions_not_a_crash() {
        if !python_available() {
            eprintln!("skipping: python not on PATH");
            return;
        }
        let dir = temp_extensions_dir();
        loader::ensure_bundled_extensions(&dir).unwrap();
        let extensions = loader::scan(&dir).unwrap();
        let repl = extensions.iter().find(|e| e.manifest.name == "python-repl").unwrap();

        let result = invoke_extension(repl, serde_json::json!({ "code": "1 / 0" })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("division"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn file_search_ranks_matching_filenames() {
        if !python_available() {
            eprintln!("skipping: python not on PATH");
            return;
        }
        let dir = temp_extensions_dir();
        loader::ensure_bundled_extensions(&dir).unwrap();
        let extensions = loader::scan(&dir).unwrap();
        let search = extensions.iter().find(|e| e.manifest.name == "file-search").unwrap();

        let files = vec!["src/auth.rs", "src/lib.rs", "tests/auth_test.rs", "README.md"];
        let result = invoke_extension(search, serde_json::json!({ "query": "auth", "files": files })).await.unwrap();

        assert!(result.success);
        let results: Vec<String> = serde_json::from_value(result.output).unwrap();
        assert!(results.contains(&"src/auth.rs".to_string()));
        assert!(results.contains(&"tests/auth_test.rs".to_string()));
        assert!(!results.contains(&"README.md".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }
}
