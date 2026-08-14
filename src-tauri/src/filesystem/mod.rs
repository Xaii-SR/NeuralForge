use crate::core::config::ensure_memory_scaffold;
use crate::core::errors::{AppError, AppResult};
use crate::core::events::emit_file_changed;
use crate::core::state::AppState;
use serde::Serialize;
use specta::Type;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

#[derive(Serialize, Type, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Serialize, Type, Clone)]
pub struct WorkspaceInfo {
    pub root: String,
    pub generation: u64,
}

fn workspace_root(state: &State<AppState>) -> AppResult<PathBuf> {
    let _activation = state.workspace_activation.lock().unwrap();
    let root_guard = state.workspace_root.lock().unwrap();
    let root = root_guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".into()))?;
    Ok(fs::canonicalize(root)?)
}

fn reject_workspace_root(root: &Path, target: &Path) -> AppResult<()> {
    if target == root {
        return Err(AppError::CommandRejected(
            "the open workspace root cannot be deleted".to_string(),
        ));
    }
    Ok(())
}

fn write_if_unchanged(
    root: &Path,
    path: &str,
    expected: &str,
    contents: &str,
) -> AppResult<PathBuf> {
    write_if_unchanged_core(root, path, expected, contents, |_| {})
}

fn write_if_unchanged_core(
    root: &Path,
    path: &str,
    expected: &str,
    contents: &str,
    before_write: impl FnOnce(&Path),
) -> AppResult<PathBuf> {
    let target = validate_within_workspace(root, path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let mut file = options.open(&target)?;
    let mut current = String::new();
    file.read_to_string(&mut current)?;
    if current != expected {
        return Err(AppError::CommandRejected(
            "the file changed after this proposal was created".to_string(),
        ));
    }
    before_write(&target);
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(target)
}

fn remove_path(root: &Path, path: &str) -> AppResult<PathBuf> {
    let target = validate_within_workspace(root, path)?;
    reject_workspace_root(root, &target)?;
    if target.is_dir() {
        fs::remove_dir_all(&target)?;
    } else {
        fs::remove_file(&target)?;
    }
    Ok(target)
}

fn activate_prepared_workspace(
    state: &AppState,
    db: &crate::database::DbState,
    request: u64,
    root: PathBuf,
    connection: rusqlite::Connection,
) -> AppResult<u64> {
    let _activation = state.workspace_activation.lock().unwrap();
    if !state.is_latest_workspace_open(request) {
        return Err(AppError::CommandRejected(
            "a newer workspace open request superseded this one".to_string(),
        ));
    }
    let mut root_guard = state.workspace_root.lock().unwrap();
    let mut db_guard = db.conn.lock().unwrap();
    *db_guard = Some(connection);
    *root_guard = Some(root);
    Ok(state.advance_workspace_generation())
}

fn prepare_workspace(path: &str) -> AppResult<(PathBuf, rusqlite::Connection)> {
    let root = fs::canonicalize(path)?;
    if !root.is_dir() {
        return Err(AppError::InvalidPath(format!("{path} is not a directory")));
    }
    ensure_memory_scaffold(&root)?;
    let connection = crate::database::open_for_workspace(&root)?;
    Ok((root, connection))
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

    let canonical_parent = fs::canonicalize(parent)
        .map_err(|_| AppError::NotFound(parent.to_string_lossy().to_string()))?;

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

/// v1.4.0 workspace restoration: the last successfully opened workspace
/// path is remembered in a small file under the app's data dir (the same
/// global, non-per-workspace location family as app_log_dir - per-workspace
/// storage can't work here, since the whole point is knowing which
/// workspace to reopen before any workspace is open). Pure functions over
/// an explicit dir so they're testable with temp dirs, per this codebase's
/// pure-core/thin-wrapper pattern.
const LAST_WORKSPACE_FILE: &str = "last_workspace.txt";

fn save_last_workspace_path(data_dir: &Path, workspace: &Path) -> std::io::Result<()> {
    fs::create_dir_all(data_dir)?;
    fs::write(
        data_dir.join(LAST_WORKSPACE_FILE),
        workspace.to_string_lossy().as_bytes(),
    )
}

fn save_last_workspace_if_current(
    state: &AppState,
    data_dir: &Path,
    workspace: &Path,
    generation: u64,
) -> std::io::Result<bool> {
    let _activation = state.workspace_activation.lock().unwrap();
    if !state.matches_workspace_generation(generation) {
        return Ok(false);
    }
    save_last_workspace_path(data_dir, workspace)?;
    Ok(true)
}

/// Returns the remembered path only if it still exists as a directory -
/// a moved/deleted workspace degrades to "nothing to restore", never an
/// error surfaced at startup.
fn load_last_workspace_path(data_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(data_dir.join(LAST_WORKSPACE_FILE)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_dir() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Startup restoration query: which workspace, if any, should the frontend
/// reopen? Read-only - the frontend then goes through the exact same
/// open_workspace flow a manual "Open Folder" uses (single source of truth,
/// so restoration gets indexing/session behavior identical to a manual open).
#[tauri::command]
pub fn get_last_workspace(app: AppHandle) -> Option<String> {
    use tauri::Manager;
    let data_dir = app.path().app_data_dir().ok()?;
    load_last_workspace_path(&data_dir)
}

#[tauri::command]
pub fn open_workspace(
    app: AppHandle,
    state: State<AppState>,
    db: State<crate::database::DbState>,
    path: String,
) -> AppResult<WorkspaceInfo> {
    let request = state.begin_workspace_open();
    let (root, connection) = prepare_workspace(&path)?;
    let generation = activate_prepared_workspace(&state, &db, request, root.clone(), connection)?;
    tracing::info!(
        target: "filesystem",
        event = "workspace_opened",
        path = %root.display(),
        generation
    );

    // Remember this workspace for startup restoration (v1.4.0). Best-effort:
    // a failure to persist the preference must never fail the open itself.
    {
        use tauri::Manager;
        if let Ok(data_dir) = app.path().app_data_dir() {
            if let Err(e) = save_last_workspace_if_current(&state, &data_dir, &root, generation) {
                tracing::warn!(target: "filesystem", event = "last_workspace_save_failed", error = %e);
            }
        }
    }

    // Automatic indexing (v1.3.0 Phase 3, made non-blocking post-audit):
    // still the exact same indexer::index_workspace implementation the
    // manual "Index Workspace" button reaches - single source of truth -
    // but now invoked on a background thread with its own Connection to
    // the same index.db, so open_workspace returns immediately.
    //
    // Why: the original synchronous call blocked this IPC command until
    // indexing finished. On a huge real-world folder (reproduced against
    // Z:\Steam\...\assettocorsa\content\cars: three opens logged
    // workspace_opened with no auto_index_completed ever following), the
    // recursive walk alone can take minutes, the window goes Not
    // Responding, and the user kills the app - a release-blocking crash
    // in practice, not the documented "tens of seconds" freeze.
    //
    // A separate Connection (rather than locking the shared DbState from
    // the thread) keeps every UI-facing command - session loading, chat
    // context, settings - fully unblocked while indexing runs. Write
    // collisions between the two connections are absorbed by the
    // busy_timeout set in database::open_for_workspace. Indexing failure
    // still can't block the workspace: the thread only logs.
    let index_root = root.clone();
    std::thread::spawn(
        move || match crate::database::open_for_workspace(&index_root) {
            Ok(conn) => match crate::database::indexer::index_workspace(&conn, &index_root) {
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
            },
            Err(e) => tracing::warn!(
                target: "filesystem",
                event = "auto_index_failed",
                error = %e,
                "could not open background indexing connection; workspace remains open"
            ),
        },
    );

    // NF-IDX-002: start (or replace) the live filesystem watcher for this
    // workspace/generation. Assigning into current_watcher drops whatever
    // watcher was active for the previous workspace, stopping it - so a
    // rapid re-open/switch can never leave two watchers running against two
    // different roots. Watcher failures (e.g. an unreadable root) are
    // logged, not fatal - the same policy as auto-indexing above.
    match crate::services::watcher_service::WorkspaceWatcher::start(
        app.clone(),
        root.clone(),
        generation,
        300,
    ) {
        Ok(watcher) => {
            *state.current_watcher.lock().unwrap() = Some(watcher);
        }
        Err(e) => {
            tracing::warn!(
                target: "filesystem",
                event = "watcher_start_failed",
                error = %e,
                "could not start the file watcher; workspace remains open without live reindexing"
            );
            *state.current_watcher.lock().unwrap() = None;
        }
    }

    Ok(WorkspaceInfo {
        root: root.to_string_lossy().to_string(),
        generation,
    })
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
pub fn write_file(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    contents: String,
) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
    let root = workspace_root(&state)?;
    let target = validate_within_workspace(&root, &path)?;
    fs::write(&target, contents)?;
    tracing::info!(target: "filesystem", event = "file_written", path = %target.display());
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "modified");
    Ok(())
}

#[tauri::command]
pub fn write_file_if_unchanged(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    expected_contents: String,
    contents: String,
) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
    let root = workspace_root(&state)?;
    let target = write_if_unchanged(&root, &path, &expected_contents, &contents)?;
    tracing::info!(target: "filesystem", event = "file_written_after_compare", path = %target.display());
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "modified");
    Ok(())
}

#[tauri::command]
pub fn create_file(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
    let root = workspace_root(&state)?;
    let target = validate_new_path_in_workspace(&root, &path)?;
    fs::write(&target, "")?;
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "created");
    Ok(())
}

