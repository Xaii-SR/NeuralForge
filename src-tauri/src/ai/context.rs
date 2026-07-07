use crate::core::config::{MEMORY_DIR_NAME, MEMORY_FILES, MEMORY_SUBDIR_NAME};
use crate::database::search::{keyword_search, SearchResult};
use rusqlite::Connection;
use std::path::Path;

const MAX_SEARCH_RESULTS: usize = 5;
const MAX_CHUNK_CHARS: usize = 800;

/// Reads .neuralforge/memory/*.md and concatenates non-empty files into a
/// single context block. Missing files (workspace never opened through
/// ensure_memory_scaffold, or a file was deleted) are silently skipped -
/// memory injection is best-effort context, not a hard requirement.
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

fn format_search_results(results: &[SearchResult]) -> String {
    results
        .iter()
        .map(|r| {
            let mut content = r.content.clone();
            if content.len() > MAX_CHUNK_CHARS {
                content.truncate(MAX_CHUNK_CHARS);
                content.push_str("...");
            }
            format!("### {} (lines {}-{})\n```\n{}\n```", r.path, r.start_line, r.end_line, content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Combines project memory (architecture/decisions/rules/etc.) with the top
/// keyword-search matches for the user's query into a single context block
/// intended to be sent as a system message ahead of the user's actual
/// question. This is "prompt management" at foundation-phase scope: no
/// token-budget trimming or ranking beyond FTS5's own relevance order yet.
pub fn build_context_prompt(workspace_root: &Path, conn: &Connection, query: &str) -> String {
    let memory = read_memory_context(workspace_root);
    let search_results = keyword_search(conn, query, MAX_SEARCH_RESULTS).unwrap_or_default();
    let code_context = format_search_results(&search_results);

    let mut parts = Vec::new();
    parts.push(
        "You are an AI assistant embedded in the NeuralForge IDE. Use the following project context to answer the user's question. If the context isn't relevant, answer normally.".to_string(),
    );
    if !memory.is_empty() {
        parts.push(format!("# Project Memory\n{memory}"));
    }
    if !code_context.is_empty() {
        parts.push(format!("# Relevant Code\n{code_context}"));
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_context_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_memory_context_skips_empty_and_header_only_files() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();

        let context = read_memory_context(&dir);
        assert!(context.is_empty(), "freshly scaffolded memory files are header-only, expected no context");

        std::fs::write(
            dir.join(".neuralforge").join("memory").join("decisions.md"),
            "# Decisions\n\nUse SQLite for the local index.",
        )
        .unwrap();

        let context = read_memory_context(&dir);
        assert!(context.contains("Use SQLite for the local index"));
        assert!(context.contains("decisions.md"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_context_prompt_includes_memory_and_search_results() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        std::fs::write(
            dir.join(".neuralforge").join("memory").join("architecture.md"),
            "# Architecture\n\nBackend is Rust/Tauri, frontend is Next.js.",
        )
        .unwrap();
        std::fs::write(dir.join("auth.rs"), "fn authenticate_user() -> bool {\n    true\n}\n").unwrap();

        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();

            let prompt = build_context_prompt(&dir, &conn, "how does authentication work");
            assert!(prompt.contains("Rust/Tauri"));
            assert!(prompt.contains("authenticate_user"));
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
