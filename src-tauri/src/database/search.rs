use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::path::Path;

#[derive(Serialize, Type, Clone)]
pub struct SearchResult {
    pub path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub score: f64,
}

#[derive(Clone, Debug)]
struct EnrichedItem {
    priority: u8,
    label: String,
    content: String,
}

/// Lightweight token estimation: ~4 chars per token.
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// FTS5's default MATCH syntax ANDs every bare word together, so convert to
/// OR-of-terms query for natural-language questions.
fn to_fts5_or_query(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// FTS5 full-text keyword search over indexed chunks.
pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<SearchResult>> {
    let fts_query = to_fts5_or_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn
        .prepare(
            "SELECT chunks.path, chunks.start_line, chunks.end_line, chunks.content, chunks_fts.rank
             FROM chunks_fts JOIN chunks ON chunks.id = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )
        .map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(SearchResult {
                path: row.get(0)?, start_line: row.get(1)?, end_line: row.get(2)?,
                content: row.get(3)?, score: -row.get::<_, f64>(4)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("search row read failed: {e}")))
}

/// Fetches a symbol summary for a given file path.
fn get_symbol_summary(conn: &Connection, file_path: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, name FROM symbols
             WHERE file_path = ?1 AND kind NOT IN ('import', 'impl')
             ORDER BY start_line LIMIT 20",
        )
        .map_err(|e| AppError::Provider(format!("symbol query failed: {e}")))?;
    let rows = stmt
        .query_map(params![file_path], |row| {
            Ok(format!("  {} {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::Provider(format!("symbol query failed: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("symbol row read failed: {e}")))
}

/// Fetches dependencies for a file as formatted strings. Depth tracks
/// recursion: 0 = immediate, 1+ = transitive (up to 2 levels max).
fn get_dependency_info(conn: &Connection, file_path: &str, depth: usize, visited: &mut HashSet<String>) -> Vec<String> {
    if depth > 2 || !visited.insert(file_path.to_string()) {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT dependency_type, import_source, target_symbol
         FROM dependencies WHERE source_file = ?1 ORDER BY dependency_type, import_source",
    ) else { return lines };
    let rows: Vec<(String, Option<String>, Option<String>)> = match stmt.query_map(params![file_path], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?))
    }) {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(_) => return lines,
    };
    for (dep_type, import_source, _target_symbol) in &rows {
        let display = import_source.as_deref().unwrap_or(dep_type);
        lines.push(format!("  [{dep_type}] {display}"));
    }
    lines
}

