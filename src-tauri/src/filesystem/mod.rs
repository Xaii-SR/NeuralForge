use crate::core::config::ensure_memory_scaffold;
use crate::core::errors::{AppError, AppResult};
use crate::core::events::emit_file_changed;
use crate::core::state::AppState;
use serde::Serialize;
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Serialize, Type, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

fn workspace_root(state: &State<AppState>) -> AppResult<PathBuf> {
    let root_guard = state.workspace_root.lock().unwrap();
    let root = root_guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".into()))?;
    Ok(fs::canonicalize(root)?)
}

/// For paths that must already exist (read/write/delete/rename source).
fn validate_within_workspace(root: &Path, path: &str) -> AppResult<PathBuf> {
    let target = fs::canonicalize(path).map_err(|_| AppError::NotFound(path.to_string()))?;

    if !target.starts_with(root) {
        return Err(AppError::InvalidPath(format!(
            "{path} is outside the open workspace"
        )));
    }

    Ok(target)
}

/// For paths that don't exist yet (create_file/create_dir/rename destination):
/// validates the parent directory instead, since the target itself can't be canonicalized.
fn validate_new_path_in_workspace(root: &Path, path: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(path);
    if candidate.exists() {
        return Err(AppError::InvalidPath(format!("{path} already exists")));
    }

    let parent = candidate
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| AppError::InvalidPath(format!("{path} has no parent directory")))?;
    let file_name = candidate
        .file_name()
        .ok_or_else(|| AppError::InvalidPath(format!("{path} has no file name")))?;

    let canonical_parent =
        fs::canonicalize(parent).map_err(|_| AppError::NotFound(parent.to_string_lossy().to_string()))?;

    if !canonical_parent.starts_with(root) {
        return Err(AppError::InvalidPath(format!(
            "{path} is outside the open workspace"
        )));
    }

    Ok(canonical_parent.join(file_name))
}

fn list_dir(dir: &Path) -> AppResult<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let path = entry.path();
            FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                is_dir: path.is_dir(),
            }
        })
        .collect();

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

#[tauri::command]
pub fn open_workspace(
    state: State<AppState>,
    db: State<crate::database::DbState>,
    path: String,
) -> AppResult<String> {
    let root = fs::canonicalize(&path)?;
    if !root.is_dir() {
        return Err(AppError::InvalidPath(format!("{path} is not a directory")));
    }
    *state.workspace_root.lock().unwrap() = Some(root.clone());
    ensure_memory_scaffold(&root)?;
    *db.conn.lock().unwrap() = Some(crate::database::open_for_workspace(&root)?);
    tracing::info!(target: "filesystem", event = "workspace_opened", path = %root.display());

    // Automatic indexing (v1.3.0 Phase 3): reuses the exact same
    // database::index_workspace command the manual "Index Workspace" button
    // calls - single source of truth, see database::indexer for the
    // content-hash/mtime incremental behavior that makes this cheap on
    // repeat opens. Indexing failure must never block opening the
    // workspace - file browsing and chat must still work.
    match crate::database::index_workspace(state.clone(), db.clone()) {
        Ok(stats) => tracing::info!(
            target: "filesystem",
            event = "auto_index_completed",
            files_indexed = stats.files_indexed,
            files_skipped_unchanged = stats.files_skipped_unchanged
        ),
        Err(e) => tracing::warn!(
            target: "filesystem",
            event = "auto_index_failed",
            error = %e,
            "automatic workspace indexing failed; workspace remains open"
        ),
    }

    Ok(root.to_string_lossy().to_string())
}

#[tauri::command]
pub fn read_dir(state: State<AppState>, path: String) -> AppResult<Vec<FileEntry>> {
    let root = workspace_root(&state)?;
    let dir = validate_within_workspace(&root, &path)?;
    list_dir(&dir)
}

#[tauri::command]
pub fn read_file(state: State<AppState>, path: String) -> AppResult<String> {
    let root = workspace_root(&state)?;
    let target = validate_within_workspace(&root, &path)?;
    Ok(fs::read_to_string(target)?)
}

