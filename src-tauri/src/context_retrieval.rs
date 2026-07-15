use crate::agent_controller::AgentContext;
use crate::database;
use crate::database::search::SearchResult;
use crate::workspace_scanner::{self, ScanResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Context retrieval metadata stored in the AgentContext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalMeta {
    pub query: String,
    pub ranked_files: Vec<RankedFile>,
    pub selected_context: String,
    pub total_files_scanned: usize,
}

/// A file relevant to a query, with a priority score and rationale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedFile {
    pub path: String,
    pub language: String,
    pub priority: u8,
    pub reason: String,
    pub matched_symbols: Vec<String>,
    pub snippet: String,
}

/// Combined context request for the AI pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub workspace_root: PathBuf,
    pub query: String,
    pub max_files: usize,
    pub include_symbols: bool,
    pub include_snippets: bool,
}

impl Default for ContextRequest {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            query: String::new(),
            max_files: 10,
            include_symbols: true,
            include_snippets: true,
        }
    }
}

/// Context search engine wrapping database indexer and workspace scanner.
pub struct ContextRetrieval;

impl ContextRetrieval {
    /// Scans workspace, indexes if needed, and ranks files by relevance.
    pub fn retrieve(request: &ContextRequest) -> Result<Vec<RankedFile>, String> {
        let root = &request.workspace_root;
        if !root.exists() || !root.is_dir() {
            return Err("Workspace root does not exist".to_string());
        }

        let scan = workspace_scanner::scan_workspace(root)?;
        let ranked = Self::rank_files(&request.query, &scan, request.max_files);
        Ok(ranked)
    }

    /// Retrieves context with full database support when a connection is available.
    pub fn retrieve_with_db(
        conn: &rusqlite::Connection,
        _workspace_root: &Path,
        query: &str,
        max_files: usize,
    ) -> Result<(Vec<RankedFile>, Vec<SearchResult>), String> {
        let raw_results = database::search::keyword_search(conn, query, 20)
            .map_err(|e| format!("Search failed: {e}"))?;

        let mut ranked = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut adjusted_results: Vec<SearchResult> = Vec::new();

        for result in &raw_results {
            if ranked.len() >= max_files {
                break;
            }
            if seen_paths.contains(&result.path) {
                adjusted_results.push(SearchResult {
                    path: result.path.clone(),
                    start_line: result.start_line,
                    end_line: result.end_line,
                    content: result.content.clone(),
                    score: result.score,
                });
                continue;
            }
            seen_paths.insert(result.path.clone());

            let language = classify_from_path(&result.path);

            let symbols: Vec<String> = result
                .content
                .lines()
                .filter(|l| l.contains("fn ") || l.contains("struct ") || l.contains("pub "))
                .take(5)
                .map(|l| l.trim().to_string())
                .collect();

            let priority = Self::compute_priority(&result.path, query, &symbols);

            ranked.push(RankedFile {
                path: result.path.clone(),
                language: language.to_string(),
                priority,
                reason: Self::build_reason(priority, &symbols),
                matched_symbols: symbols,
                snippet: result.content.lines().take(10).collect::<Vec<_>>().join("\n"),
            });

            adjusted_results.push(SearchResult {
                path: result.path.clone(),
                start_line: result.start_line,
                end_line: result.end_line,
                content: result.content.clone(),
                score: result.score,
            });
        }

        Ok((ranked, adjusted_results))
    }

    fn rank_files(query: &str, scan: &ScanResult, max_files: usize) -> Vec<RankedFile> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
            .collect();

        let mut scored: Vec<(u8, &PathBuf, String, Vec<String>)> = Vec::new();

        for file in &scan.files {
            let rel = file.to_string_lossy().to_string();
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&rel)
                .to_lowercase();
            let language = classify_from_path(&rel).to_string();
            let mut score: u8 = 0;

            if name.contains(&query_lower) {
                score = 100;
            }
            for word in &query_words {
                if rel.to_lowercase().contains(word) {
                    score = score.max(90);
                }
            }

            let lang_hints: HashMap<&str, &[&str]> = [
                ("Rust", &["rs"][..]),
                ("TypeScript", &["ts", "tsx"]),
                ("Python", &["py"]),
            ]
            .into_iter()
            .collect();

            for (lang, extensions) in &lang_hints {
                if extensions.iter().any(|e| query_lower.contains(e)) && language == *lang {
                    score = score.max(85);
                }
            }

            if score == 0 && scored.len() >= max_files {
                continue;
            }
            if score == 0 {
                score = 1;
            }

