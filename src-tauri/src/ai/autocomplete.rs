use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry;
use crate::ai::provider_router;
use crate::database::DbState;
use tauri::State;

/// Returns a ghost text suggestion using the capability-gated FIM routing
/// in `ai::provider_router::complete_fim`. Formats the editor context into
/// a Fill-in-the-Middle prompt; no direct HTTP client, hardcoded URL, or
/// hardcoded model here - provider_router owns selection, resolution,
/// health, and telemetry. Falls back to empty string on failure, matching
/// this command's original behavior (a failed ghost-text suggestion should
/// never surface an error to the editor, just show nothing).
#[tauri::command]
pub async fn fetch_ghost_suggestion(
    health: State<'_, HealthRegistry>,
    db: State<'_, DbState>,
    prefix: String,
    suffix: String,
    file_path: String,
) -> Result<String, String> {
    let fim_prompt = format!(
        "<|fim_prefix|>{}<|fim_suffix|>{}<|fim_middle|>",
        prefix, suffix
    );

    let providers = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        guard.as_ref().map(provider_registry::load_providers).unwrap_or_default()
        // guard dropped here, before the .await below
    };

    match provider_router::complete_fim(&providers, &health, &fim_prompt, 64, 0.1).await {
        Ok(text) => {
            // Strip markdown code fences if present
            let cleaned = text
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();
            tracing::info!(target: "ai", event = "ghost_suggestion_ok", file_path = %file_path);
            Ok(cleaned)
        }
        Err(e) => {
            tracing::warn!(target: "ai", event = "ghost_suggestion_failed", error = %e, file_path = %file_path);
            Ok(String::new())
        }
    }
}
