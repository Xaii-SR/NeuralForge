use crate::ai::context::{classify_intent, RetrievalIntent};
use crate::core::errors::{AppError, AppResult};
use once_cell::sync::Lazy;
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

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

#[derive(Serialize, Type, Clone, Debug)]
pub struct ContextDelta {
    pub is_delta: bool,
    pub full_text: String,
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
    pub similarity: f64,
}

static CONTEXT_CACHE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

const MIN_SIMILARITY_FOR_DELTA: f64 = 0.8;
const STITCH_THRESHOLD: i64 = 15;

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
    if fts_query.is_empty() { return Ok(vec![]); }
    let mut stmt = conn.prepare(
        "SELECT chunks.path, chunks.start_line, chunks.end_line, chunks.content, chunks_fts.rank
         FROM chunks_fts JOIN chunks ON chunks.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2"
    ).map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;
    let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
        Ok(SearchResult { path: row.get(0)?, start_line: row.get(1)?, end_line: row.get(2)?, content: row.get(3)?, score: -row.get::<_, f64>(4)? })
    }).map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| AppError::Provider(format!("search row read failed: {e}")))
}

fn get_symbol_summary(conn: &Connection, file_path: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT kind, name FROM symbols WHERE file_path = ?1 AND kind NOT IN ('import', 'impl') ORDER BY start_line LIMIT 20"
    ).map_err(|e| AppError::Provider(format!("symbol query failed: {e}")))?;
    let rows = stmt.query_map(params![file_path], |row| {
        Ok(format!("  {} {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).map_err(|e| AppError::Provider(format!("symbol query failed: {e}")))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| AppError::Provider(format!("symbol row read failed: {e}")))
}

fn get_dependency_info(conn: &Connection, file_path: &str, depth: usize, visited: &mut HashSet<String>) -> Vec<String> {
    if depth > 2 || !visited.insert(file_path.to_string()) { return Vec::new(); }
    let mut lines = Vec::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT dependency_type, import_source, target_symbol FROM dependencies WHERE source_file = ?1 ORDER BY dependency_type, import_source"
    ) else { return lines };
    let rows: Vec<(String, Option<String>, Option<String>)> = match stmt.query_map(params![file_path], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?))
    }) { Ok(r) => r.filter_map(|r| r.ok()).collect(), Err(_) => return lines };
    for (dep_type, import_source, _target_symbol) in &rows {
        lines.push(format!("  [{dep_type}] {}", import_source.as_deref().unwrap_or(dep_type)));
    }
    lines
}

fn get_symbol_boundaries(conn: &Connection, file_path: &str) -> Vec<(i64, i64, String, String)> {
    let mut stmt = match conn.prepare(
        "SELECT start_line, end_line, kind, name FROM symbols WHERE file_path = ?1 AND kind NOT IN ('import') ORDER BY start_line"
    ) {
        Ok(s) => s, Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map(params![file_path], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
    }) { Ok(r) => r, Err(_) => return Vec::new() };
    rows.filter_map(|r| r.ok()).collect()
}

fn prune_blocks(content: &str, max_chars: usize, symbols: &[(i64, i64, String, String)]) -> String {
    if content.len() <= max_chars || symbols.is_empty() { return content.to_string(); }
    let lines: Vec<&str> = content.lines().collect();
    let mut body_lines: HashSet<usize> = HashSet::new();
    for (start, end, _kind, _name) in symbols {
        if *end - *start > 3 { for ln in (*start + 3) as usize..=*end as usize { body_lines.insert(ln); } }
    }
    let mut pruned: Vec<String> = Vec::new();
    let mut in_pruned_block = false;
    for (i, line) in lines.iter().enumerate() {
        if body_lines.contains(&(i + 1)) {
            if !in_pruned_block { pruned.push("    // [body pruned for context budget]".to_string()); in_pruned_block = true; }
        } else { in_pruned_block = false; pruned.push(line.to_string()); }
    }
    let result = pruned.join("\n");
    if result.len() > max_chars { let mut s = result; s.truncate(max_chars); s.push_str("..."); return s; }
    result
}

/// Applies variable-level def-use extraction within function bodies. Scans
/// lines and identifies the definition-use span of a target variable,
/// replacing unrelated interior lines with a placeholder.
fn prune_to_def_use(content: &str, var_name: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut first_def: Option<usize> = None;
    let mut last_use: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().contains(var_name) && !line.trim().starts_with("//") {
            if first_def.is_none() { first_def = Some(i); }
            last_use = Some(i);
        }
    }
    let (start, end) = match (first_def, last_use) {
        (Some(s), Some(e)) if s < e && (e - s) < lines.len() / 2 => (s, e),
        _ => return content.to_string(),
    };
    let mut result: Vec<String> = Vec::new();
    for i in 0..start { result.push(lines[i].to_string()); }
    for i in start..=end { result.push(lines[i].to_string()); }
    for i in (end + 1)..lines.len() {
        if lines[i].trim().starts_with('}') || lines[i].trim().contains("[body pruned") {
            result.push(lines[i].to_string());
        } else {
            result.push("    // [unrelated block scope omitted]".to_string());
        }
    }
    result.join("\n")
}

