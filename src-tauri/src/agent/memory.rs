use crate::core::config::{MEMORY_DIR_NAME, MEMORY_SUBDIR_NAME};
use crate::core::errors::AppResult;
use std::path::Path;

/// Appends a one-line entry to .neuralforge/memory/agent_history.md (the
/// file already scaffolded in Phase 1) whenever a task finishes, so a
/// human (or a future agent) reading project memory sees what autonomous
/// changes were made without having to query the task database directly.
pub fn record_task_outcome(
    workspace_root: &Path,
    task_id: &str,
    objective: &str,
    file_path: &str,
    status: &str,
    verification: &str,
) -> AppResult<()> {
    let memory_dir = workspace_root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
    std::fs::create_dir_all(&memory_dir)?;
    let history_path = memory_dir.join("agent_history.md");

    let timestamp = chrono_like_timestamp();
    let entry = format!(
        "\n## {timestamp} - task {task_id} ({status})\n- Objective: {objective}\n- File: {file_path}\n- Verification: {verification}\n"
    );

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&history_path)?;
    file.write_all(entry.as_bytes())?;
    Ok(())
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("unix:{now}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_task_outcome_appends_to_agent_history() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_agent_memory_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();

        record_task_outcome(&dir, "task-1", "add logging", "main.rs", "completed", "cargo check passed").unwrap();
        record_task_outcome(&dir, "task-2", "fix bug", "lib.rs", "rolled_back", "cargo check failed").unwrap();

        let content = std::fs::read_to_string(dir.join(".neuralforge").join("memory").join("agent_history.md")).unwrap();
        assert!(content.contains("task-1"));
        assert!(content.contains("add logging"));
        assert!(content.contains("task-2"));
        assert!(content.contains("rolled_back"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
