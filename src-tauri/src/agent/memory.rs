use crate::core::config::{MEMORY_DIR_NAME, MEMORY_SUBDIR_NAME};
use crate::core::errors::AppResult;
use rusqlite::Connection;
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

/// Sprint 2: rebuilds agent_history.md from scratch out of the ledger and
/// evidence tables - the append-only source of truth - instead of the
/// one-line-per-task notes record_task_outcome writes as tasks complete.
/// This does not replace record_task_outcome (still used for the
/// real-time append on task completion); it's a separate, on-demand
/// reconstruction that proves the human-readable history and the
/// structured ledger/evidence data never diverge.
pub fn regenerate_history(conn: &Connection, workspace_root: &Path) -> AppResult<()> {
    let memory_dir = workspace_root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
    std::fs::create_dir_all(&memory_dir)?;
    let history_path = memory_dir.join("agent_history.md");

    // ledger::list orders DESC LIMIT n; a large limit plus a re-sort by
    // seq gives us the full chain in ascending (chronological) order.
    let mut entries = crate::governance::ledger::list(conn, 1_000_000)?;
    entries.sort_by_key(|e| e.seq);

    let mut out = String::from("# Agent History (regenerated from ledger + evidence)\n");
    for entry in &entries {
        out.push_str(&format!(
            "\n## seq {} - {}\n- correlation_id: {}\n- requirement_id: {}\n- task_id: {}\n- payload: {}\n",
            entry.seq,
            entry.event_type,
            entry.correlation_id.as_deref().unwrap_or("-"),
            entry.requirement_id.as_deref().unwrap_or("-"),
            entry.task_id.as_deref().unwrap_or("-"),
            entry.payload,
        ));

        if let Some(task_id) = &entry.task_id {
            for ev in crate::governance::evidence::for_task(conn, task_id)? {
                out.push_str(&format!(
                    "  - evidence[{}]: success={} content={}\n",
                    ev.kind, ev.success, ev.content
                ));
            }
        }
    }

    std::fs::write(&history_path, out)?;
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

    /// regenerate_history() must reconstruct, from ledger + evidence alone,
    /// content that reflects a real requirement -> task -> verification
    /// flow - not the separate append-only notes record_task_outcome writes.
    #[test]
    fn regenerate_history_matches_ledger_and_evidence_content() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_agent_regenerate_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        let req = crate::governance::requirements::create(
            &conn,
            "Personalize greeting",
            "The greeting should address the user by name",
            vec!["the output contains the user's name".to_string()],
            "test-user",
        )
        .unwrap();

        let task_id = "regen-task-1";
        crate::governance::ledger::append(
            &conn,
            crate::governance::ledger::LedgerEvent::TaskCompleted,
            Some(&req.correlation_id),
            Some(&req.id),
            Some(task_id),
            serde_json::json!({"verification": "cargo check passed"}),
        )
        .unwrap();
        crate::governance::evidence::record(
            &conn,
            task_id,
            Some(&req.correlation_id),
            crate::governance::evidence::kind::VERIFICATION,
            "cargo check passed",
            true,
        )
        .unwrap();

        regenerate_history(&conn, &dir).unwrap();

        let content = std::fs::read_to_string(dir.join(".neuralforge").join("memory").join("agent_history.md")).unwrap();
        assert!(content.contains("requirement_created"));
        assert!(content.contains(&req.correlation_id));
        assert!(content.contains("task_completed"));
        assert!(content.contains(task_id));
        assert!(content.contains("evidence[verification]: success=true content=cargo check passed"));

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
