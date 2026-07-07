pub mod health;
pub mod model_manager;
pub mod providers;

use crate::core::errors::{AppError, AppResult};
use health::{HealthRegistry, ProviderHealthInfo};
use providers::{ollama, ProviderMetadata};
use tauri::{AppHandle, State};

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
pub async fn chat_with_model(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    request_id: String,
    model: String,
    messages: Vec<ollama::ChatMessage>,
) -> AppResult<()> {
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
    let result = ollama::chat(&app, &request_id, &model, messages).await;
    match &result {
        Ok(()) => {
            health.record_success("ollama", start.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(target: "ai", event = "chat_completed", model = %model, request_id = %request_id);
        }
        Err(e) => {
            health.record_failure("ollama");
            tracing::warn!(target: "ai", event = "chat_failed", model = %model, error = %e);
        }
    }
    result
}
