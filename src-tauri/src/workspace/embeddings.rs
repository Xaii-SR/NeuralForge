use crate::workspace::chunker::CodeChunk;
use std::path::Path;
use walkdir::WalkDir;

const EMBEDDING_DIM: usize = 384;
const SEED: u64 = 42;

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

/// Generates a deterministic mock embedding vector for a chunk based on its
/// text content. This produces a consistent 384-d vector for the same text,
/// enabling basic similarity comparison. In production this would use a real
/// ONNX embedding model (e.g., all-MiniLM-L6-v2 via `fastembed`).
fn mock_embed(text: &str) -> Vec<f32> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let seed = hasher.finish();

    let mut vec = Vec::with_capacity(EMBEDDING_DIM);
    let mut rng = seed;
    for _ in 0..EMBEDDING_DIM {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let val = (rng >> 33) as f32 / u32::MAX as f32;
        vec.push(val * 2.0 - 1.0); // Normalize to [-1, 1]
    }
    // L2 normalize
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec { *v /= norm; }
    }
    vec
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

/// Reads the pre-built chunk index and generates deterministic vector embeddings
/// for each chunk using a hash-based mock embedding. Saves the result to
/// `.neuralforge/embeddings.json`.
#[tauri::command]
pub fn generate_local_embeddings(workspace_root: String) -> Result<usize, String> {
    let root = Path::new(&workspace_root);
    let chunks_path = root.join(".neuralforge/chunks.json");

    let json = std::fs::read_to_string(&chunks_path)
        .map_err(|e| format!("Failed to read chunks.json: {e}"))?;
    let chunks: Vec<CodeChunk> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse chunks.json: {e}"))?;

    let vectorized: Vec<VectorizedChunk> = chunks.into_iter().map(|chunk| {
        let vector = mock_embed(&chunk.text);
        VectorizedChunk {
            file_path: chunk.file_path,
            chunk_index: chunk.chunk_index,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            text: chunk.text,
            vector,
        }
    }).collect();

    let output_dir = root.join(".neuralforge");
    let out_json = serde_json::to_string_pretty(&vectorized)
        .map_err(|e| format!("Serialization failed: {e}"))?;
    std::fs::write(output_dir.join("embeddings.json"), &out_json)
        .map_err(|e| format!("Write failed: {e}"))?;

    let count = vectorized.len();
    tracing::info!(target: "workspace", event = "embeddings_generated", chunk_count = count);
    Ok(count)
}
