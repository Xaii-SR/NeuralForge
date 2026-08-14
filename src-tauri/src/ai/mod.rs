pub mod autocomplete;
pub mod cache;
pub mod completion;
pub mod composer;
pub mod context;
pub mod credential_store;
pub mod docs;
pub mod git;
pub mod health;
pub mod inline;
pub mod model_manager;
pub mod provider_registry;
pub mod provider_router;
pub mod providers;
pub mod request_registry;
pub mod router;
pub mod web;

use crate::core::errors::{AppError, AppResult};
use crate::core::state::AppState;
use crate::database::DbState;
use health::{HealthRegistry, ProviderHealthInfo};
use providers::{ollama, openai_compatible, ProviderMetadata};
use router::{AutoSelection, CostEstimate, Preferences};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn cancel_ai_request(
    requests: State<'_, request_registry::RequestRegistry>,
    request_id: String,
) -> bool {
    requests.cancel(&request_id)
}

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
    crate::database::with_workspace_conn(&state, &db, |root, conn| {
        Ok(context::build_context_prompt(root, conn, &query))
    })
}

#[tauri::command]
pub fn get_enriched_context(
    state: State<AppState>,
    db: State<DbState>,
    query: String,
    max_tokens: usize,
) -> AppResult<String> {
    crate::database::with_workspace_conn(&state, &db, |root, conn| {
        let policy = crate::workspace_scanner::WorkspacePathPolicy::new(root)
            .map_err(AppError::InvalidPath)?;
        crate::database::indexer::purge_excluded_rows(conn, root, &policy)?;
        let memory = context::read_memory_context(root);
        let new_context = crate::database::search::enriched_context(
            conn, root, &query, &memory, None, max_tokens,
        )
        .map_err(|e| AppError::Provider(e.to_string()))?;

        let cached = crate::database::search::get_cached_context();
        let delta = crate::database::search::compute_context_diff(
            &cached.unwrap_or_default(),
            &new_context,
        );
        crate::database::search::cache_context_response(&new_context);

        if delta.is_delta {
            Ok(serde_json::to_string(&delta).unwrap_or(new_context))
        } else {
            Ok(new_context)
        }
    })
}

#[tauri::command]
pub fn save_preferences(
    db: State<DbState>,
    goal: String,
    cost_preference: String,
) -> AppResult<()> {
    let guard = db.conn.lock().unwrap();
    let conn = guard
        .as_ref()
        .ok_or_else(|| AppError::InvalidPath("no workspace open".to_string()))?;
    router::save_preferences(
        conn,
        &Preferences {
            goal,
            cost_preference,
        },
    )
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

/// Tests connectivity for any configured provider by dispatching to its
/// real adapter without returning its stored credential to the renderer.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    provider_id: String,
) -> Result<bool, String> {
    let provider = crate::database::with_workspace_conn(&state, &db, |_root, conn| {
        provider_registry::load_provider_by_id(conn, &provider_id).map_err(AppError::Provider)
    })
    .map_err(|error| error.to_string())?;
    provider_router::test_connection(&provider.provider_type, provider.base_url, provider.api_key)
        .await
}

