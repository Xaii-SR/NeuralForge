use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;

#[derive(Serialize, Type, Clone)]
pub struct SearchResult {
    pub path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub score: f64,
}

/// FTS5's default MATCH syntax ANDs every bare word together, which makes it
/// useless for natural-language questions - "how does authentication work"
/// would require a chunk to contain "how" AND "does" AND "authentication"
/// AND "work" to match at all. Convert to an OR-of-terms query instead, so a
/// question surfaces anything relevant to *any* significant word in it.
/// Terms are double-quoted to avoid FTS5 treating stray punctuation as query
/// syntax.
fn to_fts5_or_query(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// FTS5 full-text keyword search over indexed chunks. This is the baseline
/// "workspace search" capability - always available, no embedding model
/// required. rank is FTS5's built-in bm25-style relevance score (more
/// negative = more relevant, per SQLite's convention), inverted here so
/// higher score = more relevant for the frontend.
pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<SearchResult>> {
    let fts_query = to_fts5_or_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let mut stmt = conn
        .prepare(
            "SELECT chunks.path, chunks.start_line, chunks.end_line, chunks.content, chunks_fts.rank
             FROM chunks_fts
             JOIN chunks ON chunks.id = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )
        .map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;

    let rows = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(SearchResult {
                path: row.get(0)?,
                start_line: row.get(1)?,
                end_line: row.get(2)?,
                content: row.get(3)?,
                score: -row.get::<_, f64>(4)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("search query failed: {e}")))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("search row read failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn to_fts5_or_query_joins_terms_with_or() {
        assert_eq!(to_fts5_or_query("how does auth work"), "\"how\" OR \"does\" OR \"auth\" OR \"work\"");
        assert_eq!(to_fts5_or_query(""), "");
    }

    #[test]
    fn keyword_search_finds_indexed_content() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_search_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("auth.rs"),
            "fn authenticate_user(token: &str) -> bool {\n    validate_token(token)\n}\n",
        )
        .unwrap();
        std::fs::write(dir.join("math.rs"), "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            crate::database::indexer::index_workspace(&conn, &dir).unwrap();

            let results = keyword_search(&conn, "how does authentication work", 10).unwrap();
            assert!(!results.is_empty(), "expected a natural-language query to find the authenticate_user chunk");
            assert!(results.iter().any(|r| r.content.contains("authenticate_user")));

            let no_match = keyword_search(&conn, "nonexistent_symbol_xyz", 10).unwrap();
            assert!(no_match.is_empty());
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
