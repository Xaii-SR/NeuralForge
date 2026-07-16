use crate::agent_controller::AgentContext;
use crate::database;
use crate::database::search::SearchResult;
use crate::workspace_scanner::{self, ScanResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalMeta {
    pub query: String,
    pub ranked_files: Vec<RankedFile>,
    pub selected_context: String,
    pub total_files_scanned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedFile {
    pub path: String,
    pub language: String,
    pub priority: u8,
    pub reason: String,
    pub matched_symbols: Vec<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub workspace_root: PathBuf,
    pub query: String,
    pub max_files: usize,
    pub include_symbols: bool,
    pub include_snippets: bool,
}

impl Default for ContextRequest {
    fn default() -> Self { Self { workspace_root: PathBuf::from("."), query: String::new(), max_files: 10, include_symbols: true, include_snippets: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub qualified_name: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefactoringKind { RenameSymbol, MoveModule, UpdateImports, SignatureChange, ApiMigration }

pub struct ContextRetrieval;

impl ContextRetrieval {
    pub fn retrieve(request: &ContextRequest) -> Result<Vec<RankedFile>, String> {
        let root = &request.workspace_root;
        if !root.exists() || !root.is_dir() { return Err("Workspace root does not exist".to_string()); }
        let scan = workspace_scanner::scan_workspace(root)?;
        Ok(Self::rank_files(&request.query, &scan, request.max_files))
    }

    pub fn retrieve_with_db(conn: &rusqlite::Connection, _workspace_root: &Path, query: &str, max_files: usize) -> Result<(Vec<RankedFile>, Vec<SearchResult>), String> {
        let raw_results = database::search::keyword_search(conn, query, 20).map_err(|e| format!("Search failed: {e}"))?;
        let symbol_matches = Self::find_symbols(conn, query, 20).unwrap_or_default();
        let mut ranked = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut adjusted: Vec<SearchResult> = Vec::new();

        // First pass: symbol-aware ranking — symbols get highest priority
        for sm in &symbol_matches {
            if ranked.len() >= max_files { break; }
            if seen_paths.contains(&sm.file_path) { continue; }
            seen_paths.insert(sm.file_path.clone());

            if let Ok(content) = conn.query_row("SELECT content FROM chunks WHERE path = ?1 AND content LIKE ?2 LIMIT 1", rusqlite::params![sm.file_path, format!("%{}%", sm.name)], |r| r.get::<_, String>(0)) {
                ranked.push(RankedFile {
                    path: sm.file_path.clone(),
                    language: classify_from_path(&sm.file_path).to_string(),
                    priority: 100,
                    reason: format!("Symbol match: {} {} ({}:{})", sm.kind, sm.name, sm.file_path, sm.start_line),
                    matched_symbols: vec![format!("{} {} @ line {}", sm.kind, sm.qualified_name, sm.start_line)],
                    snippet: content.lines().take(10).collect::<Vec<_>>().join("\n"),
                });
            }
        }

        // Second pass: FTS5 keyword results (lower priority, deduplicated)
        for result in &raw_results {
            if ranked.len() >= max_files { break; }
            if seen_paths.contains(&result.path) { adjusted.push(result.clone()); continue; }
            seen_paths.insert(result.path.clone());
            let language = classify_from_path(&result.path);
            let symbols: Vec<String> = symbol_matches.iter().filter(|s| s.file_path == result.path).map(|s| format!("{} {} @ line {}", s.kind, s.name, s.start_line)).collect();
            ranked.push(RankedFile {
                path: result.path.clone(),
                language: language.to_string(),
                priority: if symbols.is_empty() { 60 } else { 80 },
                reason: if symbols.is_empty() { "Content keyword match".into() } else { format!("{} symbol(s) matched", symbols.len()) },
                matched_symbols: symbols,
                snippet: result.content.lines().take(10).collect::<Vec<_>>().join("\n"),
            });
            adjusted.push(result.clone());
        }

        Ok((ranked, adjusted))
    }

    /// Query the symbols table for matches on name, kind, or qualified_name.
    pub fn find_symbols(conn: &rusqlite::Connection, name: &str, limit: usize) -> Result<Vec<SymbolMatch>, String> {
        let pattern = format!("%{}%", name);
        let mut stmt = conn.prepare(
            "SELECT name, kind, file_path, start_line, end_line, qualified_name, signature FROM symbols WHERE name LIKE ?1 OR qualified_name LIKE ?1 ORDER BY start_line LIMIT ?2"
        ).map_err(|e| format!("symbol query: {e}"))?;
        let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
            Ok(SymbolMatch {
                name: row.get(0)?, kind: row.get(1)?, file_path: row.get(2)?,
                start_line: row.get(3)?, end_line: row.get(4)?,
                qualified_name: row.get(5)?, signature: row.get(6)?,
            })
        }).map_err(|e| format!("symbol map: {e}"))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Find definition: returns the SymbolMatch for a given symbol name.
    pub fn find_definition(conn: &rusqlite::Connection, name: &str) -> Result<Option<SymbolMatch>, String> {
        Self::find_symbols(conn, name, 1).map(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) })
    }

    /// Find references: returns files containing references to a symbol.
    pub fn find_references(conn: &rusqlite::Connection, name: &str, exclude_path: Option<&str>) -> Result<Vec<String>, String> {
        let pattern = format!("%{}%", name);
        let mut results = Vec::new();
        if let Some(ep) = exclude_path {
            let mut stmt = conn.prepare("SELECT DISTINCT path FROM chunks WHERE content LIKE ?1 AND path != ?2 LIMIT 20")
                .map_err(|e| format!("ref query: {e}"))?;
            let rows = stmt.query_map(rusqlite::params![pattern, ep], |r| r.get::<_, String>(0))
                .map_err(|e| format!("ref map: {e}"))?;
            for r in rows { if let Ok(p) = r { results.push(p); } }
        } else {
            let mut stmt = conn.prepare("SELECT DISTINCT path FROM chunks WHERE content LIKE ?1 LIMIT 20")
                .map_err(|e| format!("ref query: {e}"))?;
            let rows = stmt.query_map(rusqlite::params![pattern], |r| r.get::<_, String>(0))
                .map_err(|e| format!("ref map: {e}"))?;
            for r in rows { if let Ok(p) = r { results.push(p); } }
        }
        Ok(results)
    }

    /// Find implementations: struct/enum/class/trait impls for a symbol.
    pub fn find_implementations(conn: &rusqlite::Connection, name: &str) -> Result<Vec<String>, String> {
        let pattern = format!("%impl%{}%", name);
        let mut stmt = conn.prepare("SELECT DISTINCT path FROM chunks WHERE content LIKE ?1 LIMIT 10")
            .map_err(|e| format!("impl query: {e}"))?;
        let rows = stmt.query_map(rusqlite::params![pattern], |r| r.get::<_, String>(0))
            .map_err(|e| format!("impl map: {e}"))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Build a lightweight repository graph from dependencies table.
    pub fn build_repository_graph(conn: &rusqlite::Connection) -> Result<HashMap<String, Vec<String>>, String> {
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        let mut stmt = conn.prepare("SELECT source_file, target_file, dependency_type, import_source FROM dependencies ORDER BY source_file")
            .map_err(|e| format!("graph query: {e}"))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?))
        }).map_err(|e| format!("graph map: {e}"))?;
        for r in rows {
            if let Ok((src, tgt, dep_type, _)) = r {
                let src2 = src.clone();
                graph.entry(src).or_default().push(dep_type);
                if let Some(t) = tgt { graph.entry(t).or_default().push(format!("depends_on:{}", src2)); }
            }
        }
        Ok(graph)
    }

    /// Multi-file refactoring plan: given a symbol to rename, find all references and generate subtasks.
    pub fn plan_rename_refactor(conn: &rusqlite::Connection, symbol_name: &str) -> Result<Vec<String>, String> {
        let def = Self::find_definition(conn, symbol_name)?.ok_or(format!("Symbol '{}' not found", symbol_name))?;
        let refs = Self::find_references(conn, symbol_name, Some(&def.file_path))?;
        let mut steps = vec![format!("Rename definition of `{}` in {}", symbol_name, def.file_path)];
        for file in &refs { steps.push(format!("Update references to `{}` in {}", symbol_name, file)); }
        Ok(steps)
    }

    fn rank_files(query: &str, scan: &ScanResult, max_files: usize) -> Vec<RankedFile> {
        let ql = query.to_lowercase();
        let words: Vec<&str> = ql.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
        let mut scored: Vec<(u8, &PathBuf, String, Vec<String>)> = Vec::new();
        for file in &scan.files {
            let rel = file.to_string_lossy().to_string();
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or(&rel).to_lowercase();
            let lang = classify_from_path(&rel).to_string();
            let mut score: u8 = 0;
            if name.contains(&ql) { score = 100; }
            for w in &words { if rel.to_lowercase().contains(w) { score = score.max(90); } }
            let hints: [(&str, &[&str]); 3] = [("Rust", &["rs"][..]), ("TypeScript", &["ts", "tsx"]), ("Python", &["py"])];
            for (l, exts) in &hints { if exts.iter().any(|e| ql.contains(e)) && lang == *l { score = score.max(85); } }
            if score == 0 && scored.len() >= max_files { continue; }
            if score == 0 { score = 1; }
            let matching: Vec<String> = words.iter().filter(|w| name.contains(*w)).map(|w| w.to_string()).collect();
            scored.push((score, file, lang, matching));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0)); scored.truncate(max_files);
        scored.into_iter().map(|(priority, file, language, symbols)| {
            RankedFile { path: file.to_string_lossy().to_string(), language, priority, reason: Self::build_reason(priority, &symbols), matched_symbols: symbols, snippet: String::new() }
        }).collect()
    }

    fn compute_priority(path: &str, query: &str, symbols: &[String]) -> u8 {
        let pl = path.to_lowercase(); let ql = query.to_lowercase();
        let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path).to_lowercase();
        if name.contains(&ql) { 100 }
        else if pl.contains(&ql) { 90 }
        else if symbols.iter().any(|s| s.to_lowercase().contains(&ql)) { 80 }
        else { 50 }
    }

    fn build_reason(priority: u8, symbols: &[String]) -> String {
        match priority {
            100 => "Exact filename match".into(), 90 => "Path component match".into(),
            85 => "Language match".into(), 80 => format!("Symbol match: {}", symbols.first().unwrap_or(&String::new())),
            1 => "Low-relevance candidate".into(), _ => format!("Priority {}", priority),
        }
    }
}