fn stitch_chunks(results: &[SearchResult]) -> Vec<SearchResult> {
    let mut by_file: HashMap<String, Vec<&SearchResult>> = HashMap::new();
    for r in results { by_file.entry(r.path.clone()).or_default().push(r); }
    let mut stitched: Vec<SearchResult> = Vec::new();
    for (_path, mut chunks) in by_file {
        chunks.sort_by_key(|c| c.start_line);
        let mut merged = chunks[0].clone();
        for chunk in chunks.iter().skip(1) {
            if chunk.start_line <= merged.end_line + STITCH_THRESHOLD {
                let gap = "\n".repeat((chunk.start_line - merged.end_line - 1).max(0) as usize);
                merged.content.push_str(&gap); merged.content.push_str("\n");
                merged.content.push_str(&chunk.content);
                merged.end_line = merged.end_line.max(chunk.end_line);
            } else { stitched.push(merged); merged = (*chunk).clone(); }
        }
        stitched.push(merged);
    }
    stitched
}

/// Computes a line-by-line diff between old and new context strings. Returns a
/// ContextDelta: if similarity >= 80%, returns only changed lines (is_delta=true);
/// otherwise returns the full text (is_delta=false).
pub fn compute_context_diff(old: &str, new: &str) -> ContextDelta {
    if old.is_empty() {
        return ContextDelta { is_delta: false, full_text: new.to_string(), added_lines: vec![], removed_lines: vec![], similarity: 0.0 };
    }
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let old_set: HashSet<&str> = old_lines.iter().copied().collect();
    let new_set: HashSet<&str> = new_lines.iter().copied().collect();
    let added: Vec<String> = new_lines.iter().filter(|l| !old_set.contains(*l)).map(|l| l.to_string()).collect();
    let removed: Vec<String> = old_lines.iter().filter(|l| !new_set.contains(*l)).map(|l| l.to_string()).collect();
    let total = old_lines.len().max(new_lines.len());
    let common = old_lines.iter().filter(|l| new_set.contains(*l)).count();
    let similarity = if total == 0 { 0.0 } else { common as f64 / total as f64 };
    if similarity >= MIN_SIMILARITY_FOR_DELTA {
        ContextDelta { is_delta: true, full_text: String::new(), added_lines: added, removed_lines: removed, similarity }
    } else {
        ContextDelta { is_delta: false, full_text: new.to_string(), added_lines: vec![], removed_lines: vec![], similarity }
    }
}

/// Stores the last context response for delta comparison on subsequent calls.
pub fn cache_context_response(response: &str) {
    if let Ok(mut guard) = CONTEXT_CACHE.lock() { *guard = Some(response.to_string()); }
}

/// Retrieves the previously cached context, if any.
pub fn get_cached_context() -> Option<String> {
    CONTEXT_CACHE.lock().ok().and_then(|g| g.clone())
}

/// Looks up a function symbol by name in the symbols table.
fn get_function_symbol(conn: &Connection, name: &str) -> Option<(String, i64, i64)> {
    conn.query_row(
        "SELECT file_path, start_line, end_line FROM symbols WHERE kind = 'function' AND name = ?1 LIMIT 1",
        params![name],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
    ).ok()
}

/// Finds files whose content references a given function name (callers).
fn get_callers(conn: &Connection, name: &str, exclude_path: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT DISTINCT path FROM chunks WHERE content LIKE ?1 AND path != ?2 LIMIT 10"
    ) {
        Ok(s) => s, Err(_) => return vec![],
    };
    let pattern = format!("%{}%", name);
    let rows = match stmt.query_map(params![pattern, exclude_path], |row| {
        row.get::<_, String>(0)
    }) { Ok(r) => r.filter_map(|r| r.ok()).collect(), Err(_) => return vec![] };
    rows
}

/// Extracts a target variable name from the query (lowercase, 2+ chars, not noise words).
fn extract_target_variable(query: &str) -> Option<String> {
    let words: Vec<&str> = query.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
    let skip = ["the","how","what","when","this","that","from","with","does","work"];
    for w in &words {
        if w.len() >= 2 && w.chars().all(|c| c.is_lowercase() || c == '_') && !skip.contains(w) {
            return Some(w.to_string());
        }
    }
    None
}

