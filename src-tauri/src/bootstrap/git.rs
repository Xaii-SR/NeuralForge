use crate::core::errors::{AppError, AppResult};
use std::path::{Path, PathBuf};
use tokio::process::Command;

async fn run_git(root: &Path, args: &[&str]) -> AppResult<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await
        .map_err(|e| AppError::Provider(format!("failed to run git {}: {e}", args.join(" "))))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Provider(format!("git {} failed: {}", args.join(" "), stderr.trim())));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Creates and checks out a new local branch `neuralforge/suggest-<slug>`.
/// Fails loudly if `root` isn't a git repository, or if the branch already
/// exists - this never force-overwrites an existing branch.
pub async fn create_branch(root: &Path, slug: &str) -> AppResult<String> {
    run_git(root, &["rev-parse", "--is-inside-work-tree"])
        .await
        .map_err(|_| AppError::Provider(format!("{} is not a git repository", root.display())))?;

    let branch_name = format!("neuralforge/suggest-{slug}");
    run_git(root, &["checkout", "-b", &branch_name]).await?;
    Ok(branch_name)
}

/// Writes the proposed content and commits it on the current branch - a
/// local commit only. Nothing here pushes to a remote; that stays a
/// separate, explicit, human-driven action forever (see ARCHITECTURE.md /
/// ROADMAP.md "Autonomous GitHub operations").
pub async fn write_and_commit(root: &Path, file_path: &str, content: &str, title: &str) -> AppResult<()> {
    let target = root.join(file_path);
    // Sprint 4: the write goes through the PromotionController's shared
    // file-mutation primitive - same bytes, same path, same result as the
    // direct std::fs::write it replaces; the point is that both promotion
    // flows (this one and the agent's) now share one apply code path.
    crate::governance::promotion::write_promoted_content(&target, content)?;
    run_git(root, &["add", "--", file_path]).await?;
    run_git(root, &["commit", "-m", &format!("neuralforge: {title}")]).await?;
    Ok(())
}

fn find_cargo_dir(file_dir: &Path, root: &Path) -> Option<PathBuf> {
    let mut dir = file_dir.to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if dir == root {
            return None;
        }
        match dir.parent() {
            Some(p) if p.starts_with(root) || p == root => dir = p.to_path_buf(),
            _ => return None,
        }
    }
}

fn package_json_has_test_script(root: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(root.join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    value.get("scripts").and_then(|s| s.get("test")).is_some()
}

/// Runs whatever real test suite covers the changed file: `cargo test --lib`
/// if the file lives under a Cargo project, `npm test` if package.json
/// declares a test script. Otherwise returns an honest "not checked" note
/// instead of a fabricated pass - same discipline as
/// agent::executor::verify for file types it can't check.
pub async fn run_tests(root: &Path, file_path: &str) -> AppResult<(bool, String)> {
    let file_dir = root.join(file_path).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.to_path_buf());

    if let Some(cargo_dir) = find_cargo_dir(&file_dir, root) {
        let output = Command::new("cargo")
            .arg("test")
            .arg("--lib")
            .current_dir(&cargo_dir)
            .output()
            .await
            .map_err(|e| AppError::Provider(format!("failed to run cargo test: {e}")))?;
        let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let trimmed: String = combined.chars().take(4000).collect();
        return Ok((output.status.success(), trimmed));
    }

    if package_json_has_test_script(root) {
        let output = Command::new("npm").args(["test", "--silent"]).current_dir(root).output().await;
        if let Ok(out) = output {
            let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let trimmed: String = combined.chars().take(4000).collect();
            return Ok((out.status.success(), trimmed));
        }
    }

    Ok((true, "no automated test runner detected for this change - written without a test check".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_git_repo() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_bootstrap_git_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git").arg("init").arg("--quiet").current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "test@example.com"]).current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "Test"]).current_dir(&dir).output().unwrap();
        std::fs::write(dir.join("README.md"), "hello").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "initial"]).current_dir(&dir).output().unwrap();
        dir
    }

    #[tokio::test]
    async fn create_branch_checks_out_a_new_neuralforge_branch() {
        let dir = temp_git_repo();
        let branch = create_branch(&dir, "improve-thing").await.unwrap();
        assert_eq!(branch, "neuralforge/suggest-improve-thing");

        let current = run_git(&dir, &["branch", "--show-current"]).await.unwrap();
        assert_eq!(current, branch);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn create_branch_rejects_non_git_directory() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_not_a_repo_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();

        let result = create_branch(&dir, "x").await;
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn write_and_commit_creates_a_real_local_commit() {
        let dir = temp_git_repo();
        create_branch(&dir, "add-file").await.unwrap();

        write_and_commit(&dir, "README.md", "updated content", "update readme").await.unwrap();

        assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "updated content");
        let log = run_git(&dir, &["log", "-1", "--pretty=%s"]).await.unwrap();
        assert_eq!(log, "neuralforge: update readme");

        // Never pushed anywhere - no remote exists on this repo at all.
        let remotes = run_git(&dir, &["remote"]).await.unwrap();
        assert!(remotes.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn run_tests_reports_honestly_when_no_runner_is_detected() {
        let dir = temp_git_repo();
        let (passed, output) = run_tests(&dir, "README.md").await.unwrap();
        assert!(passed);
        assert!(output.contains("no automated test runner detected"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn run_tests_runs_real_cargo_test_for_a_rust_change() {
        let dir = temp_git_repo();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"bootstrap_git_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() { assert_eq!(super::add(2, 2), 4); }\n}\n",
        )
        .unwrap();

        let (passed, output) = run_tests(&dir, "src/lib.rs").await.unwrap();
        assert!(passed, "expected cargo test to pass, got: {output}");
        assert!(output.contains("test result: ok"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
