use crate::core::config::{MEMORY_DIR_NAME, MEMORY_FILES, MEMORY_SUBDIR_NAME};
use crate::database::resolver::resolve_file_reference;
use crate::database::search::{enriched_context, SearchResult};
use rusqlite::Connection;
use std::path::Path;

const MAX_SEARCH_RESULTS: usize = 5;
const MAX_CHUNK_CHARS: usize = 800;
const MAX_RESOLVED_FILE_CHARS: usize = 4000;

/// Reads .neuralforge/memory/*.md and concatenates non-empty files into a
/// single context block. Missing files are silently skipped.
pub fn read_memory_context(workspace_root: &Path) -> String {
    let memory_dir = workspace_root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
    let mut sections = Vec::new();
    for file_name in MEMORY_FILES {
        let path = memory_dir.join(file_name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let trimmed = content.trim();
            let is_only_header = trimmed.lines().count() <= 1;
            if !trimmed.is_empty() && !is_only_header {
                sections.push(format!("## {file_name}\n{trimmed}"));
            }
        }
    }
    sections.join("\n\n")
}

/// Cursor-style file resolution: if the query confidently names a specific
/// file, read its real content and surface it explicitly.
fn resolved_file_block(workspace_root: &Path, conn: &Connection, query: &str) -> Option<String> {
    let result = resolve_file_reference(conn, query).ok()?;
    let path = result.resolved?;
    let mut content = std::fs::read_to_string(workspace_root.join(&path)).ok()?;
    if content.len() > MAX_RESOLVED_FILE_CHARS {
        content.truncate(MAX_RESOLVED_FILE_CHARS);
        content.push_str("\n...(truncated)");
    }
    Some(format!("Resolved \"{query}\" to `{path}`:\n```\n{content}\n```"))
}

/// Combines project memory, resolved file content, and enriched context
/// (FTS5 + symbols + dependencies with token budget) into a prompt.
pub fn build_context_prompt(workspace_root: &Path, conn: &Connection, query: &str) -> String {
    let memory = read_memory_context(workspace_root);
    let resolved = resolved_file_block(workspace_root, conn, query);
    let enriched = enriched_context(conn, workspace_root, query, &memory, resolved.as_deref(), 2000).unwrap_or_default();
    if enriched.is_empty() {
        return "You are an AI assistant embedded in the NeuralForge IDE.".to_string();
    }
    format!(
        "You are an AI assistant embedded in the NeuralForge IDE. Use the following project context to answer the user's question.\n\n{}",
        enriched
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_context_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap(); dir
    }

    #[test]
    fn read_memory_context_skips_empty_and_header_only_files() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        let context = read_memory_context(&dir);
        assert!(context.is_empty());
        std::fs::write(dir.join(".neuralforge").join("memory").join("decisions.md"),
            "# Decisions\n\nUse SQLite for the local index.").unwrap();
        let context = read_memory_context(&dir);
        assert!(context.contains("Use SQLite for the local index"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_context_prompt_includes_memory_and_search_results() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        std::fs::write(dir.join(".neuralforge").join("memory").join("architecture.md"),
            "# Architecture\n\nBackend is Rust/Tauri.").unwrap();
        std::fs::write(dir.join("auth.rs"), "fn authenticate_user() -> bool { true }\n").unwrap();
        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();
            let prompt = build_context_prompt(&dir, &conn, "how does authentication work");
            assert!(prompt.contains("Rust/Tauri"));
            assert!(prompt.contains("authenticate_user"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_context_prompt_includes_full_content_of_a_confidently_resolved_file() {
        let dir = temp_workspace();
        std::fs::create_dir_all(dir.join("carina_egti")).unwrap();
        std::fs::write(dir.join("carina_egti").join("ui_car.json"),
            "{\"screen\": \"dashboard\", \"widgets\": []}").unwrap();
        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();
            let prompt = build_context_prompt(&dir, &conn, "clear the UI JSON for the carina");
            assert!(prompt.contains("dashboard"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}