use crate::core::errors::AppResult;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::prelude::*;

const LOG_FILE_NAME: &str = "app.log";

/// Fixed filename (no rotation) for Phase 1: keeps get_recent_logs/export_logs
/// trivial to point at a single known path. Rotation policy can be layered on
/// later without touching the read-back commands below.
pub fn init(log_dir: &Path) -> std::io::Result<WorkerGuard> {
    std::fs::create_dir_all(log_dir)?;
    let file_appender = RollingFileAppender::new(Rotation::NEVER, log_dir, LOG_FILE_NAME);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking);

    let stdout_layer = tracing_subscriber::fmt::layer().with_target(true);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(file_layer)
        .with(stdout_layer)
        .init();

    Ok(guard)
}

fn log_file_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| crate::core::errors::AppError::InvalidPath(e.to_string()))?;
    Ok(dir.join(LOG_FILE_NAME))
}

#[tauri::command]
pub fn get_recent_logs(app: AppHandle, lines: usize) -> AppResult<Vec<String>> {
    let path = log_file_path(&app)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(path)?;
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(lines);
    Ok(all[start..].iter().map(|s| s.to_string()).collect())
}

#[tauri::command]
pub fn export_logs(app: AppHandle, destination: String) -> AppResult<()> {
    let path = log_file_path(&app)?;
    std::fs::copy(path, destination)?;
    Ok(())
}
