use crate::core::errors::{AppError, AppResult};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileCandidate {
    pub path: String,
    pub score: f64,
    pub match_kind: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ResolutionResult {
    /// Some(path) only when one candidate clearly beats the runner-up - a
    /// confident, transparent auto-resolution, not a silent guess among
    /// close contenders.
    pub resolved: Option<String>,
    /// Top candidates (highest score first), always populated when any file
    /// matched at all - used for a disambiguation prompt when `resolved` is
    /// None but this isn't empty.
    pub candidates: Vec<FileCandidate>,
}

const FILENAME_TOKEN_WEIGHT: f64 = 10.0;
const FILENAME_STEM_BONUS: f64 = 20.0;
const PATH_TOKEN_WEIGHT: f64 = 3.0;
const CONTENT_MATCH_WEIGHT: f64 = 1.0;
const MAX_CANDIDATES: usize = 5;
/// The top match must beat the runner-up by this ratio to auto-resolve
/// without asking the human - a near-tie is exactly the case a
/// disambiguation prompt exists for.
const CLEAR_WINNER_RATIO: f64 = 1.5;

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Resolves a natural-language file reference ("my Toyota Carina car's UI
/// JSON") against the already-indexed workspace (Phase 3's `files`/`chunks`
/// tables) instead of requiring an exact path. Filename matches rank above
/// path matches, which rank above content-only matches (found via the
/// existing FTS5 keyword_search) - a query word appearing in a filename is a
/// much stronger signal of "this is the file you mean" than the same word
/// merely appearing somewhere inside a file's text.
pub fn resolve_file_reference(conn: &Connection, query: &str) -> AppResult<ResolutionResult> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Ok(ResolutionResult { resolved: None, candidates: vec![] });
    }

    let mut stmt = conn.prepare("SELECT path FROM files").map_err(|e| AppError::Provider(format!("failed to list files: {e}")))?;
    let paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| AppError::Provider(format!("failed to list files: {e}")))?
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read file row: {e}")))?;

    let mut scores: HashMap<String, (f64, &'static str)> = HashMap::new();
    let joined_query: String = tokens.join("");

    for path in &paths {
        let name = basename(path).to_lowercase();
        let name_stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&name);
        let stem_alnum: String = name_stem.chars().filter(|c| c.is_alphanumeric()).collect();
        let path_lower = path.to_lowercase();

        let mut filename_score = 0.0;
        for tok in &tokens {
            if name.contains(tok.as_str()) {
                filename_score += FILENAME_TOKEN_WEIGHT;
            }
        }
        if !joined_query.is_empty() && stem_alnum.contains(&joined_query) {
            filename_score += FILENAME_STEM_BONUS;
        }

        let mut path_score = 0.0;
        for tok in &tokens {
            if path_lower.contains(tok.as_str()) && !name.contains(tok.as_str()) {
                path_score += PATH_TOKEN_WEIGHT;
            }
        }

        let total = filename_score + path_score;
        if total > 0.0 {
            let kind = if filename_score > 0.0 { "filename" } else { "path" };
            scores.insert(path.clone(), (total, kind));
        }
    }

    let content_hits = crate::database::search::keyword_search(conn, query, 20).unwrap_or_default();
    for hit in content_hits {
        let entry = scores.entry(hit.path.clone()).or_insert((0.0, "content"));
        entry.0 += CONTENT_MATCH_WEIGHT;
    }

    let mut candidates: Vec<FileCandidate> =
        scores.into_iter().map(|(path, (score, kind))| FileCandidate { path, score, match_kind: kind.to_string() }).collect();
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(MAX_CANDIDATES);

    let resolved = match candidates.as_slice() {
        [] => None,
        [only] => Some(only.path.clone()),
        [first, second, ..] if first.score >= second.score * CLEAR_WINNER_RATIO => Some(first.path.clone()),
        _ => None,
    };

    Ok(ResolutionResult { resolved, candidates })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_indexed_workspace(files: &[(&str, &str)]) -> (std::path::PathBuf, Connection) {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_resolver_test_{nanos}"));
        for (rel_path, content) in files {
            let full = dir.join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, content).unwrap();
        }
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        crate::database::indexer::index_workspace(&conn, &dir).unwrap();
        (dir, conn)
    }

    #[test]
    fn resolves_a_clear_filename_winner_without_exact_path() {
        let (dir, conn) = temp_indexed_workspace(&[
            ("carina_egti/ui_car.json", "{\"screen\": \"dashboard\"}"),
            ("carina_egti/ext_config.ini", "[general]\nname=carina"),
            ("unrelated/notes.md", "just some notes"),
        ]);

        let result = resolve_file_reference(&conn, "clear the UI JSON for the carina").unwrap();
        // indexer::index_workspace stores paths with the OS-native separator
        // (backslash on Windows), same convention as search::keyword_search -
        // normalize before comparing rather than assuming forward slashes.
        let resolved = result.resolved.map(|p| p.replace('\\', "/"));
        assert_eq!(resolved.as_deref(), Some("carina_egti/ui_car.json"));

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ambiguous_query_returns_candidates_without_auto_resolving() {
        let (dir, conn) = temp_indexed_workspace(&[
            ("carina_egti/ui_car.json", "{}"),
            ("carina_egti/ui_car_backup.json", "{}"),
        ]);

        let result = resolve_file_reference(&conn, "carina ui car").unwrap();
        assert!(result.candidates.len() >= 2, "expected multiple close candidates, got {:?}", result.candidates);

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_match_returns_empty_result_not_an_error() {
        let (dir, conn) = temp_indexed_workspace(&[("readme.md", "hello world")]);

        let result = resolve_file_reference(&conn, "nonexistent_zzz_reference").unwrap();
        assert!(result.resolved.is_none());
        assert!(result.candidates.is_empty());

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn filename_match_outranks_content_only_match() {
        let (dir, conn) = temp_indexed_workspace(&[
            ("auth.rs", "fn authenticate_user() {}"),
            ("notes.md", "remember to check the auth flow carefully"),
        ]);

        let result = resolve_file_reference(&conn, "auth").unwrap();
        assert_eq!(result.resolved.as_deref(), Some("auth.rs"));

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_query_resolves_to_nothing() {
        let (dir, conn) = temp_indexed_workspace(&[("readme.md", "hello")]);
        let result = resolve_file_reference(&conn, "   ").unwrap();
        assert!(result.resolved.is_none());
        assert!(result.candidates.is_empty());
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
