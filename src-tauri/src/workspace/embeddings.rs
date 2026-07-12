use crate::workspace::chunker::CodeChunk;
use std::path::Path;
use walkdir::WalkDir;

/// A code chunk with its vector embedding.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VectorizedChunk {
    pub file_path: String,
    pub chunk_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub vector: Vec<f32>,
}

/// Builds a local chunk index for the workspace by scanning all source files,
/// chunking them, and saving the results to `.neuralforge/chunks.json`.
#[tauri::command]
pub fn build_local_index(workspace_root: String) -> Result<usize, String> {
    use crate::workspace::chunker::chunk_file_text;
    let root = Path::new(&workspace_root);
    let excluded = [
        "node_modules", ".next", "out", "target", "dist", "logs", "models", ".git", ".neuralforge",
    ];

    let mut all_chunks: Vec<CodeChunk> = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.file_type().is_dir() {
            let name = e.file_name().to_string_lossy();
            return !excluded.contains(&name.as_ref());
        }
        true
    }) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }

        let rel_path = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel_str = rel_path.to_string_lossy().to_string();
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let chunks = chunk_file_text(&rel_str, &content, 50, 10);
        all_chunks.extend(chunks);
    }

    // Save to disk
    let output_dir = root.join(".neuralforge");
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("Failed to create output dir: {e}"))?;
    let json = serde_json::to_string_pretty(&all_chunks).map_err(|e| format!("Serialization failed: {e}"))?;
    std::fs::write(output_dir.join("chunks.json"), &json).map_err(|e| format!("Write failed: {e}"))?;

    let count = all_chunks.len();
    tracing::info!(target: "workspace", event = "index_built", chunk_count = count);
    Ok(count)
}

/// Computes the cosine similarity between two equal-length vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct QueryResult {
    pub file_path: String, pub start_line: usize, pub end_line: usize,
    pub text: String, pub score: f32,
}

#[tauri::command]
pub fn query_codebase_semantic(query: String, max_results: usize, workspace_root: String) -> Result<Vec<QueryResult>, String> {
    let root = std::path::Path::new(&workspace_root);
    let raw = std::fs::read_to_string(root.join(".neuralforge/embeddings.json")).map_err(|e| format!("read: {e}"))?;
    let chunks: Vec<VectorizedChunk> = serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))?;
    if chunks.is_empty() { return Ok(vec![]); }

    use fastembed::{InitOptionsWithLength, EmbeddingModel};
    let mut model = fastembed::TextEmbedding::try_new(InitOptionsWithLength::new(EmbeddingModel::AllMiniLML6V2))
        .map_err(|e| format!("model: {e}"))?;
    let qv = model.embed(vec![query.as_str()], None).map_err(|e| format!("embed: {e}"))?;
    let qv = &qv[0];

    let mut scored: Vec<(f32, &VectorizedChunk)> = chunks.iter().map(|c| (cosine_similarity(qv, &c.vector), c)).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_results);

    Ok(scored.into_iter().map(|(s, c)| QueryResult {
        file_path: c.file_path.clone(), start_line: c.start_line, end_line: c.end_line,
        text: c.text.clone(), score: s,
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn cosine_identical() { let v = vec![1.0, 0.0, 0.0]; assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6); }
    #[test] fn cosine_orthogonal() { assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]) - 0.0).abs() < 1e-6); }
    #[test] fn cosine_opposite() { assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6); }
    #[test] fn cosine_mismatch() { assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0); }
    #[test] fn cosine_empty() { assert_eq!(cosine_similarity(&[], &[]), 0.0); }
    #[test] fn cosine_partial() {
        let a = vec![1.0, 2.0, 3.0]; let c = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &c);
        assert!((sim - (1.0_f32 / (14.0_f32.sqrt()))).abs() < 1e-6);
    }
}

/// Reads the pre-built chunk index and generates 384-d vector embeddings using
/// the local ONNX all-MiniLM-L6-v2 model via fastembed. Saves to embeddings.json.
#[tauri::command]
pub fn generate_local_embeddings(workspace_root: String) -> Result<usize, String> {
    let root = Path::new(&workspace_root);
    let chunks_path = root.join(".neuralforge/chunks.json");
    let raw = std::fs::read_to_string(&chunks_path).map_err(|e| format!("read: {e}"))?;
    let chunks: Vec<CodeChunk> = serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))?;
    if chunks.is_empty() { return Ok(0); }

    use fastembed::{InitOptionsWithLength, EmbeddingModel};
    let mut model = fastembed::TextEmbedding::try_new(InitOptionsWithLength::new(EmbeddingModel::AllMiniLML6V2))
        .map_err(|e| format!("model: {e}"))?;
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let vectors = model.embed(texts, None).map_err(|e| format!("embed: {e}"))?;

    let vectorized: Vec<VectorizedChunk> = chunks.into_iter().zip(vectors).map(|(c, v)| VectorizedChunk {
        file_path: c.file_path, chunk_index: c.chunk_index, start_line: c.start_line,
        end_line: c.end_line, text: c.text, vector: v,
    }).collect();

    let out_dir = root.join(".neuralforge");
    let out_json = serde_json::to_string_pretty(&vectorized).map_err(|e| format!("json: {e}"))?;
    std::fs::write(out_dir.join("embeddings.json"), &out_json).map_err(|e| format!("write: {e}"))?;
    tracing::info!(target: "workspace", event = "embeddings_generated", chunk_count = vectorized.len());
    Ok(vectorized.len())
}
