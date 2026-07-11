use crate::workspace::chunker::{chunk_file_text, CodeChunk};
use std::path::Path;
use walkdir::WalkDir;

/// Builds a local chunk index for the workspace by scanning all source files,
/// chunking them, and saving the results to `.neuralforge/chunks.json`.
#[tauri::command]
pub fn build_local_index(workspace_root: String) -> Result<usize, String> {
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