/// Determines if a query contains a likely function name (capitalized or followed by parentheses).
fn extract_target_function(query: &str) -> Option<String> {
    let words: Vec<&str> = query.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
    for w in &words {
        if w.ends_with("()") || w.chars().any(|c| c.is_uppercase()) {
            let clean = w.trim_end_matches("()").trim_end_matches('s');
            if clean.len() >= 2 { return Some(clean.to_string()); }
        }
    }
    None
}

pub fn enriched_context(
    conn: &Connection, _workspace_root: &Path, query: &str, memory: &str,
    resolved_file: Option<&str>, max_tokens: usize,
) -> AppResult<String> {
    let _intent = classify_intent(query);
    let target_var = extract_target_variable(query);
    let mut items: Vec<EnrichedItem> = Vec::new();

    let search_results = keyword_search(conn, query, 5).unwrap_or_default();
    let stitched = stitch_chunks(&search_results);
    let mut matched_paths: Vec<String> = Vec::new();
    for result in &stitched {
        let symbols = get_symbol_boundaries(conn, &result.path);
        let mut content = prune_blocks(&result.content, 800, &symbols);
        if let Some(ref var) = target_var { content = prune_to_def_use(&content, var); }
        items.push(EnrichedItem { priority: 0, label: format!("Code: {} (lines {}-{})", result.path, result.start_line, result.end_line), content });
        if !matched_paths.contains(&result.path) { matched_paths.push(result.path.clone()); }
    }

    if let Some(resolved) = resolved_file { items.push(EnrichedItem { priority: 1, label: "Referenced File".to_string(), content: resolved.to_string() }); }
    for file_path in &matched_paths {
        if let Ok(symbols) = get_symbol_summary(conn, file_path) {
            if !symbols.is_empty() { items.push(EnrichedItem { priority: 2, label: format!("Symbols in {}", file_path), content: symbols.join("\n") }); }
        }
    }

    let mut global_visited: HashSet<String> = HashSet::new();
    for file_path in &matched_paths {
        if global_visited.contains(file_path) { continue; }
        let deps = get_dependency_info(conn, file_path, 0, &mut global_visited);
        if !deps.is_empty() { items.push(EnrichedItem { priority: 3, label: format!("Dependencies of {}", file_path), content: deps.join("\n") }); }
    }

    // Phase 5: Call-graph priority scoring
    let target_fn = extract_target_function(query);
    let callers = target_fn.as_ref().and_then(|name| {
        let (def_path, _, _) = get_function_symbol(conn, name)?;
        Some(get_callers(conn, name, &def_path))
    }).unwrap_or_default();

    if let Some(name) = &target_fn {
        if let Some((def_path, _, _)) = get_function_symbol(conn, name) {
            for item in &mut items {
                if callers.contains(&item.label.split(' ').nth(1).unwrap_or("").to_string()) {
                    // Direct caller: 1.35x boost (lower effective priority number = higher rank)
                    item.priority = (item.priority as f64 / 1.35) as u8;
                }
                if item.label.contains(&def_path) {
                    // Definition file: 1.2x boost
                    item.priority = (item.priority as f64 / 1.2) as u8;
                }
            }
        }
    }

    items.sort_by_key(|i| i.priority);
    let mut used_tokens = estimate_tokens(memory);
    let mut output_parts = Vec::new();
    if !memory.is_empty() && used_tokens < max_tokens { output_parts.push(format!("# Project Memory\n{memory}")); }
    for item in &items {
        let t = format!("# {}\n{}", item.label, item.content);
        let tk = estimate_tokens(&t);
        if used_tokens + tk <= max_tokens { output_parts.push(t); used_tokens += tk; }
    }
    Ok(if output_parts.is_empty() { String::new() } else { output_parts.join("\n\n") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn tw() -> std::path::PathBuf {
        let mut d = std::env::temp_dir(); let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        d.push(format!("nf_search_{n}")); std::fs::create_dir_all(&d).unwrap(); d
    }
    #[test] fn fts5_joins() { assert_eq!(to_fts5_or_query("how does auth work"), "\"how\" OR \"does\" OR \"auth\" OR \"work\""); assert_eq!(to_fts5_or_query(""), ""); }
    #[test] fn keyword_finds() {
        let d = tw(); std::fs::write(d.join("auth.rs"), "fn authenticate_user() -> bool { true }").unwrap();
        { let c = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&c, &d).unwrap(); let r = keyword_search(&c, "authentication", 10).unwrap(); assert!(!r.is_empty()); } std::fs::remove_dir_all(&d).ok();
    }
    #[test] fn enriched_has_syms_deps() {
        let d = tw(); std::fs::write(d.join("lib.rs"), "use serde::Serialize;\npub fn compute()->i32{42}\n").unwrap();
        let c = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&c, &d).unwrap();
        let ctx = enriched_context(&c, &d, "compute", "", None, 2000).unwrap(); assert!(ctx.contains("compute")); assert!(ctx.contains("serde")); drop(c); std::fs::remove_dir_all(&d).ok();
    }
    #[test] fn enriched_budget() {
        let d = tw(); std::fs::write(d.join("lib.rs"), "pub fn run()->i32{0}\n").unwrap();
        let c = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&c, &d).unwrap();
        let t = enriched_context(&c, &d, "run", "", None, 100).unwrap(); assert!(t.len() <= 1000); drop(c); std::fs::remove_dir_all(&d).ok();
    }
    #[test] fn enriched_memory() {
        let d = tw(); std::fs::write(d.join("lib.rs"), "pub fn add(a:i32)->i32{a}\n").unwrap();
        let c = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&c, &d).unwrap();
        let ctx = enriched_context(&c, &d, "add", "# Architecture\nRust backend.", None, 5000).unwrap(); assert!(ctx.contains("Architecture")); drop(c); std::fs::remove_dir_all(&d).ok();
    }
    #[test] fn prune_sigs() {
        let content = "pub fn complex(a: i32, b: i32) -> i32 {\n    let x = a + b;\n    let y = x * 2;\n    let z = y / 3;\n    z\n}\n";
        let syms = vec![(1i64, 6i64, "function".to_string(), "complex".to_string())];
        let p = prune_blocks(content, 100, &syms); assert!(p.contains("pub fn complex")); assert!(p.contains("[body pruned"));
    }
    #[test] fn prune_short() { assert_eq!(prune_blocks("fn short() {}", 200, &[]), "fn short() {}"); }
    #[test] fn stitch_merges_adjacent() {
        let r = vec![
            SearchResult { path: "lib.rs".to_string(), start_line: 1, end_line: 10, content: "aaa".to_string(), score: 1.0 },
            SearchResult { path: "lib.rs".to_string(), start_line: 15, end_line: 25, content: "bbb".to_string(), score: 0.8 },
        ];
        let s = stitch_chunks(&r); assert_eq!(s.len(), 1); assert!(s[0].content.contains("aaa")); assert!(s[0].content.contains("bbb"));
    }
    #[test] fn stitch_keeps_distant() {
        let r = vec![
            SearchResult { path: "lib.rs".to_string(), start_line: 1, end_line: 10, content: "aaa".to_string(), score: 1.0 },
            SearchResult { path: "lib.rs".to_string(), start_line: 50, end_line: 60, content: "bbb".to_string(), score: 0.8 },
        ];
        assert_eq!(stitch_chunks(&r).len(), 2);
    }
    #[test] fn diff_full_when_empty_cache() {
        let delta = compute_context_diff("", "hello world");
        assert!(!delta.is_delta);
        assert_eq!(delta.full_text, "hello world");
    }
    #[test] fn diff_delta_when_mostly_same() {
        let old = "line1\nline2\nline3\nline4";
        let new = "line1\nline2\nline3\nline4\nline5";
        let delta = compute_context_diff(old, new);
        assert!(delta.is_delta);
        assert_eq!(delta.added_lines, vec!["line5"]);
        assert!(delta.removed_lines.is_empty());
    }
    #[test] fn diff_full_when_very_different() {
        let old = "aaaa";
        let new = "bbbb";
        let delta = compute_context_diff(old, new);
        assert!(!delta.is_delta);
        assert_eq!(delta.full_text, "bbbb");
    }
    #[test] fn def_use_isolates_var_span() {
        let content = "pub fn process() {\n    let x = 1;\n    let y = x + 2;\n    let z = y * 3;\n    println!(\"{}\", z);\n}\n";
        let pruned = prune_to_def_use(content, "y");
        assert!(pruned.contains("let y = x + 2"), "def line should be kept");
        assert!(pruned.contains("let z = y * 3"), "use line should be kept");
        assert!(pruned.contains("[unrelated block scope omitted]"), "unrelated lines should be marked");
    }
    #[test] fn def_use_no_var_unchanged() {
        let content = "pub fn run() {\n    let a = 1;\n    let b = 2;\n}\n";
        assert_eq!(prune_to_def_use(content, "nonexistent"), content, "no match should return unchanged");
    }
    #[test] fn def_use_var_single_occurrence() {
        let content = "pub fn calc() {\n    let result = 42;\n    return result;\n}\n";
        let pruned = prune_to_def_use(content, "result");
        assert!(pruned.contains("let result = 42"), "single occurrence def should be kept");
    }
}
