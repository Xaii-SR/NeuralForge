use crate::core::config::{MEMORY_DIR_NAME, MEMORY_FILES, MEMORY_SUBDIR_NAME};
use crate::database::resolver::resolve_file_reference;
use crate::database::search::{enriched_context, SearchResult};
use rusqlite::Connection;
use std::path::Path;

/// Classifies a user query into a retrieval intent for context-aware routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalIntent {
    Structural,
    Semantic,
}

/// Classifies a natural-language query into a RetrievalIntent based on keyword patterns.
pub fn classify_intent(query: &str) -> RetrievalIntent {
    let q = query.to_lowercase();
    let structural_keywords = [
        "architecture", "depend", "structure", "import", "module",
        "project layout", "component", "relationship", "module map",
        "file tree", "how is", "organized", "dependency graph",
    ];
    if structural_keywords.iter().any(|kw| q.contains(kw)) {
        RetrievalIntent::Structural
    } else {
        RetrievalIntent::Semantic
    }
}

const MAX_SEARCH_RESULTS: usize = 5;
const MAX_CHUNK_CHARS: usize = 800;
const MAX_RESOLVED_FILE_CHARS: usize = 4000;

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

fn resolved_file_block(workspace_root: &Path, conn: &Connection, query: &str) -> Option<String> {
    let result = resolve_file_reference(conn, query).ok()?;
    let path = result.resolved?;
    let mut content = std::fs::read_to_string(workspace_root.join(&path)).ok()?;
    if content.len() > MAX_RESOLVED_FILE_CHARS {
        content.truncate(MAX_RESOLVED_FILE_CHARS);
        content.push_str("\n...(truncated)");
    }
    Some(format!("Resolved \"{query}\" to {path}:\n`\n{content}\n`"))
}

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
    #[test] fn read_memory_context_skips_empty_files() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        assert!(read_memory_context(&dir).is_empty());
        std::fs::write(dir.join(".neuralforge").join("memory").join("decisions.md"),
            "# Decisions\n\nUse SQLite for the local index.").unwrap();
        assert!(read_memory_context(&dir).contains("Use SQLite"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test] fn build_context_prompt_includes_memory() {
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
    #[test] fn build_context_prompt_resolves_file() {
        let dir = temp_workspace();
        std::fs::create_dir_all(dir.join("carina_egti")).unwrap();
        std::fs::write(dir.join("carina_egti").join("ui_car.json"),
            "{\"screen\": \"dashboard\"}").unwrap();
        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();
            let prompt = build_context_prompt(&dir, &conn, "clear the UI JSON for the carina");
            assert!(prompt.contains("dashboard"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test] fn classify_intent_classifies_correctly() {
        assert_eq!(classify_intent("show me the dependency graph"), RetrievalIntent::Structural);
        assert_eq!(classify_intent("architecture of the auth module"), RetrievalIntent::Structural);
        assert_eq!(classify_intent("how does authentication work"), RetrievalIntent::Semantic);
        assert_eq!(classify_intent("fix the login bug"), RetrievalIntent::Semantic);
    }
}