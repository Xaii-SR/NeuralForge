use crate::ai::context::{classify_intent, RetrievalIntent};
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
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

fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

fn to_fts5_or_query(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

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

/// Loads symbol boundaries (start_line, end_line, kind, name) for AST-guided
/// pruning. Uses owned types to avoid rusqlite borrow lifetime issues.
fn get_symbol_boundaries(conn: &Connection, file_path: &str) -> Vec<(i64, i64, String, String)> {
    let mut stmt = match conn.prepare(
        "SELECT start_line, end_line, kind, name FROM symbols
         WHERE file_path = ?1 AND kind NOT IN ('import')
         ORDER BY start_line"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map(params![file_path], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// Prunes function/struct bodies from content, replacing inner implementation
/// lines with a placeholder while preserving signatures and doc comments.
fn prune_blocks(content: &str, max_chars: usize, symbols: &[(i64, i64, String, String)]) -> String {
    if content.len() <= max_chars || symbols.is_empty() {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    let mut body_lines: HashSet<usize> = HashSet::new();
    for (start, end, _kind, _name) in symbols {
        if *end - *start > 3 {
            for ln in (*start + 3) as usize..=*end as usize {
                body_lines.insert(ln);
            }
        }
    }
    let mut pruned: Vec<String> = Vec::new();
    let mut in_pruned_block = false;
    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        if body_lines.contains(&line_num) {
            if !in_pruned_block {
                pruned.push("    // [body pruned for context budget]".to_string());
                in_pruned_block = true;
            }
        } else {
            in_pruned_block = false;
            pruned.push(line.to_string());
        }
    }
    let result = pruned.join("\n");
    if result.len() > max_chars {
        let mut s = result;
        s.truncate(max_chars);
        s.push_str("...");
        return s;
    }
    result
}

pub fn enriched_context(
    conn: &Connection,
    _workspace_root: &Path,
    query: &str,
    memory: &str,
    resolved_file: Option<&str>,
    max_tokens: usize,
) -> AppResult<String> {
    let _intent = classify_intent(query);
    let mut items: Vec<EnrichedItem> = Vec::new();

    // Phase 1: FTS5 keyword search with AST-guided body pruning
    let search_results = keyword_search(conn, query, 5).unwrap_or_default();
    let mut matched_paths: Vec<String> = Vec::new();
    for result in &search_results {
        let symbols = get_symbol_boundaries(conn, &result.path);
        let content = prune_blocks(&result.content, 800, &symbols);
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

    // Phase 4: Dependency info for matched files with cycle-safe global visited set
    let mut global_visited: HashSet<String> = HashSet::new();
    for file_path in &matched_paths {
        if global_visited.contains(file_path) {
            continue;
        }
        let deps = get_dependency_info(conn, file_path, 0, &mut global_visited);
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

    Ok(if output_parts.is_empty() {
        String::new()
    } else {
        output_parts.join("\n\n")
    })
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
            "use serde::Serialize;\npub fn compute() -> i32 { 42 }\n"
        ).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        crate::database::indexer::index_workspace(&conn, &dir).unwrap();
        let context = enriched_context(&conn, &dir, "compute", "", None, 2000).unwrap();
        assert!(context.contains("compute"), "expected symbols: {context}");
        assert!(context.contains("serde"), "expected deps: {context}");
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }

    #[test] fn enriched_context_respects_token_budget() {
        let dir = temp_workspace();
        std::fs::write(dir.join("lib.rs"),
            "use std::collections::HashMap;\npub fn run() -> i32 { 0 }\n"
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

    #[test] fn prune_blocks_preserves_signatures() {
        let content = "pub fn complex(a: i32, b: i32) -> i32 {\n    let x = a + b;\n    let y = x * 2;\n    let z = y / 3;\n    z\n}\n";
        let symbols = vec![(1i64, 6i64, "function".to_string(), "complex".to_string())];
        // Force pruning: budget < content length (130) but > pruned result length (~94)
        let pruned = prune_blocks(content, 100, &symbols);
        assert!(pruned.contains("pub fn complex"), "signature should be preserved");
        assert!(pruned.contains("[body pruned"), "body should be replaced with placeholder");
    }

    #[test] fn prune_blocks_short_content_unchanged() {
        let content = "fn short() {}";
        let pruned = prune_blocks(content, 200, &[]);
        assert_eq!(pruned, content, "short content without symbols should be unchanged");
    }
}