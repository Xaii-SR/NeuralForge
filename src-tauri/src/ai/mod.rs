pub mod autocomplete;
pub mod cache;
pub mod completion;
pub mod composer;
pub mod docs;
pub mod git;
pub mod context;
pub mod health;
pub mod inline;
pub mod web;
pub mod model_manager;
pub mod provider_registry;
pub mod provider_router;
pub mod providers;
pub mod router;

use crate::core::errors::{AppError, AppResult};
use crate::core::state::AppState;
use crate::database::DbState;
use health::{HealthRegistry, ProviderHealthInfo};
use providers::{ollama, openai_compatible, ProviderMetadata};
use router::{AutoSelection, CostEstimate, Preferences};
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

#[tauri::command]
pub fn get_enriched_context(
    state: State<AppState>,
    db: State<DbState>,
    query: String,
    max_tokens: usize,
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

    let memory = context::read_memory_context(&root);
    let new_context = crate::database::search::enriched_context(conn, &root, &query, &memory, None, max_tokens)
        .map_err(|e| AppError::Provider(e.to_string()))?;

    // Diff against cached context for delta compression
    let cached = crate::database::search::get_cached_context();
    let delta = crate::database::search::compute_context_diff(&cached.unwrap_or_default(), &new_context);
    crate::database::search::cache_context_response(&new_context);

    if delta.is_delta {
        Ok(serde_json::to_string(&delta).unwrap_or(new_context))
    } else {
        Ok(new_context)
    }
}

#[tauri::command]
pub fn save_preferences(db: State<DbState>, goal: String, cost_preference: String) -> AppResult<()> {
    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
    router::save_preferences(conn, &Preferences { goal, cost_preference })
}

#[tauri::command]
pub fn get_preferences(db: State<DbState>) -> Preferences {
    let guard = db.conn.lock().unwrap();
    match guard.as_ref() {
        Some(conn) => router::load_preferences(conn),
        None => Preferences::default(),
    }
}

#[tauri::command]
pub fn estimate_cost_for_prompt(prompt: String) -> CostEstimate {
    router::estimate_cost(&providers::ProviderId::Ollama, &prompt)
}

#[tauri::command]
pub fn clear_response_cache(db: State<DbState>) -> AppResult<usize> {
    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
    cache::clear_cache(conn)
}

#[tauri::command]
pub async fn test_openai_compatible_connection(
    base_url: String,
    api_key: String,
) -> Result<bool, String> {
    let provider = openai_compatible::OpenAiCompatibleProvider::new(base_url, api_key);
    Ok(provider.health_check().await)
}

