pub mod context;
pub mod health;
pub mod model_manager;
pub mod providers;

use crate::core::errors::{AppError, AppResult};
use crate::core::state::AppState;
use crate::database::DbState;
use health::{HealthRegistry, ProviderHealthInfo};
use providers::{ollama, ProviderMetadata};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn ollama_health_check() -> bool {
    ollama::health_check().await
}

#[tauri::command]
pub async fn list_models() -> AppResult<Vec<ollama::OllamaModel>> {
    ollama::list_models().await
}

#[tauri::command]
pub async fn pull_model(app: AppHandle, name: String) -> AppResult<()> {
    ollama::pull_model(&app, &name).await
}

#[tauri::command]
pub async fn remove_model(name: String) -> AppResult<()> {
    ollama::remove_model(&name).await
}

#[tauri::command]
pub fn list_providers() -> Vec<ProviderMetadata> {
    providers::registry()
}

#[tauri::command]
pub fn get_provider_health(health: State<HealthRegistry>) -> Vec<ProviderHealthInfo> {
    health.snapshot()
}

#[tauri::command]
pub fn check_vram_for_model(
    parameter_size: String,
    quantization_level: String,
) -> model_manager::VramCheckResult {
    let hardware = crate::hardware::detect_all();
    model_manager::check(&parameter_size, &quantization_level, &hardware)
}

#[tauri::command]
pub fn get_context_for_query(
    state: State<AppState>,
    db: State<DbState>,
    query: String,
) -> AppResult<String> {
    let root = state
        .workspace_root
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    let db_guard = db.conn.lock().unwrap();
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;

    Ok(context::build_context_prompt(&root, conn, &query))
}

/// Pure core: model lookup -> VRAM gate -> health-cooldown check -> stream ->
/// record health + log. Decoupled from AppHandle so it's testable without a
/// live Tauri runtime (same pattern as ollama::chat_stream).
async fn chat_with_model_core<F>(
    health: &HealthRegistry,
    model: &str,
    messages: Vec<ollama::ChatMessage>,
    on_token: F,
) -> AppResult<()>
where
    F: FnMut(&str, bool),
{
    if !health.is_healthy("ollama") {
        return Err(AppError::Provider(
            "Ollama is in cooldown after repeated failures - try again shortly".to_string(),
        ));
    }

    let models = ollama::list_models().await?;
    if let Some(info) = models.iter().find(|m| m.name == model) {
        let hardware = crate::hardware::detect_all();
        let vram = model_manager::check(&info.parameter_size, &info.quantization_level, &hardware);
        if !vram.sufficient {
            tracing::warn!(target: "ai", event = "model_load_refused", model = %model, required_mb = vram.required_mb, available_mb = vram.available_mb);
            return Err(AppError::InsufficientResources(vram.message));
        }
    }

    let start = std::time::Instant::now();
    let result = ollama::chat_stream(model, messages, on_token).await;
    match &result {
        Ok(()) => {
            health.record_success("ollama", start.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(target: "ai", event = "chat_completed", model = %model);
        }
        Err(e) => {
            health.record_failure("ollama");
            tracing::warn!(target: "ai", event = "chat_failed", model = %model, error = %e);
        }
    }
    result
}

#[tauri::command]
pub async fn chat_with_model(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    request_id: String,
    model: String,
    messages: Vec<ollama::ChatMessage>,
) -> AppResult<()> {
    chat_with_model_core(&health, &model, messages, move |token, done| {
        let _ = app.emit(
            crate::core::events::AI_RESPONSE_TOKEN,
            serde_json::json!({
                "request_id": request_id,
                "token": token,
                "done": done,
            }),
        );
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the exact logic the chat_with_model command runs - not just
    /// the low-level HTTP stream - against a real running Ollama instance:
    /// model lookup, VRAM gate, health-registry recording, and the tracing
    /// log line that LogViewer reads. No Tauri runtime needed (see
    /// chat_with_model_core's doc comment for why). Requires
    /// deepseek-coder:latest to be pulled locally.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn chat_with_model_core_logs_and_records_health() {
        let mut log_dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        log_dir.push(format!("neuralforge_chat_log_test_{nanos}"));
        let _guard = crate::core::logging::init(&log_dir).expect("failed to init logging");

        let health = HealthRegistry::default();
        let mut accumulated = String::new();

        let result = chat_with_model_core(
            &health,
            "deepseek-coder:latest",
            vec![ollama::ChatMessage {
                role: "user".to_string(),
                content: "What is Rust? Answer in one short sentence.".to_string(),
            }],
            |token, _done| accumulated.push_str(token),
        )
        .await;

        assert!(result.is_ok(), "chat_with_model_core failed: {:?}", result.err());
        assert!(!accumulated.trim().is_empty(), "expected non-empty streamed content");

        let snapshot = health.snapshot();
        let ollama_health = snapshot
            .iter()
            .find(|h| h.provider == "ollama")
            .expect("expected an ollama health entry after a successful chat");
        assert_eq!(ollama_health.failure_count, 0, "expected zero failures after a successful chat");
        assert!(ollama_health.avg_latency_ms.is_some(), "expected latency to be recorded");

        // Give the non-blocking file writer a moment to flush before reading back.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let log_content = std::fs::read_to_string(log_dir.join("app.log")).expect("failed to read log file");
        assert!(
            log_content.contains("\"event\":\"chat_completed\""),
            "expected a chat_completed log entry, got: {log_content}"
        );
        assert!(
            log_content.contains("deepseek-coder:latest"),
            "expected the model name in the log entry, got: {log_content}"
        );

        std::fs::remove_dir_all(&log_dir).ok();
    }
}
