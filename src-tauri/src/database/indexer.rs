use crate::core::errors::AppResult;
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const EXCLUDED_DIRS: &[&str] = &[
    "node_modules", ".next", "out", "target", "dist", "logs", "models", ".git", ".neuralforge",
];

const MAX_FILE_BYTES: u64 = 1_000_000;
const CHUNK_LINES: usize = 40;
const CHUNK_OVERLAP: usize = 5;

#[derive(Serialize, Type, Clone, Default)]
pub struct IndexStats {
    pub files_scanned: u64,
    pub files_indexed: u64,
    pub files_skipped_unchanged: u64,
    pub files_skipped_binary: u64,
    pub files_skipped_size: u64,
    pub files_failed: u64,
    pub chunks_created: u64,
    pub languages_detected: HashMap<String, u64>,
    pub total_bytes_indexed: u64,
    pub last_index_timestamp: i64,
}

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn is_probably_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    !sample.contains(&0)
}

fn should_skip_dir(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

fn classify_language(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "Rust",
        Some("ts" | "tsx") => "TypeScript",
        Some("js" | "jsx") => "JavaScript",
        Some("py") => "Python",
        Some("c" | "h" | "cpp" | "hpp" | "cc" | "cxx") => "C/C++",
        Some("json") => "JSON",
        Some("yaml" | "yml") => "YAML",
        Some("toml") => "TOML",
        Some("md" | "mdx") => "Markdown",
        Some("css" | "scss" | "less") => "CSS",
        Some("html" | "htm") => "HTML",
        Some("sql") => "SQL",
        Some("sh" | "bash" | "zsh") => "Shell",
        Some("env" | "ini" | "cfg") => "Config",
        Some("txt" | "text") => "Text",
        _ => "Other",
    }
}

fn chunk_lines(content: &str) -> Vec<(usize, usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let text = lines[start..end].join("\n");
        chunks.push((start + 1, end, text));
        if end == lines.len() {
            break;
        }
        start = end - CHUNK_OVERLAP;
    }
    chunks
}

