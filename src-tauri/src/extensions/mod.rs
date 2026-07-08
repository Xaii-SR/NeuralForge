pub mod api;
pub mod loader;
pub mod manifest;

use crate::core::errors::{AppError, AppResult};
use manifest::InstalledExtension;
use std::collections::HashMap;

pub fn ensure_and_scan() -> AppResult<Vec<InstalledExtension>> {
    let dir = loader::extensions_dir()?;
    std::fs::create_dir_all(&dir)?;
    loader::ensure_bundled_extensions(&dir)?;
    loader::scan(&dir)
}

#[tauri::command]
pub fn list_extensions() -> AppResult<Vec<InstalledExtension>> {
    ensure_and_scan()
}

#[tauri::command]
pub fn set_extension_enabled(name: String, enabled: bool) -> AppResult<()> {
    let dir = loader::extensions_dir()?;
    let extensions = loader::scan(&dir)?;
    let mut state: HashMap<String, bool> = extensions.into_iter().map(|e| (e.manifest.name, e.enabled)).collect();
    state.insert(name, enabled);
    loader::save_enabled_state(&dir, &state)
}

#[tauri::command]
pub fn uninstall_extension(name: String) -> AppResult<()> {
    let dir = loader::extensions_dir()?;
    let target = dir.join(&name);
    let canonical_dir = std::fs::canonicalize(&dir)?;
    let canonical_target = std::fs::canonicalize(&target).map_err(|_| AppError::NotFound(name.clone()))?;
    if !canonical_target.starts_with(&canonical_dir) || canonical_target == canonical_dir {
        return Err(AppError::InvalidPath(format!("{name} is not a valid extension directory")));
    }
    std::fs::remove_dir_all(canonical_target)?;
    tracing::info!(target: "extensions", event = "extension_uninstalled", name = %name);
    Ok(())
}

#[tauri::command]
pub async fn run_extension(name: String, request: serde_json::Value) -> AppResult<api::ExtensionResult> {
    let dir = loader::extensions_dir()?;
    let extensions = loader::scan(&dir)?;
    let ext = extensions
        .into_iter()
        .find(|e| e.manifest.name == name)
        .ok_or_else(|| AppError::NotFound(name.clone()))?;

    if !ext.enabled {
        return Err(AppError::Provider(format!("extension '{name}' is disabled")));
    }

    let result = api::invoke_extension(&ext, request).await;
    match &result {
        Ok(r) => tracing::info!(target: "extensions", event = "extension_run", name = %name, success = r.success),
        Err(e) => tracing::warn!(target: "extensions", event = "extension_run_failed", name = %name, error = %e),
    }
    result
}
