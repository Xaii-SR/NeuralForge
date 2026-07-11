use std::path::Path;
use walkdir::WalkDir;

/// Searches workspace files by case-insensitive substring match.
/// Respects the same ignore rules as the indexer (node_modules, .next, target, etc.).
#[tauri::command]
pub fn search_workspace_files(query: String, max_results: usize, workspace_root: String) -> Result<Vec<String>, String> {
    let excluded = [
        "node_modules", ".next", "out", "target", "dist", "logs", "models", ".git", ".neuralforge",
    ];
    let query_lower = query.to_lowercase();
    let root = Path::new(&workspace_root);
    let mut results = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.file_type().is_dir() {
            let name = e.file_name().to_string_lossy();
            return !excluded.contains(&name.as_ref());
        }
        true
    }) {
        if results.len() >= max_results {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let rel_path = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel_str = rel_path.to_string_lossy().to_string();

        if rel_str.to_lowercase().contains(&query_lower) {
            results.push(rel_str);
        }
    }

    Ok(results)
}