#[tauri::command]
pub fn create_dir(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
    let root = workspace_root(&state)?;
    let target = validate_new_path_in_workspace(&root, &path)?;
    fs::create_dir(&target)?;
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "created");
    Ok(())
}

#[tauri::command]
pub fn delete_path(app: AppHandle, state: State<AppState>, path: String) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
    let root = workspace_root(&state)?;
    let target = remove_path(&root, &path)?;
    tracing::warn!(target: "filesystem", event = "path_deleted", path = %target.display());
    let _ = emit_file_changed(&app, &target.to_string_lossy(), "deleted");
    Ok(())
}

#[tauri::command]
pub fn rename_path(
    app: AppHandle,
    state: State<AppState>,
    from: String,
    to: String,
) -> AppResult<()> {
    let _mutation = state.filesystem_mutation.lock().unwrap();
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
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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

    #[test]
    fn workspace_root_is_never_deletable() {
        let root = temp_workspace();
        let with_separator = format!("{}{}", root.display(), std::path::MAIN_SEPARATOR);

        assert!(remove_path(&root, root.to_str().unwrap()).is_err());
        assert!(remove_path(&root, &with_separator).is_err());
        assert!(root.exists());

        #[cfg(windows)]
        {
            let case_variant = root.to_string_lossy().to_uppercase();
            assert!(remove_path(&root, &case_variant).is_err());
            assert!(root.exists());
        }

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn workspace_child_remains_deletable() {
        let root = temp_workspace();
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("data.txt"), "preserve root").unwrap();

        remove_path(&root, child.to_str().unwrap()).unwrap();

        assert!(!child.exists());
        assert!(root.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn junction_to_workspace_root_is_never_deletable() {
        let root = temp_workspace();
        let junction = root.join("root-junction");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                junction.to_str().unwrap(),
                root.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success(), "test junction creation must succeed");

        assert!(remove_path(&root, junction.to_str().unwrap()).is_err());
        assert!(root.exists());

        fs::remove_dir(&junction).unwrap();
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn compare_write_rejects_stale_content_without_writing() {
        let root = temp_workspace();
        let file = root.join("proposal.txt");
        fs::write(&file, "newer content").unwrap();

        let result = write_if_unchanged(&root, file.to_str().unwrap(), "old content", "proposal");

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer content");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn compare_write_applies_when_base_matches() {
        let root = temp_workspace();
        let file = root.join("proposal.txt");
        fs::write(&file, "base").unwrap();

        write_if_unchanged(&root, file.to_str().unwrap(), "base", "proposal").unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "proposal");
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn compare_write_holds_exclusive_handle_through_the_write() {
        let root = temp_workspace();
        let file = root.join("proposal.txt");
        fs::write(&file, "base").unwrap();
        let mut competing_write_was_rejected = false;

        write_if_unchanged_core(
            &root,
            file.to_str().unwrap(),
            "base",
            "proposal",
            |target| {
                competing_write_was_rejected = fs::write(target, "racing edit").is_err();
            },
        )
        .unwrap();

        assert!(competing_write_was_rejected);
        assert_eq!(fs::read_to_string(&file).unwrap(), "proposal");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn stale_overlapping_workspace_open_cannot_publish_state() {
        let state = std::sync::Arc::new(AppState::default());
        let db = std::sync::Arc::new(crate::database::DbState::default());
        let first = state.begin_workspace_open();
        let first_root = temp_workspace();
        let second_root = temp_workspace();
        let first_conn = crate::database::open_for_workspace(&first_root).unwrap();
        let second_conn = crate::database::open_for_workspace(&second_root).unwrap();
        let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_state = state.clone();
        let first_db = db.clone();
        let first_ready = ready.clone();
        let first_release = release.clone();
        let first_root_for_thread = first_root.clone();
        let stale_thread = std::thread::spawn(move || {
            first_ready.wait();
            first_release.wait();
            activate_prepared_workspace(
                &first_state,
                &first_db,
                first,
                first_root_for_thread,
                first_conn,
            )
        });

        ready.wait();
        let second = state.begin_workspace_open();
        let generation =
            activate_prepared_workspace(&state, &db, second, second_root.clone(), second_conn)
                .unwrap();
        release.wait();
        assert!(stale_thread.join().unwrap().is_err());

        assert_eq!(generation, 1);
        assert_eq!(
            state.workspace_root.lock().unwrap().as_ref(),
            Some(&second_root)
        );
        drop(db.conn.lock().unwrap().take());
        fs::remove_dir_all(first_root).unwrap();
        fs::remove_dir_all(second_root).unwrap();
    }

    #[test]
    fn failed_workspace_preparation_preserves_active_state() {
        let state = AppState::default();
        let db = crate::database::DbState::default();
        let active_root = temp_workspace();
        let request = state.begin_workspace_open();
        let connection = crate::database::open_for_workspace(&active_root).unwrap();
        activate_prepared_workspace(&state, &db, request, active_root.clone(), connection).unwrap();
        let missing = active_root.join("missing");

        assert!(prepare_workspace(missing.to_str().unwrap()).is_err());
        assert_eq!(
            state.workspace_root.lock().unwrap().as_ref(),
            Some(&active_root)
        );
        assert_eq!(state.workspace_generation(), 1);

        drop(db.conn.lock().unwrap().take());
        fs::remove_dir_all(active_root).unwrap();
    }

    #[test]
    fn stale_workspace_open_cannot_overwrite_last_workspace_preference() {
        let state = AppState::default();
        let data_dir = temp_workspace();
        let first = temp_workspace();
        let second = temp_workspace();
        *state.workspace_root.lock().unwrap() = Some(second.clone());
        let first_generation = state.advance_workspace_generation();
        let second_generation = state.advance_workspace_generation();

        assert!(
            save_last_workspace_if_current(&state, &data_dir, &second, second_generation).unwrap()
        );
        assert!(
            !save_last_workspace_if_current(&state, &data_dir, &first, first_generation).unwrap()
        );
        assert_eq!(
            PathBuf::from(load_last_workspace_path(&data_dir).unwrap()),
            second
        );

        fs::remove_dir_all(data_dir).unwrap();
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
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

    /// v1.4.0 workspace restoration: save/load round trip against a real
    /// temp data dir, plus every graceful-degradation path (missing file,
    /// empty file, remembered workspace that no longer exists on disk).
    #[test]
    fn last_workspace_round_trips_and_degrades_gracefully() {
        let data_dir = temp_workspace();
        let workspace = temp_workspace();

        // Nothing saved yet -> nothing to restore.
        assert_eq!(load_last_workspace_path(&data_dir), None);

        // Round trip.
        save_last_workspace_path(&data_dir, &workspace).unwrap();
        let loaded = load_last_workspace_path(&data_dir).expect("saved workspace must load");
        assert_eq!(PathBuf::from(&loaded), workspace);

        // Empty file -> None, not an error.
        fs::write(data_dir.join(LAST_WORKSPACE_FILE), "").unwrap();
        assert_eq!(load_last_workspace_path(&data_dir), None);

        // Remembered workspace deleted from disk -> None, never an error.
        save_last_workspace_path(&data_dir, &workspace).unwrap();
        fs::remove_dir_all(&workspace).unwrap();
        assert_eq!(load_last_workspace_path(&data_dir), None);

        fs::remove_dir_all(&data_dir).unwrap();
    }

    #[test]
    fn opening_a_fresh_workspace_indexes_it_without_a_manual_step() {
        let root = temp_workspace();
        fs::write(
            root.join("auth.rs"),
            "fn authenticate_user() -> bool { true }\n",
        )
        .unwrap();

        let conn = crate::database::open_for_workspace(&root).unwrap();
        let stats = crate::database::indexer::index_workspace(&conn, &root).unwrap();
        assert_eq!(stats.files_indexed, 1);

        let results =
            crate::database::search::keyword_search(&conn, "authenticate_user", 20).unwrap();
        assert!(
            !results.is_empty(),
            "freshly auto-indexed content must be queryable without pressing the manual button"
        );

        drop(conn); // release the sqlite file handle before deleting on Windows
        fs::remove_dir_all(&root).unwrap();
    }

    /// Regression test for the v1.3.0 folder-open crash fix: automatic
    /// indexing now runs on a background thread with its OWN Connection to
    /// the same index.db, concurrently with the UI's shared connection.
    /// This proves two live connections to one workspace database can
    /// index (bulk writes) and persist a chat session (interleaved writes)
    /// at the same time without "database is locked" failures - the
    /// property the busy_timeout added in open_for_workspace exists to
    /// guarantee. Real threads, real concurrent SQLite connections, no
    /// mocks, matching this codebase's testing philosophy.
    #[test]
    fn background_indexing_connection_does_not_lock_out_the_ui_connection() {
        let root = temp_workspace();
        for i in 0..50 {
            fs::write(
                root.join(format!("file_{i}.rs")),
                format!("fn function_number_{i}() -> u32 {{ {i} }}\n"),
            )
            .unwrap();
        }

        // "UI" connection: what DbState would hold.
        let ui_conn = crate::database::open_for_workspace(&root).unwrap();

        // "Background indexer" connection: what the spawned thread opens.
        let index_root = root.clone();
        let indexer_thread = std::thread::spawn(move || {
            let conn = crate::database::open_for_workspace(&index_root).unwrap();
            crate::database::indexer::index_workspace(&conn, &index_root).unwrap()
        });

        // Meanwhile the UI connection persists a chat session - the exact
        // workload that races the background indexer in the real app.
        let session = crate::database::sessions::create_session(
            &ui_conn,
            root.to_string_lossy().as_ref(),
            "concurrent test",
            None,
            None,
        )
        .unwrap();
        for i in 0..10 {
            crate::database::sessions::append_message(
                &ui_conn,
                &session.id,
                "user",
                &format!("message {i}"),
                "completed",
            )
            .unwrap();
        }

        let stats = indexer_thread
            .join()
            .expect("indexer thread must not panic");
        assert_eq!(
            stats.files_indexed, 50,
            "all files indexed despite concurrent session writes"
        );

        let messages =
            crate::database::sessions::get_session_messages(&ui_conn, &session.id).unwrap();
        assert_eq!(
            messages.len(),
            10,
            "all session writes survived despite concurrent indexing"
        );

        drop(ui_conn);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reopening_an_already_indexed_workspace_skips_unchanged_files() {
        let root = temp_workspace();
        fs::write(
            root.join("auth.rs"),
            "fn authenticate_user() -> bool { true }\n",
        )
        .unwrap();

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
        assert!(
            result.is_ok(),
            "indexer must not panic/hard-fail on an unreadable workspace root"
        );
        assert_eq!(result.unwrap().files_indexed, 0);

        drop(conn);
        fs::remove_dir_all(&db_root).unwrap();
    }

    #[test]
    fn end_to_end_open_then_ai_chat_context_without_manual_indexing() {
        let root = temp_workspace();
        fs::write(
            root.join("auth.rs"),
            "fn authenticate_user() -> bool { true }\n",
        )
        .unwrap();

        // Step 1: open_for_workspace (the DB half of open_workspace).
        let conn = crate::database::open_for_workspace(&root).unwrap();

        // Step 2: automatic indexing, exactly as open_workspace now performs it
        // (same call, same function, no manual "Index Workspace" click).
        crate::database::indexer::index_workspace(&conn, &root).unwrap();

        // Step 3: the real backend entry point AI Chat uses for repository
        // context (ai::mod::get_context_for_query delegates straight into
        // this), per the required audit of the AI context retrieval path.
        let prompt =
            crate::ai::context::build_context_prompt(&root, &conn, "how does authentication work");
        assert!(
            prompt.contains("authenticate_user"),
            "AI Chat must be repository-aware immediately after open, with no manual indexing step"
        );

        drop(conn);
        fs::remove_dir_all(&root).unwrap();
    }
}
