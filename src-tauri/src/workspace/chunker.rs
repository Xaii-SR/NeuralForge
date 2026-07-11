use serde::{Deserialize, Serialize};

/// A single chunk of code from a source file, with positional metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_path: String,
    pub chunk_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

/// Splits file content into overlapping chunks.
///
/// * `file_path` - The relative path of the file (for metadata).
/// * `content` - The full text of the file.
/// * `chunk_lines` - Maximum lines per chunk (default 50).
/// * `overlap_lines` - Number of lines to overlap between chunks (default 10).
pub fn chunk_file_text(
    file_path: &str,
    content: &str,
    chunk_lines: usize,
    overlap_lines: usize,
) -> Vec<CodeChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut start: usize = 0;
    let mut index: usize = 0;

    while start < lines.len() {
        let end = (start + chunk_lines).min(lines.len());
        let text = lines[start..end].join("\n");

        chunks.push(CodeChunk {
            file_path: file_path.to_string(),
            chunk_index: index,
            start_line: start + 1, // 1-based line numbers
            end_line: end,
            text,
        });

        if end == lines.len() {
            break;
        }

        index += 1;
        start = end.saturating_sub(overlap_lines);
        // Prevent infinite loop when overlap >= chunk_lines
        if start >= end {
            start = end.saturating_sub(1);
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_basic_overlap() {
        let content = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunk_file_text("test.rs", &content, 50, 10);
        assert_eq!(chunks.len(), 3, "100 lines @ 50 chunk/10 overlap = 3 chunks");

        // Chunk 0: lines 1..50
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 50);

        // Chunk 1: starts at 50-10 = 41 (1-based)
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[1].start_line, 41);

        // Chunk 2: last chunk holds the rest (start = 80, 1-based = 81)
        assert_eq!(chunks[2].chunk_index, 2);
        assert_eq!(chunks[2].start_line, 81);
        assert_eq!(chunks[2].end_line, 100);
    }

    #[test]
    fn chunks_short_file() {
        let content = "only one line";
        let chunks = chunk_file_text("short.rs", content, 50, 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "only one line");
    }

    #[test]
    fn chunks_empty_file() {
        let chunks = chunk_file_text("empty.rs", "", 50, 10);
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunks_exact_fit() {
        let content = (1..=50).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunk_file_text("exact.rs", &content, 50, 10);
        assert_eq!(chunks.len(), 1, "50 lines @ 50 chunk = single chunk");
    }
}