#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    provider_id: String,
) -> Result<Vec<provider_router::ProviderModel>, String> {
    let config = crate::database::with_workspace_conn(&state, &db, |_root, conn| {
        provider_registry::load_provider_by_id(conn, &provider_id).map_err(AppError::Provider)
    })
    .map_err(|error| error.to_string())?;
    provider_router::list_models(&config)
        .await
        .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
pub struct ChatModelDescriptor {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub display_name: String,
    pub is_local: bool,
}

#[tauri::command]
pub async fn list_chat_models(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Vec<ChatModelDescriptor>, String> {
    let providers = crate::database::with_workspace_conn(&state, &db, |_root, conn| {
        Ok(provider_registry::load_providers(conn))
    })
    .map_err(|error| error.to_string())?;
    let mut descriptors = Vec::new();
    for provider in providers.into_iter().filter(|provider| {
        provider.enabled && provider.capabilities.chat && provider.capabilities.streaming
    }) {
        let is_local = provider.adapter_kind() == provider_registry::AdapterKind::Ollama;
        let models = if is_local {
            provider_router::list_models(&provider)
                .await
                .unwrap_or_default()
        } else {
            provider
                .models
                .iter()
                .map(|model| provider_router::ProviderModel {
                    id: model.clone(),
                    display_name: model.clone(),
                })
                .collect()
        };
        descriptors.extend(models.into_iter().map(|model| ChatModelDescriptor {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            model_id: model.id,
            display_name: model.display_name,
            is_local,
        }));
    }
    Ok(descriptors)
}

#[tauri::command]
pub async fn auto_select_model(
    db: State<'_, DbState>,
    health: State<'_, HealthRegistry>,
    prompt: String,
) -> AppResult<AutoSelection> {
    let prefs = {
        let guard = db.conn.lock().unwrap();
        guard
            .as_ref()
            .map(router::load_preferences)
            .unwrap_or_default()
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
    config: &provider_registry::ProviderConfig,
    model: &str,
    messages: &[ollama::ChatMessage],
    mut on_token: F,
) -> AppResult<(String, ollama::ChatStats)>
where
    F: FnMut(&str, bool),
{
    let health_key = provider_router::health_key_for(config);
    if !health.is_healthy(&health_key) {
        return Err(AppError::Provider(
            "Ollama is in cooldown after repeated failures - try again shortly".to_string(),
        ));
    }

    let models = ollama::list_models_at(&config.base_url).await?;
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
    let result =
        ollama::chat_stream_at(&config.base_url, model, messages.to_vec(), |token, done| {
            accumulated.push_str(token);
            on_token(token, done);
        })
        .await;

    match &result {
        Ok(_) => {
            health.record_success(&health_key, start.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(target: "ai", event = "chat_completed", model = %model);
        }
        Err(e) => {
            health.record_failure(&health_key);
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
    messages: &[ollama::ChatMessage],
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
    let response = if config.adapter_kind() == provider_registry::AdapterKind::Ollama {
        let (response, _stats) =
            chat_with_model_core(health, config, model, messages, &mut on_token).await?;
        response
    } else {
        provider_router::stream_cloud_chat(health, config, model, messages, &mut on_token).await?
    };
    Ok(Some(response))
}

fn prepare_chat_messages(
    config: &provider_registry::ProviderConfig,
    mut messages: Vec<ollama::ChatMessage>,
    share_workspace_context: bool,
) -> Vec<ollama::ChatMessage> {
    if config.adapter_kind() != provider_registry::AdapterKind::Ollama && !share_workspace_context {
        messages.retain(|message| !context::contains_workspace_context(&message.content));
    }
    context::budget_chat_messages(messages, config.capabilities.context_length)
}

#[tauri::command]
pub async fn chat_with_model(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    requests: State<'_, request_registry::RequestRegistry>,
    request_id: String,
    provider_id: String,
    model: String,
    messages: Vec<ollama::ChatMessage>,
    workspace_generation: Option<u64>,
    share_workspace_context: bool,
) -> AppResult<()> {
    let workspace_generation = workspace_generation.unwrap_or_else(|| state.workspace_generation());
    let cancelled = requests
        .begin(&request_id)
        .map_err(AppError::CommandRejected)?;
    let outcome = async {
        let resolved = crate::database::with_workspace_conn_at_generation(
            &state,
            &db,
            workspace_generation,
            |_root, conn| {
                provider_registry::resolve_provider_model(
                    conn,
                    &provider_id,
                    &model,
                    provider_registry::AiMode::Chat,
                )
                .map_err(AppError::Provider)
            },
        )?;
        let config = resolved.provider;
        let model = resolved.model;
        let messages = prepare_chat_messages(&config, messages, share_workspace_context);
        let effective_options = "default";
        let cached = crate::database::with_workspace_conn_at_generation(
            &state,
            &db,
            workspace_generation,
            |_root, conn| {
                Ok(cache::get_cached(
                    conn,
                    &config.id,
                    &model,
                    effective_options,
                    &messages,
                ))
            },
        )?;
        let was_cached = cached.is_some();
        let token_request_id = request_id.clone();
        let fresh = request_registry::run_cancellable(
            &cancelled,
            chat_or_use_cache(
                &health,
                cached,
                &config,
                &model,
                &messages,
                |token, _adapter_done| {
                    if token.is_empty()
                        || request_registry::is_cancelled(&cancelled)
                        || !state.matches_workspace_generation(workspace_generation)
                    {
                        return;
                    }
                    let _ = app.emit(
                        crate::core::events::AI_RESPONSE_TOKEN,
                        serde_json::json!({
                            "request_id": token_request_id,
                            "token": token,
                            "done": false,
                            "from_cache": was_cached,
                        }),
                    );
                },
            ),
        )
        .await?;

        if !state.matches_workspace_generation(workspace_generation) {
            return Err(AppError::CommandRejected(
                "workspace changed while chat generation was running".to_string(),
            ));
        }
        if request_registry::is_cancelled(&cancelled) {
            return Err(AppError::CommandRejected("request cancelled".to_string()));
        }
        if let Some(response) = fresh {
            crate::database::with_workspace_conn_at_generation(
                &state,
                &db,
                workspace_generation,
                |_root, conn| {
                    if let Err(e) = cache::store_response(
                        conn,
                        &config.id,
                        &model,
                        effective_options,
                        &messages,
                        &response,
                    ) {
                        tracing::warn!(target: "ai", event = "cache_store_failed", error = %e);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    }
    .await;

    let (status, error) = match &outcome {
        Ok(()) => ("success", None),
        Err(AppError::CommandRejected(message)) if message == "request cancelled" => {
            ("cancelled", None)
        }
        Err(error) => ("error", Some(error.to_string())),
    };
    let _ = app.emit(
        crate::core::events::AI_RESPONSE_TOKEN,
        serde_json::json!({
            "request_id": request_id,
            "token": "",
            "done": true,
            "status": status,
            "error": error,
        }),
    );
    requests.finish(&request_id);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_chat_drops_workspace_context_without_explicit_consent() {
        let mut cloud = provider_registry::default_ollama_provider();
        cloud.provider_type = "openai_compatible".to_string();
        cloud.capabilities.context_length = 4096;
        let messages = vec![
            ollama::ChatMessage {
                role: "system".into(),
                content:
                    "<untrusted_workspace_context>NF_CONTEXT_SENTINEL</untrusted_workspace_context>"
                        .into(),
            },
            ollama::ChatMessage {
                role: "user".into(),
                content: "hello".into(),
            },
        ];
        let blocked = prepare_chat_messages(&cloud, messages.clone(), false);
        assert!(!blocked
            .iter()
            .any(|message| message.content.contains("NF_CONTEXT_SENTINEL")));
        let allowed = prepare_chat_messages(&cloud, messages, true);
        assert!(allowed
            .iter()
            .any(|message| message.content.contains("NF_CONTEXT_SENTINEL")));
    }

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
            &provider_registry::default_ollama_provider(),
            "deepseek-coder:latest",
            &[ollama::ChatMessage {
                role: "user".to_string(),
                content: "What is Rust? Answer in one short sentence.".to_string(),
            }],
            |token, _done| streamed.push_str(token),
        )
        .await;

        assert!(
            result.is_ok(),
            "chat_with_model_core failed: {:?}",
            result.err()
        );
        let (accumulated, stats) = result.unwrap();
        assert!(
            !accumulated.trim().is_empty(),
            "expected non-empty accumulated response"
        );
        assert_eq!(
            accumulated, streamed,
            "accumulated response should match what was streamed"
        );
        assert!(
            stats.eval_count.is_some(),
            "expected real Ollama generation stats"
        );

        let snapshot = health.snapshot();
        let ollama_health = snapshot
            .iter()
            .find(|h| h.provider == "ollama")
            .expect("expected an ollama health entry after a successful chat");
        assert_eq!(
            ollama_health.failure_count, 0,
            "expected zero failures after a successful chat"
        );
        assert!(
            ollama_health.avg_latency_ms.is_some(),
            "expected latency to be recorded"
        );

        // Give the non-blocking file writer a moment to flush before reading back.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let log_content =
            std::fs::read_to_string(log_dir.join("app.log")).expect("failed to read log file");
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
        assert!(cache::get_cached(&conn, "default-ollama", model, "default", &messages).is_none());
        let start1 = std::time::Instant::now();
        let mut streamed1 = String::new();
        let fresh1 = chat_or_use_cache(&health, None, &config, model, &messages, |t, _d| {
            streamed1.push_str(t)
        })
        .await
        .unwrap();
        let elapsed1 = start1.elapsed();
        let response1 =
            fresh1.expect("first call should be a cache miss producing a fresh response");
        cache::store_response(
            &conn,
            "default-ollama",
            model,
            "default",
            &messages,
            &response1,
        )
        .unwrap();

        // Second call: real cache hit.
        let cached2 = cache::get_cached(&conn, "default-ollama", model, "default", &messages);
        assert!(cached2.is_some());
        let start2 = std::time::Instant::now();
        let mut streamed2 = String::new();
        let fresh2 = chat_or_use_cache(&health, cached2, &config, model, &messages, |t, _d| {
            streamed2.push_str(t)
        })
        .await
        .unwrap();
        let elapsed2 = start2.elapsed();

        assert!(
            fresh2.is_none(),
            "second call should be a cache hit, not a fresh generation"
        );
        assert_eq!(
            streamed2, response1,
            "cached response should match the original"
        );
        assert!(
            elapsed2 < elapsed1 / 2,
            "cache hit ({elapsed2:?}) should be dramatically faster than real generation ({elapsed1:?})"
        );

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_does_not_cross_provider_boundaries() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_cache_provider_isolation_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();

        let messages = vec![ollama::ChatMessage {
            role: "user".into(),
            content: "hello".into(),
        }];

        // Store a response for provider-a.
        cache::store_response(
            &conn,
            "provider-a",
            "model-x",
            "default",
            &messages,
            "response-from-a",
        )
        .unwrap();

        // provider-b should NOT see provider-a's cache entry.
        assert!(cache::get_cached(&conn, "provider-b", "model-x", "default", &messages).is_none());

        // Even with the same model, different provider is a miss.
        assert!(cache::get_cached(&conn, "provider-b", "model-x", "default", &messages).is_none());

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