fn classify_from_path(path: &str) -> &'static str {
    let p = Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some("rs") => "Rust", Some("ts"|"tsx") => "TypeScript", Some("js"|"jsx") => "JavaScript",
        Some("py") => "Python", Some("json") => "JSON", Some("yaml"|"yml") => "YAML",
        Some("toml") => "TOML", Some("md"|"mdx") => "Markdown", Some("css"|"scss"|"less") => "CSS",
        Some("html"|"htm") => "HTML", Some("sql") => "SQL", Some("sh"|"bash") => "Shell", _ => "Other",
    }
}

pub fn analyze_workspace_for_agent(ctx: &mut AgentContext, query: &str, conn: Option<&rusqlite::Connection>) -> Result<Vec<RankedFile>, String> {
    let root = ctx.workspace_root.clone();
    let request = ContextRequest { workspace_root: root.clone(), query: query.to_string(), max_files: 10, include_symbols: true, include_snippets: true };
    let ranked = if let Some(conn) = conn {
        let (files, _) = ContextRetrieval::retrieve_with_db(conn, &root, query, 10)?; files
    } else { ContextRetrieval::retrieve(&request)? };
    let file_paths: Vec<String> = ranked.iter().map(|r| r.path.clone()).collect();
    crate::agent_controller::AgentController::analyze(ctx, file_paths);
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn td() -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("nf_ctx_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&d).unwrap(); d
    }
    #[test] fn ranks_files_by_name_match() {
        let d = td(); fs::create_dir_all(d.join("src")).unwrap(); fs::write(d.join("src/auth.rs"), "fn authenticate() {}").unwrap(); fs::write(d.join("config.json"),"{}").unwrap(); fs::write(d.join("main.ts"),"console.log('hi')").unwrap();
        let scan = workspace_scanner::scan_workspace(&d).unwrap(); let results = ContextRetrieval::rank_files("auth", &scan, 10);
        assert!(!results.is_empty()); assert!(results[0].path.contains("auth.rs")); fs::remove_dir_all(&d).ok();
    }
    #[test] fn language_detection() { assert_eq!(classify_from_path("main.rs"), "Rust"); assert_eq!(classify_from_path("app.tsx"), "TypeScript"); }
    #[test] fn analyze_updates_context() {
        let d = td(); fs::write(d.join("lib.rs"), "pub fn run() {}").unwrap(); let mut ctx = AgentContext::new("t","Improve",d.clone());
        let _ = analyze_workspace_for_agent(&mut ctx, "improve", None).unwrap();
        assert!(!ctx.relevant_files.is_empty()); fs::remove_dir_all(&d).ok();
    }
    #[test] fn find_symbols_via_db() {
        let d = td(); fs::write(d.join("lib.rs"),"pub fn compute()->i32{42}\npub struct User{pub name:String}\n").unwrap();
        let conn = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&conn, &d).unwrap();
        let symbols = ContextRetrieval::find_symbols(&conn, "compute", 10).unwrap(); assert!(!symbols.is_empty()); assert_eq!(symbols[0].kind, "function");
        let syms2 = ContextRetrieval::find_symbols(&conn, "User", 10).unwrap(); assert!(!syms2.is_empty()); assert_eq!(syms2[0].kind, "struct");
        drop(conn); fs::remove_dir_all(&d).ok();
    }
    #[test] fn find_definition_and_references() {
        let d = td(); fs::write(d.join("lib.rs"),"pub fn authenticate()->bool{true}\n").unwrap(); fs::write(d.join("main.rs"),"mod lib;\nfn main(){authenticate();}\n").unwrap();
        let conn = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&conn, &d).unwrap();
        let def = ContextRetrieval::find_definition(&conn, "authenticate").unwrap(); assert!(def.is_some()); assert_eq!(def.unwrap().file_path, "lib.rs");
        let refs = ContextRetrieval::find_references(&conn, "authenticate", Some("lib.rs")).unwrap();
        assert!(refs.contains(&"main.rs".to_string())); drop(conn); fs::remove_dir_all(&d).ok();
    }
    #[test] fn rename_refactor_plan() {
        let d = td(); fs::write(d.join("auth.rs"),"pub fn login()->bool{true}\n").unwrap(); fs::write(d.join("main.rs"),"mod auth;\nfn main(){auth::login();}\n").unwrap();
        let conn = crate::database::open_for_workspace(&d).unwrap(); crate::database::indexer::index_workspace(&conn, &d).unwrap();
        let plan = ContextRetrieval::plan_rename_refactor(&conn, "login").unwrap();
        assert!(plan.len() >= 2); assert!(plan[0].contains("auth.rs")); assert!(plan[1].contains("main.rs"));
        drop(conn); fs::remove_dir_all(&d).ok();
    }
}