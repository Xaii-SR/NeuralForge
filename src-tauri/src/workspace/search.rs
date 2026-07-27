use walkdir::WalkDir;

/// Searches workspace files by case-insensitive substring match.
/// Respects the same ignore rules as the indexer (node_modules, .next, target, etc.).
#[tauri::command]
pub fn search_workspace_files(
    state: tauri::State<'_, crate::core::state::AppState>,
    query: String,
    max_results: usize,
) -> Result<Vec<String>, String> {
    let root = state
        .workspace_root
        .lock()
        .map_err(|_| "workspace state lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "no workspace open".to_string())?;
    search_workspace_files_in(&root, &query, max_results)
}

fn search_workspace_files_in(
    root: &std::path::Path,
    query: &str,
    max_results: usize,
) -> Result<Vec<String>, String> {
    let query_lower = query.to_lowercase();
    let policy = crate::workspace_scanner::WorkspacePathPolicy::new(root)?;
    let mut results = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !policy.is_excluded(entry.path(), entry.file_type().is_dir()))
    {
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
        if policy.is_excluded(entry.path(), false) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_results_exclude_secret_and_ignored_paths() {
        let mut root = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        root.push(format!("neuralforge_search_policy_{nanos}"));
        std::fs::create_dir_all(root.join("ignored")).unwrap();
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(root.join(".env"), "secret").unwrap();
        std::fs::write(root.join("ignored").join("match.rs"), "ignored").unwrap();
        std::fs::write(root.join("safe-match.rs"), "safe").unwrap();

        let results = search_workspace_files_in(&root, "match", 10).unwrap();
        assert_eq!(results, vec!["safe-match.rs"]);
        std::fs::remove_dir_all(root).ok();
    }
}
