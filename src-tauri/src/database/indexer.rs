use crate::core::errors::AppResult;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const EXCLUDED_DIRS: &[&str] = &[
    "node_modules", ".next", "out", "target", "dist", "logs", "models", ".git", ".neuralforge",
];

const MAX_FILE_BYTES: u64 = 1_000_000;
const CHUNK_LINES: usize = 40;
const CHUNK_OVERLAP: usize = 5;

#[derive(Serialize, Clone, Default)]
pub struct IndexStats {
    pub files_scanned: u64,
    pub files_indexed: u64,
    pub files_skipped_unchanged: u64,
    pub chunks_created: u64,
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
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }

        stats.files_scanned += 1;

        let Ok(bytes) = std::fs::read(path) else { continue };
        if !is_probably_text(&bytes) {
            continue;
        }
        let Ok(content) = String::from_utf8(bytes) else { continue };

        let rel_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let hash = hash_content(&content);

        let existing_hash: Option<String> = conn
            .query_row("SELECT content_hash FROM files WHERE path = ?1", params![rel_path], |row| row.get(0))
            .ok();

        if existing_hash.as_deref() == Some(hash.as_str()) {
            stats.files_skipped_unchanged += 1;
            continue;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        let file_id: i64 = if let Some(id) = conn
            .query_row("SELECT id FROM files WHERE path = ?1", params![rel_path], |row| row.get::<_, i64>(0))
            .ok()
        {
            conn.execute(
                "UPDATE files SET content_hash = ?1, indexed_at = ?2 WHERE id = ?3",
                params![hash, now, id],
            )
            .ok();
            conn.execute("DELETE FROM chunks WHERE file_id = ?1", params![id]).ok();
            id
        } else {
            conn.execute(
                "INSERT INTO files (path, content_hash, indexed_at) VALUES (?1, ?2, ?3)",
                params![rel_path, hash, now],
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
        // consecutive chunks overlap
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

            let stats2 = index_workspace(&conn, &dir).unwrap();
            assert_eq!(stats2.files_indexed, 0);
            assert_eq!(stats2.files_skipped_unchanged, 1);

            let count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)).unwrap();
            assert!(count >= 1);
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