#[tauri::command]
pub async fn list_openai_compatible_models(
    base_url: String,
    api_key: String,
) -> Result<Vec<openai_compatible::OpenAiModel>, String> {
    let provider = openai_compatible::OpenAiCompatibleProvider::new(base_url, api_key);
    provider.list_models().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn auto_select_model(
    db: State<'_, DbState>,
    health: State<'_, HealthRegistry>,
    prompt: String,
) -> AppResult<AutoSelection> {
    let prefs = {
        let guard = db.conn.lock().unwrap();
        guard.as_ref().map(router::load_preferences).unwrap_or_default()
    };

    let models = ollama::list_models().await?;

    let selection = router::select_model(&models, &health, &prefs, &prompt)?;
    tracing::info!(
        target: "ai",
        event = "auto_selected",
        provider = %selection.provider,
        model = %selection.model,
        reason = %selection.reason
    );
    Ok(selection)
}

/// Pure core: model lookup -> VRAM gate -> health-cooldown check -> stream ->
/// record health + log. Decoupled from AppHandle so it's testable without a
/// live Tauri runtime (same pattern as ollama::chat_stream). Returns the
/// full accumulated response text (for caching) alongside Ollama's real
/// generation stats (proof the response came from a genuine generation).
async fn chat_with_model_core<F>(
    health: &HealthRegistry,
    model: &str,
    messages: Vec<ollama::ChatMessage>,
    mut on_token: F,
) -> AppResult<(String, ollama::ChatStats)>
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
    let mut accumulated = String::new();
    let result = ollama::chat_stream(model, messages, |token, done| {
        accumulated.push_str(token);
        on_token(token, done);
    })
    .await;

    match &result {
        Ok(_) => {
            health.record_success("ollama", start.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(target: "ai", event = "chat_completed", model = %model);
        }
        Err(e) => {
            health.record_failure("ollama");
            tracing::warn!(target: "ai", event = "chat_failed", model = %model, error = %e);
        }
    }

    result.map(|stats| (accumulated, stats))
}

/// Checks a pre-fetched cache value; on hit, emits it as a single instant
/// "token" and returns None (nothing new to cache). On miss, streams a real
/// generation via chat_with_model_core and returns Some(response) for the
/// caller to store. Takes an owned Option<String> rather than a &Connection
/// deliberately: #[tauri::command] futures must be Send, and rusqlite's
/// Connection is Send but not Sync, so a borrowed &Connection can't be held
/// across the .await inside this function. Keeping DB reads/writes in the
/// caller (before/after, never spanning the await) is what makes both the
/// real command and this function Send-safe.
async fn chat_or_use_cache<F>(
    health: &HealthRegistry,
    cached: Option<String>,
    config: &provider_registry::ProviderConfig,
    model: &str,
    messages: Vec<ollama::ChatMessage>,
    mut on_token: F,
) -> AppResult<Option<String>>
where
    F: FnMut(&str, bool),
{
    if let Some(response) = cached {
        tracing::info!(target: "ai", event = "cache_hit", model = %model);
        on_token(&response, true);
        return Ok(None);
    }
    tracing::info!(target: "ai", event = "cache_miss", model = %model);

    // The Ollama path goes through chat_with_model_core exactly as it
    // always has (VRAM gating, "ollama" health key, existing log lines and
    // tests are all pinned to this function). Every other configured
    // provider routes through provider_router, which owns adapter selection
    // for anything non-Ollama - see its module doc for why the two paths
    // aren't merged into one generic function.
    let response = if config.provider_type == "ollama" {
        let (response, _stats) = chat_with_model_core(health, model, messages, &mut on_token).await?;
        response
    } else {
        provider_router::stream_cloud_chat(health, config, model, messages, &mut on_token).await?
    };
    Ok(Some(response))
}

#[tauri::command]
pub async fn chat_with_model(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    db: State<'_, DbState>,
    request_id: String,
    model: String,
    messages: Vec<ollama::ChatMessage>,
) -> AppResult<()> {
    let (cached, config) = {
        let guard = db.conn.lock().unwrap();
        let conn = guard.as_ref();
        let cached = conn.and_then(|conn| cache::get_cached(conn, &model, &messages));
        let config = provider_router::resolve_provider_for_model(conn, &model);
        (cached, config)
    };
    let was_cached = cached.is_some();

    let fresh = chat_or_use_cache(&health, cached, &config, &model, messages.clone(), move |token, done| {
        let _ = app.emit(
            crate::core::events::AI_RESPONSE_TOKEN,
            serde_json::json!({
                "request_id": request_id,
                "token": token,
                "done": done,
                "from_cache": was_cached,
            }),
        );
    })
    .await?;

    if let Some(response) = fresh {
        let guard = db.conn.lock().unwrap();
        if let Some(conn) = guard.as_ref() {
            if let Err(e) = cache::store_response(conn, &model, &messages, &response) {
                tracing::warn!(target: "ai", event = "cache_store_failed", error = %e);
            }
        }
    }

    Ok(())
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
        let mut streamed = String::new();

        let result = chat_with_model_core(
            &health,
            "deepseek-coder:latest",
            vec![ollama::ChatMessage {
                role: "user".to_string(),
                content: "What is Rust? Answer in one short sentence.".to_string(),
            }],
            |token, _done| streamed.push_str(token),
        )
        .await;

        assert!(result.is_ok(), "chat_with_model_core failed: {:?}", result.err());
        let (accumulated, stats) = result.unwrap();
        assert!(!accumulated.trim().is_empty(), "expected non-empty accumulated response");
        assert_eq!(accumulated, streamed, "accumulated response should match what was streamed");
        assert!(stats.eval_count.is_some(), "expected real Ollama generation stats");

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

    /// Gate test: "same question twice -> second uses cache (instant
    /// response)". First call is a real cache miss hitting Ollama; second
    /// call with identical model+messages must be a cache hit - verified by
    /// (a) chat_or_use_cache returning None (nothing fresh to cache), (b)
    /// identical content, and (c) the second call being dramatically faster
    /// than the first, proving no real generation happened.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn second_identical_chat_uses_cache_and_is_dramatically_faster() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_cache_gate_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        let health = HealthRegistry::default();
        let config = provider_registry::default_ollama_provider();
        let model = "deepseek-coder:latest";
        let messages = vec![ollama::ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly the word: hello".to_string(),
        }];

        // First call: real cache miss.
        assert!(cache::get_cached(&conn, model, &messages).is_none());
        let start1 = std::time::Instant::now();
        let mut streamed1 = String::new();
        let fresh1 = chat_or_use_cache(&health, None, &config, model, messages.clone(), |t, _d| streamed1.push_str(t))
            .await
            .unwrap();
        let elapsed1 = start1.elapsed();
        let response1 = fresh1.expect("first call should be a cache miss producing a fresh response");
        cache::store_response(&conn, model, &messages, &response1).unwrap();

        // Second call: real cache hit.
        let cached2 = cache::get_cached(&conn, model, &messages);
        assert!(cached2.is_some());
        let start2 = std::time::Instant::now();
        let mut streamed2 = String::new();
        let fresh2 = chat_or_use_cache(&health, cached2, &config, model, messages.clone(), |t, _d| streamed2.push_str(t))
            .await
            .unwrap();
        let elapsed2 = start2.elapsed();

        assert!(fresh2.is_none(), "second call should be a cache hit, not a fresh generation");
        assert_eq!(streamed2, response1, "cached response should match the original");
        assert!(
            elapsed2 < elapsed1 / 2,
            "cache hit ({elapsed2:?}) should be dramatically faster than real generation ({elapsed1:?})"
        );

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}