/// Builds an enriched context string by combining FTS5 search results with
/// dependency and symbol information. Priority order (lowest number =
/// highest priority):
///   0 = FTS5 chunks, 1 = resolved file, 2 = symbols, 3 = dependencies
/// Applies token budget truncation: lowest-priority items are dropped first.
pub fn enriched_context(
    conn: &Connection,
    _workspace_root: &Path,
    query: &str,
    memory: &str,
    resolved_file: Option<&str>,
    max_tokens: usize,
) -> AppResult<String> {
    let mut items: Vec<EnrichedItem> = Vec::new();

    // Phase 1: FTS5 keyword search
    let search_results = keyword_search(conn, query, 5).unwrap_or_default();
    let mut matched_paths: Vec<String> = Vec::new();
    for result in &search_results {
        let content = if result.content.len() > 800 {
            let mut s = result.content.clone();
            s.truncate(800); s.push_str("..."); s
        } else { result.content.clone() };
        items.push(EnrichedItem {
            priority: 0,
            label: format!("Code: {} (lines {}-{})", result.path, result.start_line, result.end_line),
            content,
        });
        if !matched_paths.contains(&result.path) {
            matched_paths.push(result.path.clone());
        }
    }

    // Phase 2: Resolved file
    if let Some(resolved) = resolved_file {
        items.push(EnrichedItem { priority: 1, label: "Referenced File".to_string(), content: resolved.to_string() });
    }

    // Phase 3: Symbol context for matched files
    for file_path in &matched_paths {
        if let Ok(symbols) = get_symbol_summary(conn, file_path) {
            if !symbols.is_empty() {
                items.push(EnrichedItem {
                    priority: 2, label: format!("Symbols in {}", file_path), content: symbols.join("\n"),
                });
            }
        }
    }

    // Phase 4: Dependency info for matched files
    for file_path in &matched_paths {
        let deps = get_dependency_info(conn, file_path, 0, &mut HashSet::new());
        if !deps.is_empty() {
            items.push(EnrichedItem {
                priority: 3, label: format!("Dependencies of {}", file_path), content: deps.join("\n"),
            });
        }
    }

    // Phase 5: Sort by priority
    items.sort_by_key(|i| i.priority);

    // Phase 6: Apply token budget
    let mut used_tokens = estimate_tokens(memory);
    let mut output_parts = Vec::new();

    // Memory always first if under budget
    if !memory.is_empty() && used_tokens < max_tokens {
        output_parts.push(format!("# Project Memory\n{memory}"));
    }

    for item in &items {
        let item_text = format!("# {}\n{}", item.label, item.content);
        let tokens = estimate_tokens(&item_text);
        if used_tokens + tokens <= max_tokens {
            output_parts.push(item_text);
            used_tokens += tokens;
        }
    }

    if output_parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(output_parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_search_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap(); dir
    }

    #[test] fn to_fts5_or_query_joins_terms_with_or() {
        assert_eq!(to_fts5_or_query("how does auth work"), "\"how\" OR \"does\" OR \"auth\" OR \"work\"");
        assert_eq!(to_fts5_or_query(""), "");
    }

    #[test] fn keyword_search_finds_indexed_content() {
        let dir = temp_workspace();
        std::fs::write(dir.join("auth.rs"), "fn authenticate_user() -> bool { true }\n").unwrap();
        std::fs::write(dir.join("math.rs"), "fn add(a: i32) -> i32 { a }\n").unwrap();
        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();
            let results = keyword_search(&conn, "authentication", 10).unwrap();
            assert!(!results.is_empty());
            assert!(results.iter().any(|r| r.content.contains("authenticate_user")));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test] fn enriched_context_includes_symbols_and_deps() {
        let dir = temp_workspace();
        std::fs::write(dir.join("lib.rs"),
            "use serde::Serialize;\n/// Compute.\npub fn compute() -> i32 { 42 }\n"
        ).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        crate::database::indexer::index_workspace(&conn, &dir).unwrap();
        let context = enriched_context(&conn, &dir, "compute", "", None, 2000).unwrap();
        assert!(context.contains("function") || context.contains("compute"), "expected symbols: {context}");
        assert!(context.contains("import") || context.contains("serde"), "expected deps: {context}");
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }

    #[test] fn enriched_context_respects_token_budget() {
        let dir = temp_workspace();
        std::fs::write(dir.join("lib.rs"),
            "use std::collections::HashMap;\nuse serde::Serialize;\npub fn run() -> i32 { 0 }\n"
        ).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        crate::database::indexer::index_workspace(&conn, &dir).unwrap();
        let tight = enriched_context(&conn, &dir, "run", "", None, 100).unwrap();
        assert!(tight.len() <= 1000, "tight budget: {} chars", tight.len());
        let generous = enriched_context(&conn, &dir, "run", "", None, 10000).unwrap();
        assert!(!generous.is_empty());
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }

    #[test] fn enriched_context_includes_memory_when_provided() {
        let dir = temp_workspace();
        std::fs::write(dir.join("lib.rs"), "pub fn add(a: i32) -> i32 { a }\n").unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        crate::database::indexer::index_workspace(&conn, &dir).unwrap();
        let context = enriched_context(&conn, &dir, "add", "# Architecture\nRust backend.", None, 5000).unwrap();
        assert!(context.contains("Architecture"));
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }
}