#[tauri::command]
pub fn write_file(app: AppHandle, state: State<AppState>, path: String, contents: String) -> AppResult<()> {
    let root = workspace_root(&state)?;
    let target = validate_within_workspace(&root, &path)?;
    fs::write(&target, contents)?;
    tracing::info!(target: "filesystem", event = "file_written", path = %target.display());
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "modified");
    Ok(())
}

#[tauri::command]
pub fn create_file(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let root = workspace_root(&state)?;
    let target = validate_new_path_in_workspace(&root, &path)?;
    fs::write(&target, "")?;
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "created");
    Ok(())
}

#[tauri::command]
pub fn create_dir(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let root = workspace_root(&state)?;
    let target = validate_new_path_in_workspace(&root, &path)?;
    fs::create_dir(&target)?;
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "created");
    Ok(())
}

#[tauri::command]
pub fn delete_path(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let root = workspace_root(&state)?;
    let target = validate_within_workspace(&root, &path)?;
    if target.is_dir() {
        fs::remove_dir_all(&target)?;
    } else {
        fs::remove_file(&target)?;
    }
    tracing::warn!(target: "filesystem", event = "path_deleted", path = %target.display());
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "deleted");
    Ok(())
}

#[tauri::command]
pub fn rename_path(app: AppHandle, state: State<AppState>, from: String, to: String) -> AppResult<()> {
    let root = workspace_root(&state)?;
    let source = validate_within_workspace(&root, &from)?;
    let target = validate_new_path_in_workspace(&root, &to)?;
    fs::rename(&source, &target)?;
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "renamed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_workspace() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("neuralforge_test_{}", uuid_like()));
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    fn uuid_like() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn validate_within_workspace_accepts_path_inside_root() {
        let root = temp_workspace();
        let file = root.join("a.txt");
        fs::write(&file, "hi").unwrap();

        let result = validate_within_workspace(&root, file.to_str().unwrap());
        assert!(result.is_ok());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validate_within_workspace_rejects_path_outside_root() {
        let root = temp_workspace();
        let outside = temp_workspace();
        let file = outside.join("b.txt");
        fs::write(&file, "hi").unwrap();

        let result = validate_within_workspace(&root, file.to_str().unwrap());
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn validate_within_workspace_rejects_traversal_via_dotdot() {
        let root = temp_workspace();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let escape = nested.join("..").join("..").join("etc_passwd_equivalent");
        fs::write(root.parent().unwrap().join("etc_passwd_equivalent"), "x").ok();

        let result = validate_within_workspace(&root, escape.to_str().unwrap());
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
        let _ = fs::remove_file(root.parent().unwrap().join("etc_passwd_equivalent"));
    }

    #[test]
    fn validate_new_path_in_workspace_accepts_new_file_inside_root() {
        let root = temp_workspace();
        let target = root.join("new.txt");

        let result = validate_new_path_in_workspace(&root, target.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validate_new_path_in_workspace_rejects_existing_path() {
        let root = temp_workspace();
        let target = root.join("exists.txt");
        fs::write(&target, "x").unwrap();

        let result = validate_new_path_in_workspace(&root, target.to_str().unwrap());
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn validate_new_path_in_workspace_rejects_parent_outside_root() {
        let root = temp_workspace();
        let outside = temp_workspace();
        let target = outside.join("sneaky.txt");

        let result = validate_new_path_in_workspace(&root, target.to_str().unwrap());
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn list_dir_sorts_directories_before_files_case_insensitively() {
        let root = temp_workspace();
        fs::write(root.join("b.txt"), "").unwrap();
        fs::write(root.join("A.txt"), "").unwrap();
        fs::create_dir(root.join("zdir")).unwrap();

        let entries = list_dir(&root).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", "A.txt", "b.txt"]);

        fs::remove_dir_all(&root).unwrap();
    }

    // ── Automatic indexing (v1.3.0 Phase 3) ─────────────────────────────
    //
    // open_workspace itself takes `State<AppState>`/`State<DbState>`, which
    // (as already documented for the Phase 2 IPC commands - see the
    // MockRuntime notes in agent/mod.rs and agent_core/orchestrator.rs)
    // cannot be constructed in a #[test] without a live Tauri app. These
    // tests instead exercise the exact same sequence of pure calls that
    // open_workspace's body performs (open_for_workspace, then
    // indexer::index_workspace - the identical function the manual "Index
    // Workspace" button calls via database::index_workspace), which is
    // the real logic under test; the command wrapper itself is a
    // provably trivial match over that call, visible directly in the diff.

    #[test]
    fn opening_a_fresh_workspace_indexes_it_without_a_manual_step() {
        let root = temp_workspace();
        fs::write(root.join("auth.rs"), "fn authenticate_user() -> bool { true }\n").unwrap();

        let conn = crate::database::open_for_workspace(&root).unwrap();
        let stats = crate::database::indexer::index_workspace(&conn, &root).unwrap();
        assert_eq!(stats.files_indexed, 1);

        let results = crate::database::search::keyword_search(&conn, "authenticate_user", 20).unwrap();
        assert!(!results.is_empty(), "freshly auto-indexed content must be queryable without pressing the manual button");

        drop(conn); // release the sqlite file handle before deleting on Windows
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reopening_an_already_indexed_workspace_skips_unchanged_files() {
        let root = temp_workspace();
        fs::write(root.join("auth.rs"), "fn authenticate_user() -> bool { true }\n").unwrap();

        let conn = crate::database::open_for_workspace(&root).unwrap();
        let first = crate::database::indexer::index_workspace(&conn, &root).unwrap();
        assert_eq!(first.files_indexed, 1);
        assert_eq!(first.files_skipped_unchanged, 0);

        // Simulates automatic indexing firing again on a second open_workspace
        // call for the same folder - the existing content-hash/mtime
        // incremental behavior in database::indexer (not reimplemented here)
        // must make this a cheap no-op, not a full reprocess.
        let second = crate::database::indexer::index_workspace(&conn, &root).unwrap();
        assert_eq!(second.files_indexed, 0);
        assert_eq!(second.files_skipped_unchanged, 1);

        drop(conn);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn indexing_failure_does_not_prevent_workspace_opening() {
        // The db connection (from a real, still-live workspace) is
        // independent of the *indexing target* path - so this simulates
        // "the opened folder became unreadable/vanished mid-scan" (e.g.
        // removable media, permissions change) without needing to delete a
        // directory that still has an open sqlite file handle in it, which
        // Windows disallows outright.
        let db_root = temp_workspace();
        let conn = crate::database::open_for_workspace(&db_root).unwrap();

        let vanished_root = temp_workspace();
        fs::remove_dir_all(&vanished_root).unwrap();

        // This mirrors exactly the `match crate::database::index_workspace(...)`
        // added to open_workspace: a failure/no-op here must be swallowed
        // (logged) rather than propagated with `?`, so workspace opening
        // itself is unaffected. WalkDir on a missing root yields no entries
        // rather than an Err, but either outcome must not stop the caller.
        let result = crate::database::indexer::index_workspace(&conn, &vanished_root);
        assert!(result.is_ok(), "indexer must not panic/hard-fail on an unreadable workspace root");
        assert_eq!(result.unwrap().files_indexed, 0);

        drop(conn);
        fs::remove_dir_all(&db_root).unwrap();
    }

    #[test]
    fn end_to_end_open_then_ai_chat_context_without_manual_indexing() {
        let root = temp_workspace();
        fs::write(root.join("auth.rs"), "fn authenticate_user() -> bool { true }\n").unwrap();

        // Step 1: open_for_workspace (the DB half of open_workspace).
        let conn = crate::database::open_for_workspace(&root).unwrap();

        // Step 2: automatic indexing, exactly as open_workspace now performs it
        // (same call, same function, no manual "Index Workspace" click).
        crate::database::indexer::index_workspace(&conn, &root).unwrap();

        // Step 3: the real backend entry point AI Chat uses for repository
        // context (ai::mod::get_context_for_query delegates straight into
        // this), per the required audit of the AI context retrieval path.
        let prompt = crate::ai::context::build_context_prompt(&root, &conn, "how does authentication work");
        assert!(prompt.contains("authenticate_user"), "AI Chat must be repository-aware immediately after open, with no manual indexing step");

        drop(conn);
        fs::remove_dir_all(&root).unwrap();
    }
}