pub fn index_workspace(conn: &Connection, workspace_root: &Path) -> AppResult<IndexStats> {
    let mut stats = IndexStats::default();
    stats.languages_detected = HashMap::new();
    stats.last_index_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let walker = WalkDir::new(workspace_root).into_iter().filter_entry(|entry| {
        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy();
            return !should_skip_dir(&name);
        }
        true
    });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        let file_size = metadata.len();
        if file_size > MAX_FILE_BYTES {
            stats.files_skipped_size += 1;
            continue;
        }

        stats.files_scanned += 1;

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let rel_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        // Check if file exists in DB with matching modified_at for fast skip
        let existing: Option<(String, i64)> = conn
            .query_row(
                "SELECT content_hash, modified_at FROM files WHERE path = ?1",
                params![rel_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .ok();

        if let Some((ref existing_hash, existing_modified)) = existing {
            if existing_modified == modified_at {
                stats.files_skipped_unchanged += 1;
                continue;
            }
            // modified_at changed: re-read and re-hash
            let Ok(bytes) = std::fs::read(path) else {
                stats.files_failed += 1;
                continue;
            };
            if !is_probably_text(&bytes) {
                stats.files_skipped_binary += 1;
                continue;
            }
            let Ok(content) = String::from_utf8(bytes) else {
                stats.files_failed += 1;
                continue;
            };
            let hash = hash_content(&content);
            if hash == *existing_hash && existing_modified == modified_at {
                // Content unchanged despite different modified_at — just update timestamp
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                conn.execute(
                    "UPDATE files SET modified_at = ?1, indexed_at = ?2 WHERE path = ?3",
                    params![modified_at, now, rel_path],
                )
                .ok();
                stats.files_skipped_unchanged += 1;
                continue;
            }
            // Content or timestamp changed — re-index below
        }

        let Ok(bytes) = std::fs::read(path) else {
            stats.files_failed += 1;
            continue;
        };
        if !is_probably_text(&bytes) {
            stats.files_skipped_binary += 1;
            continue;
        }
        let Ok(content) = String::from_utf8(bytes) else {
            stats.files_failed += 1;
            continue;
        };
        let hash = hash_content(&content);
        let language = classify_language(path);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let line_count = content.lines().count() as i64;

        *stats
            .languages_detected
            .entry(language.to_string())
            .or_insert(0) += 1;
        stats.total_bytes_indexed += file_size;

        let file_id: i64 = if let Some(id) = conn
            .query_row(
                "SELECT id FROM files WHERE path = ?1",
                params![rel_path],
                |row| row.get::<_, i64>(0),
            )
            .ok()
        {
            conn.execute(
                "UPDATE files SET content_hash = ?1, indexed_at = ?2, file_size = ?3, modified_at = ?4, language = ?5, line_count = ?6 WHERE id = ?7",
                params![hash, now, file_size as i64, modified_at, language, line_count, id],
            )
            .ok();
            conn.execute("DELETE FROM chunks WHERE file_id = ?1", params![id])
                .ok();
            id
        } else {
            conn.execute(
                "INSERT INTO files (path, content_hash, indexed_at, file_size, modified_at, language, line_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![rel_path, hash, now, file_size as i64, modified_at, language, line_count],
            )
            .ok();
            conn.last_insert_rowid()
        };

        for (start_line, end_line, text) in chunk_lines(&content) {
            conn.execute(
                "INSERT INTO chunks (file_id, path, start_line, end_line, content) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![file_id, rel_path, start_line as i64, end_line as i64, text],
            )
            .ok();
            stats.chunks_created += 1;
        }

        stats.files_indexed += 1;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_lines_splits_with_overlap() {
        let content = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunk_lines(&content);
        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].0, 1);
        assert_eq!(chunks[0].1, CHUNK_LINES);
        assert_eq!(chunks[1].0, CHUNK_LINES - CHUNK_OVERLAP + 1);
    }

    #[test]
    fn chunk_lines_handles_short_content() {
        let chunks = chunk_lines("just one line");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, 1);
        assert_eq!(chunks[0].1, 1);
    }

    #[test]
    fn is_probably_text_rejects_null_bytes() {
        assert!(is_probably_text(b"hello world"));
        assert!(!is_probably_text(b"hello\0world"));
    }

    #[test]
    fn classify_language_returns_correct_language() {
        assert_eq!(classify_language(Path::new("main.rs")), "Rust");
        assert_eq!(classify_language(Path::new("app.ts")), "TypeScript");
        assert_eq!(classify_language(Path::new("app.tsx")), "TypeScript");
        assert_eq!(classify_language(Path::new("script.py")), "Python");
        assert_eq!(classify_language(Path::new("index.js")), "JavaScript");
        assert_eq!(classify_language(Path::new("config.json")), "JSON");
        assert_eq!(classify_language(Path::new("config.yaml")), "YAML");
        assert_eq!(classify_language(Path::new("Cargo.toml")), "TOML");
        assert_eq!(classify_language(Path::new("README.md")), "Markdown");
        assert_eq!(classify_language(Path::new("unknown.xyz")), "Other");
    }

    #[test]
    fn index_workspace_indexes_and_skips_unchanged() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_indexer_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join("node_modules").join("skip.js"), "should not be indexed").unwrap();

        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();

            let stats1 = index_workspace(&conn, &dir).unwrap();
            assert_eq!(stats1.files_indexed, 1);
            assert!(stats1.chunks_created >= 1);
            assert_eq!(*stats1.languages_detected.get("Rust").unwrap_or(&0), 1);

            let stats2 = index_workspace(&conn, &dir).unwrap();
            assert_eq!(stats2.files_indexed, 0);
            assert_eq!(stats2.files_skipped_unchanged, 1);

            let count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)).unwrap();
            assert!(count >= 1);
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn index_workspace_reindexes_after_file_change() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_indexer_reindex_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("lib.rs");
        std::fs::write(&file_path, "pub fn a() -> i32 { 1 }").unwrap();

        let conn = crate::database::open_for_workspace(&dir).unwrap();

        let stats1 = index_workspace(&conn, &dir).unwrap();
        assert_eq!(stats1.files_indexed, 1);

        // Use a small delay to ensure modified_at changes
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&file_path, "pub fn b() -> i32 { 2 }").unwrap();

        let stats2 = index_workspace(&conn, &dir).unwrap();
        assert_eq!(stats2.files_indexed, 1, "modified file should be re-indexed");
        assert_eq!(stats2.files_skipped_unchanged, 0);

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}