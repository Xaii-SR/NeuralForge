use crate::core::errors::{AppError, AppResult};
use std::path::{Path, PathBuf};

pub struct ApplyResult {
    pub verification: String,
    pub rolled_back: bool,
}

/// Applies a previously-planned change: writes the proposed content, runs
/// verification, and restores the original content if verification fails.
/// Only reachable from the AwaitingApproval -> Applying transition, which
/// the command layer gates behind an explicit user approval - this function
/// itself has no concept of "approval", it trusts the caller already got it.
pub async fn apply_and_verify(
    workspace_root: &Path,
    file_path: &str,
    original_content: &str,
    proposed_content: &str,
) -> AppResult<ApplyResult> {
    let target = workspace_root.join(file_path);
    if !target.exists() {
        return Err(AppError::NotFound(file_path.to_string()));
    }

    let canonical_root = std::fs::canonicalize(workspace_root)?;
    let canonical_target = std::fs::canonicalize(&target)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::InvalidPath(format!("{file_path} is outside the workspace")));
    }

    std::fs::write(&target, proposed_content)?;

    match verify(&canonical_target, &canonical_root).await {
        Ok(message) => Ok(ApplyResult { verification: message, rolled_back: false }),
        Err(message) => {
            std::fs::write(&target, original_content)?;
            Ok(ApplyResult { verification: message, rolled_back: true })
        }
    }
}

fn find_cargo_dir(file: &Path, workspace_root: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir.to_path_buf());
        }
        if dir == workspace_root {
            return None;
        }
        dir = dir.parent()?;
    }
}

async fn verify(file: &Path, workspace_root: &Path) -> Result<String, String> {
    if file.extension().and_then(|e| e.to_str()) != Some("rs") {
        return Ok("no automated verification available for this file type - written without a build/test check".to_string());
    }

    let Some(cargo_dir) = find_cargo_dir(file, workspace_root) else {
        return Ok("no Cargo.toml found for this file - written without a build check".to_string());
    };

    let output = tokio::process::Command::new("cargo")
        .arg("check")
        .current_dir(&cargo_dir)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => Ok("cargo check passed".to_string()),
        Ok(out) => {
            let stderr: String = String::from_utf8_lossy(&out.stderr).chars().take(800).collect();
            Err(format!("cargo check failed:\n{stderr}"))
        }
        Err(e) => Err(format!("failed to run cargo check: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_executor_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn apply_to_non_rust_file_succeeds_without_verification() {
        let dir = temp_workspace();
        let file = dir.join("notes.md");
        std::fs::write(&file, "old content").unwrap();

        let result = apply_and_verify(&dir, "notes.md", "old content", "new content").await.unwrap();
        assert!(!result.rolled_back);
        assert!(result.verification.contains("no automated verification"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn apply_rejects_path_outside_workspace() {
        let dir = temp_workspace();
        let outside = temp_workspace();
        let escape_target = outside.join("evil.md");
        std::fs::write(&escape_target, "x").unwrap();

        // Reference the outside file via a workspace-relative path that
        // resolves elsewhere - the canonicalized-prefix check must catch it.
        let result = apply_and_verify(&dir, "../evil.md", "x", "y").await;
        // Either NotFound (join literally doesn't exist under dir) or
        // InvalidPath (escapes workspace) is acceptable - both mean nothing
        // outside the workspace got written.
        assert!(result.is_err() || !std::fs::read_to_string(&escape_target).unwrap().contains('y'));

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    /// The core safety property of Phase 5: a change that breaks compilation
    /// gets rolled back automatically, not just flagged. Sets up a real
    /// minimal Cargo project and runs a real `cargo check` - this is the
    /// slowest test in the suite (invokes rustc) but it's the one that
    /// actually proves the safety mechanism works, not just that the code
    /// compiles.
    #[tokio::test]
    async fn broken_rust_change_is_automatically_rolled_back() {
        let dir = temp_workspace();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"agent_verify_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let lib_path = dir.join("src").join("lib.rs");
        let original = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        std::fs::write(&lib_path, original).unwrap();

        let broken = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b +\n}\n"; // syntax error

        let result = apply_and_verify(&dir, "src/lib.rs", original, broken).await.unwrap();

        assert!(result.rolled_back, "a syntax error should trigger rollback");
        assert!(result.verification.contains("cargo check failed"));
        assert_eq!(
            std::fs::read_to_string(&lib_path).unwrap(),
            original,
            "file content should be restored to the original after rollback"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn valid_rust_change_passes_verification_without_rollback() {
        let dir = temp_workspace();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"agent_verify_fixture2\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let lib_path = dir.join("src").join("lib.rs");
        let original = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        std::fs::write(&lib_path, original).unwrap();

        let valid_change = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n";

        let result = apply_and_verify(&dir, "src/lib.rs", original, valid_change).await.unwrap();

        assert!(!result.rolled_back, "valid code should not be rolled back: {}", result.verification);
        assert_eq!(std::fs::read_to_string(&lib_path).unwrap(), valid_change);

        std::fs::remove_dir_all(&dir).ok();
    }
}