            let matching: Vec<String> = query_words
                .iter()
                .filter(|w| name.contains(*w))
                .map(|w| w.to_string())
                .collect();

            scored.push((score, file, language, matching));
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(max_files);

        scored
            .into_iter()
            .map(|(priority, file, language, symbols)| {
                let reason = Self::build_reason(priority, &symbols);
                RankedFile {
                    path: file.to_string_lossy().to_string(),
                    language,
                    priority,
                    reason,
                    matched_symbols: symbols,
                    snippet: String::new(),
                }
            })
            .collect()
    }

    fn compute_priority(path: &str, query: &str, symbols: &[String]) -> u8 {
        let path_lower = path.to_lowercase();
        let query_lower = query.to_lowercase();
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_lowercase();

        if name.contains(&query_lower) {
            return 100;
        }
        if path_lower.contains(&query_lower) {
            return 90;
        }
        for sym in symbols {
            if sym.to_lowercase().contains(&query_lower) {
                return 80;
            }
        }
        50
    }

    fn build_reason(priority: u8, symbols: &[String]) -> String {
        match priority {
            100 => "Exact filename match".to_string(),
            90 => "Path component match".to_string(),
            85 => "Language match".to_string(),
            80 => format!("Symbol match: {}", symbols.first().unwrap_or(&String::new())),
            1 => "Low-relevance candidate".to_string(),
            _ => format!("Matched with priority {}", priority),
        }
    }
}

fn classify_from_path(path: &str) -> &'static str {
    let p = Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some("rs") => "Rust",
        Some("ts" | "tsx") => "TypeScript",
        Some("js" | "jsx") => "JavaScript",
        Some("py") => "Python",
        Some("json") => "JSON",
        Some("yaml" | "yml") => "YAML",
        Some("toml") => "TOML",
        Some("md" | "mdx") => "Markdown",
        Some("css" | "scss" | "less") => "CSS",
        Some("html" | "htm") => "HTML",
        Some("sql") => "SQL",
        Some("sh" | "bash") => "Shell",
        _ => "Other",
    }
}

pub fn analyze_workspace_for_agent(
    ctx: &mut AgentContext,
    query: &str,
    conn: Option<&rusqlite::Connection>,
) -> Result<Vec<RankedFile>, String> {
    let root = ctx.workspace_root.clone();
    let request = ContextRequest {
        workspace_root: root.clone(),
        query: query.to_string(),
        max_files: 10,
        include_symbols: true,
        include_snippets: true,
    };

    let ranked = if let Some(conn) = conn {
        let (files, _) = ContextRetrieval::retrieve_with_db(conn, &root, query, 10)?;
        files
    } else {
        ContextRetrieval::retrieve(&request)?
    };

    let file_paths: Vec<String> = ranked.iter().map(|r| r.path.clone()).collect();
    crate::agent_controller::AgentController::analyze(ctx, file_paths);
    Ok(ranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let mut d = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        d.push(format!("nf_context_test_{nanos}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ranks_files_by_name_match() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("auth.rs"), "fn authenticate() {}").unwrap();
        fs::write(dir.join("config.json"), "{}").unwrap();
        fs::write(dir.join("main.ts"), "console.log('hi')").unwrap();

        let scan = workspace_scanner::scan_workspace(&dir).unwrap();
        let results = ContextRetrieval::rank_files("auth", &scan, 10);
        assert!(!results.is_empty());
        let first = &results[0];
        assert!(first.path.contains("auth.rs"));
        assert!(first.priority >= 80);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn language_detection_works() {
        assert_eq!(classify_from_path("main.rs"), "Rust");
        assert_eq!(classify_from_path("app.tsx"), "TypeScript");
        assert_eq!(classify_from_path("script.py"), "Python");
        assert_eq!(classify_from_path("config.json"), "JSON");
    }

    #[test]
    fn analyze_updates_agent_context() {
        let dir = temp_dir();
        fs::write(dir.join("lib.rs"), "pub fn run() {}").unwrap();
        fs::write(dir.join("main.rs"), "fn main() {}").unwrap();

        let mut ctx = AgentContext::new("task-1", "Improve auth system", dir.clone());
        let _results = analyze_workspace_for_agent(&mut ctx, "auth", None).unwrap();
        assert!(!ctx.relevant_files.is_empty());
        assert!(matches!(ctx.phase, crate::agent_controller::AgentPhase::Analyzing { .. }));
        fs::remove_dir_all(&dir).ok();
    }
}