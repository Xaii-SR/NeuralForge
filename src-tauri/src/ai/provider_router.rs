//! Unified provider routing layer.
//!
//! This is the ONLY place that decides which HTTP adapter (Ollama vs. the
//! generic OpenAI-compatible client vs. a not-yet-implemented native client)
//! handles a given chat request. Every AI feature - chat, inline edit, future
//! agent/council work - must resolve a provider through this module rather
//! than constructing its own client. See ai/provider_registry.rs for the
//! persisted provider configs this module reads.

use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry::{self, AdapterKind, ProviderConfig};
use crate::ai::providers::{anthropic, gemini, ollama, openai_compatible};
use crate::core::errors::{AppError, AppResult};
use rusqlite::Connection;
use std::time::Instant;

// AdapterKind and its classification (`adapter_kind_for` / `ProviderConfig::
// adapter_kind()`) live in `provider_registry` now - it's registry-level
// data classification (what can this persisted provider_type do?), and
// keeping it there lets `provider_registry` enforce capability clamping at
// construction time without depending on this module. `provider_router`
// re-exports `AdapterKind` (via the `use` above) since it's still the type
// every dispatch decision in this file is written against.

/// Health/cooldown key for a provider. Ollama keeps its historical "ollama"
/// key unchanged (existing UI and tests read this key directly); every other
/// provider gets its own independent cooldown tracked by config id, so one
/// degraded cloud provider never affects Ollama's or another provider's
/// health state.
pub fn health_key_for(config: &ProviderConfig) -> String {
    if config.adapter_kind() == AdapterKind::Ollama {
        "ollama".to_string()
    } else {
        format!("provider:{}", config.id)
    }
}

/// Resolves which configured provider owns `model`. Any provider whose
/// `models` list contains an exact match wins; Ollama's own model catalog is
/// never mirrored into provider_registry, so no match falls back to the
/// default local Ollama provider - this preserves today's behavior where
/// "just type an Ollama model name" works with zero provider configuration.
pub fn resolve_provider_for_model(conn: Option<&Connection>, model: &str) -> ProviderConfig {
    let providers = match conn {
        Some(conn) => provider_registry::load_providers(conn),
        None => vec![provider_registry::default_ollama_provider()],
    };
    let resolved = providers
        .into_iter()
        .find(|p| p.enabled && p.models.iter().any(|m| m == model))
        .unwrap_or_else(provider_registry::default_ollama_provider);
    tracing::debug!(
        target: "ai",
        event = "provider_resolved_for_model",
        model = %model,
        provider = %resolved.name,
        provider_type = %resolved.provider_type,
    );
    resolved
}

/// Fast-path streaming entry point for latency-sensitive, already-resolved
/// callers (inline edit, ghost text) that just need "stream this exact
/// model, whichever adapter it belongs to" without the full chat pipeline's
/// cache/VRAM-gate/request-id bookkeeping. Takes an already-resolved
/// `ProviderConfig` (resolve it synchronously via `resolve_provider_for_model`
/// before calling this - see that function's doc comment on why a
/// `Connection` guard can't cross this function's await points) so this
/// stays a single dispatch decision, not a second routing system: Ollama
/// goes straight to the same `providers::ollama::chat_stream` adapter
/// `ai::chat_with_model_core` uses, everything else delegates to
/// `stream_cloud_chat` below.
pub async fn stream_chat<F>(
    health: &HealthRegistry,
    config: &ProviderConfig,
    model: &str,
    messages: Vec<ollama::ChatMessage>,
    mut on_token: F,
) -> AppResult<String>
where
    F: FnMut(&str, bool),
{
    if config.adapter_kind() != AdapterKind::Ollama {
        return stream_cloud_chat(health, config, model, messages, on_token).await;
    }

    let health_key = health_key_for(config);
    if !health.is_healthy(&health_key) {
        return Err(AppError::Provider(format!(
            "{} is in cooldown after repeated failures - try again shortly",
            config.name
        )));
    }

    let start = Instant::now();
    let mut accumulated = String::new();
    let result = ollama::chat_stream(model, messages, |token, done| {
        if !token.is_empty() {
            accumulated.push_str(token);
        }
        on_token(token, done);
    })
    .await;

    match &result {
        Ok(_) => health.record_success(&health_key, start.elapsed().as_secs_f64() * 1000.0),
        Err(_) => health.record_failure(&health_key),
    }
    result.map(|_| accumulated)
}

/// Streams a chat completion through whichever adapter `config.provider_type`
/// maps to. Ollama's own request path (VRAM gating, model discovery) is
/// intentionally NOT duplicated here - see `ai::chat_with_model_core` in
/// `ai/mod.rs`, which callers still use directly for the Ollama case so its
/// exact existing behavior (and the tests pinned to it) are untouched. This
/// function only handles the non-Ollama adapters.
pub async fn stream_cloud_chat<F>(
    health: &HealthRegistry,
    config: &ProviderConfig,
    model: &str,
    messages: Vec<ollama::ChatMessage>,
    mut on_token: F,
) -> AppResult<String>
where
    F: FnMut(&str, bool),
{
    let health_key = health_key_for(config);
    if !health.is_healthy(&health_key) {
        return Err(AppError::Provider(format!(
            "{} is in cooldown after repeated failures - try again shortly",
            config.name
        )));
    }

    let kind = config.adapter_kind();
    tracing::debug!(target: "ai", event = "provider_dispatch_decision", provider = %config.name, provider_type = %config.provider_type, adapter_kind = ?kind);
    let start = Instant::now();

    let result: AppResult<String> = match kind {
        AdapterKind::Ollama => {
            // Callers route Ollama through chat_with_model_core directly;
            // reaching here means a provider config is misclassified.
            Err(AppError::Provider(
                "internal routing error: Ollama config reached stream_cloud_chat".to_string(),
            ))
        }
        AdapterKind::Unimplemented => Err(AppError::Provider(format!(
            "{} does not have a native adapter yet - only Ollama and OpenAI-compatible providers are supported today",
            config.provider_type
        ))),
        AdapterKind::OpenAiCompatible => {
            let client = openai_compatible::OpenAiCompatibleProvider::new(
                config.base_url.clone(),
                config.api_key.clone(),
            );
            let oc_messages: Vec<openai_compatible::ChatMessage> = messages
                .into_iter()
                .map(|m| openai_compatible::ChatMessage { role: m.role, content: m.content })
                .collect();

            let mut accumulated = String::new();
            client
                .chat_stream(model, oc_messages, |token, done| {
                    if !token.is_empty() {
                        accumulated.push_str(token);
                    }
                    on_token(token, done);
                })
                .await
                .map(|_stats| accumulated)
        }
        AdapterKind::Anthropic => {
            let client = anthropic::AnthropicProvider::new(
                config.base_url.clone(),
                config.api_key.clone(),
            );
            let anthropic_messages: Vec<anthropic::ChatMessage> = messages
                .into_iter()
                .map(|m| anthropic::ChatMessage { role: m.role, content: m.content })
                .collect();

            let mut accumulated = String::new();
            client
                .chat_stream(model, anthropic_messages, |token, done| {
                    if !token.is_empty() {
                        accumulated.push_str(token);
                    }
                    on_token(token, done);
                })
                .await
                .map(|_stats| accumulated)
        }
        AdapterKind::Gemini => {
            let client = gemini::GeminiProvider::new(
                config.base_url.clone(),
                config.api_key.clone(),
            );
            let gemini_messages: Vec<gemini::ChatMessage> = messages
                .into_iter()
                .map(|m| gemini::ChatMessage { role: m.role, content: m.content })
                .collect();

            let mut accumulated = String::new();
            client
                .chat_stream(model, gemini_messages, |token, done| {
                    if !token.is_empty() {
                        accumulated.push_str(token);
                    }
                    on_token(token, done);
                })
                .await
                .map(|_stats| accumulated)
        }
    };

    match &result {
        Ok(_) => health.record_success(&health_key, start.elapsed().as_secs_f64() * 1000.0),
        Err(e) => {
            health.record_failure(&health_key);
            tracing::warn!(target: "ai", event = "provider_chat_failed", provider = %config.name, error = %e);
        }
    }

    result
}

/// Tests connectivity for `provider_type` by dispatching to that adapter's
/// real `health_check()` - the single place connection testing decides
/// which adapter to use, mirroring `stream_cloud_chat`'s `AdapterKind`
/// dispatch above. `base_url`/`api_key` are ignored for `Ollama` (it has no
/// concept of either - always localhost, no auth) the same way
/// `stream_chat` already treats Ollama as its own path. `Unimplemented`
/// fails loudly with an `Err` rather than silently reporting success or
/// running the wrong adapter's check against it.
pub async fn test_connection(provider_type: &str, base_url: String, api_key: String) -> Result<bool, String> {
    let kind = provider_registry::adapter_kind_for(provider_type);
    test_connection_for_kind(kind, provider_type, base_url, api_key).await
}

/// The actual per-`AdapterKind` dispatch behind `test_connection`, split out
/// so `AdapterKind::Unimplemented`'s branch is directly testable - no live
/// `provider_type` string maps to `Unimplemented` today (see
/// `provider_registry::adapter_kind_for`'s unmatched-string fallback to
/// `OpenAiCompatible`), the same reachability gap `provider_registry`'s own
/// tests hit and worked around by testing `max_capabilities_for` directly.
async fn test_connection_for_kind(kind: AdapterKind, provider_type: &str, base_url: String, api_key: String) -> Result<bool, String> {
    match kind {
        AdapterKind::Ollama => Ok(ollama::health_check().await),
        AdapterKind::OpenAiCompatible => {
            Ok(openai_compatible::OpenAiCompatibleProvider::new(base_url, api_key).health_check().await)
        }
        AdapterKind::Anthropic => {
            Ok(anthropic::AnthropicProvider::new(base_url, api_key).health_check().await)
        }
        AdapterKind::Gemini => Ok(gemini::GeminiProvider::new(base_url, api_key).health_check().await),
        AdapterKind::Unimplemented => Err(format!(
            "{provider_type} does not have a native adapter yet - cannot test connection"
        )),
    }
}

// ═══════════════════════════════════════════════════════════════
// Capability-aware task routing
// ═══════════════════════════════════════════════════════════════

/// A coarse task classification used to pick a model by declared capability
/// rather than by hardcoding provider/model names. This is an honest,
/// documented heuristic (keyword classification + provider-declared
/// capabilities/context length as proxies) - there is no real benchmark data
/// behind "reasoning" or "fast" the way there is for e.g. measured tokens/sec,
/// so it must never be presented as more precise than that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCapability {
    Coding,
    Fast,
    Reasoning,
}

pub fn classify_task(prompt: &str) -> TaskCapability {
    let p = prompt.to_lowercase();
    let fast_hit = ["summar", "tl;dr", "short", "quick", "brief"].iter().any(|k| p.contains(k));
    let reasoning_hit = ["architect", "design", "complex", "reason", "plan", "tradeoff", "trade-off"]
        .iter()
        .any(|k| p.contains(k));
    let coding_hit = ["code", "rust", "python", "typescript", "function", "implement", "bug", "refactor", "compile"]
        .iter()
        .any(|k| p.contains(k));

    if reasoning_hit {
        TaskCapability::Reasoning
    } else if fast_hit && !coding_hit {
        TaskCapability::Fast
    } else if coding_hit {
        TaskCapability::Coding
    } else {
        // Default bias: this is a coding IDE, most unclassified prompts are
        // coding-adjacent work.
        TaskCapability::Coding
    }
}

/// Picks the best (provider, model) pair for a task from the enabled
/// providers' declared capabilities. Scoring is intentionally simple and
/// documented inline - swap in real per-model benchmark data later without
/// changing this function's contract.
pub fn select_provider_and_model_for_task(
    providers: &[ProviderConfig],
    task: TaskCapability,
) -> Option<(ProviderConfig, String)> {
    let mut best: Option<(f64, &ProviderConfig, &str)> = None;

    for provider in providers.iter().filter(|p| p.enabled) {
        for model in &provider.models {
            let score = match task {
                // Coding: prefer providers that advertise coding support,
                // then by context length (proxy for handling larger files).
                TaskCapability::Coding => {
                    if provider.capabilities.coding {
                        1_000_000.0 + provider.capabilities.context_length as f64
                    } else {
                        provider.capabilities.context_length as f64
                    }
                }
                // Fast/cheap: prefer the smallest context length as a proxy
                // for a lighter, faster-responding model.
                TaskCapability::Fast => 10_000_000.0 - provider.capabilities.context_length as f64,
                // Reasoning: prefer the largest context length, with a
                // coding-capability bonus (reasoning-heavy dev tasks still
                // benefit from code-aware models).
                TaskCapability::Reasoning => {
                    provider.capabilities.context_length as f64
                        + if provider.capabilities.coding { 50_000.0 } else { 0.0 }
                }
            };

            if best.as_ref().map(|(s, _, _)| score > *s).unwrap_or(true) {
                best = Some((score, provider, model.as_str()));
            }
        }
    }

    tracing::debug!(
        target: "ai",
        event = "task_capability_selection",
        task = ?task,
        candidates = providers.iter().filter(|p| p.enabled).count(),
        selected = best.as_ref().map(|(_, p, m)| format!("{}/{}", p.name, m)),
    );

    best.map(|(_, provider, model)| (provider.clone(), model.to_string()))
}

// ═══════════════════════════════════════════════════════════════
// FIM (fill-in-middle) capability routing
// ═══════════════════════════════════════════════════════════════

/// Picks a configured, enabled, non-Ollama provider that explicitly
/// declares FIM support (`capabilities.fim = true`), if any. Selection
/// prefers the smallest declared context length as a speed proxy - FIM
/// completions are the most latency-sensitive request type in the app,
/// same reasoning as `TaskCapability::Fast` elsewhere in this module.
/// Returns `None` in the common/default case where no configured provider
/// advertises FIM (`ProviderCapabilities::default()` sets `fim: false`).
pub fn select_fim_provider(providers: &[ProviderConfig]) -> Option<ProviderConfig> {
    let rejected: Vec<&str> = providers
        .iter()
        .filter(|p| p.adapter_kind() != AdapterKind::Ollama && !(p.enabled && p.capabilities.fim))
        .map(|p| p.name.as_str())
        .collect();

    let selected = providers
        .iter()
        .filter(|p| p.enabled && p.adapter_kind() != AdapterKind::Ollama && p.capabilities.fim)
        .min_by_key(|p| p.capabilities.context_length)
        .cloned();

    tracing::debug!(
        target: "ai",
        event = "fim_provider_selection",
        rejected = ?rejected,
        selected = selected.as_ref().map(|p| p.name.as_str()),
    );

    selected
}

/// Capability-gated FIM (fill-in-middle) raw-completion routing - the
/// single entry point ghost text (`ai::completion`) and inline autocomplete
/// (`ai::autocomplete`) use for latency-sensitive completions that cannot
/// be expressed as a chat message list (Ollama's `/api/generate` with
/// `raw: true`, not `/api/chat`).
///
/// Only Ollama has a real, working FIM adapter today
/// (`providers::ollama::generate_raw`). A configured non-Ollama provider is
/// only even considered if it explicitly declares `capabilities.fim = true`
/// via `select_fim_provider`; since no adapter currently implements FIM for
/// any non-Ollama provider_type, selecting one produces a clear, honest
/// error - the same pattern already used for Anthropic/Gemini's
/// `AdapterKind::Unimplemented` chat routing - rather than silently
/// dropping the request or sending it somewhere it can't be served. The
/// common/default case (no provider declares FIM) falls straight through
/// to the real local Ollama path, with model choice via the existing speed
/// heuristic in `ai::router` (never a hardcoded model name).
pub async fn complete_fim(
    providers: &[ProviderConfig],
    health: &HealthRegistry,
    prompt: &str,
    num_predict: u32,
    temperature: f32,
) -> AppResult<String> {
    if let Some(config) = select_fim_provider(providers) {
        let msg = format!(
            "{} advertises FIM support but no FIM adapter exists yet for provider_type '{}'",
            config.name, config.provider_type
        );
        tracing::debug!(target: "ai", event = "fim_rejected_no_adapter", provider = %config.name, provider_type = %config.provider_type);
        return Err(AppError::Provider(msg));
    }
    tracing::debug!(target: "ai", event = "fim_falling_back_to_ollama");

    let models = ollama::list_models().await?;
    let prefs = crate::ai::router::Preferences { goal: "speed".to_string(), cost_preference: "free".to_string() };
    let model = crate::ai::router::score_models(&models, &prefs)
        .into_iter()
        .next()
        .map(|(_, name, _)| name)
        .ok_or_else(|| AppError::Provider("no local Ollama models available".to_string()))?;

    let config = provider_registry::default_ollama_provider();
    let health_key = health_key_for(&config);
    if !health.is_healthy(&health_key) {
        return Err(AppError::Provider(format!(
            "{} is in cooldown after repeated failures - try again shortly",
            config.name
        )));
    }

    let start = Instant::now();
    let result = ollama::generate_raw(&model, prompt, num_predict, temperature).await;
    match &result {
        Ok(_) => health.record_success(&health_key, start.elapsed().as_secs_f64() * 1000.0),
        Err(_) => health.record_failure(&health_key),
    }
    result
}

/// Single-shot, non-streaming chat generation for callers that just need a
/// complete response string (e.g. agent_v2's planner/coder/reviewer nodes),
/// with the model chosen by task capability rather than hardcoded.
///
/// `providers` should be the already-loaded provider list (callers must load
/// it themselves via `provider_registry::load_providers` *before* calling
/// this function if they're holding a `rusqlite::Connection` guard - the
/// guard cannot be held across this function's `.await` points, matching the
/// Send-safety constraint documented on `ai::chat_or_use_cache`).
///
/// Selection order: a configured, enabled, capability-matching non-Ollama
/// provider first; otherwise fall back to local Ollama, picking a real
/// installed model via the existing speed/quality heuristic in `ai::router`
/// (never a hardcoded model name).
pub async fn generate_for_task(
    providers: &[ProviderConfig],
    health: &HealthRegistry,
    task: TaskCapability,
    system_prompt: &str,
    user_prompt: &str,
) -> AppResult<String> {
    let messages = vec![
        ollama::ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
        ollama::ChatMessage { role: "user".to_string(), content: user_prompt.to_string() },
    ];

    let non_ollama: Vec<ProviderConfig> = providers.iter().filter(|p| p.adapter_kind() != AdapterKind::Ollama).cloned().collect();
    if let Some((config, model)) = select_provider_and_model_for_task(&non_ollama, task) {
        return stream_cloud_chat(health, &config, &model, messages, |_token, _done| {}).await;
    }

    // Fall back to local Ollama. Model choice reuses the existing
    // speed/quality scoring heuristic (ai::router::score_models) rather than
    // a fixed model name: Fast tasks bias toward smaller/quicker models,
    // Coding/Reasoning bias toward quality.
    let models = ollama::list_models().await?;
    let goal = if task == TaskCapability::Fast { "speed" } else { "quality" };
    let prefs = crate::ai::router::Preferences { goal: goal.to_string(), cost_preference: "free".to_string() };
    let model = crate::ai::router::score_models(&models, &prefs)
        .into_iter()
        .next()
        .map(|(_, name, _)| name)
        .ok_or_else(|| AppError::Provider("no local Ollama models available".to_string()))?;

    let config = provider_registry::default_ollama_provider();
    let health_key = health_key_for(&config);
    if !health.is_healthy(&health_key) {
        return Err(AppError::Provider(
            "Ollama is in cooldown after repeated failures - try again shortly".to_string(),
        ));
    }

    let start = Instant::now();
    let mut accumulated = String::new();
    let result = ollama::chat_stream(&model, messages, |token, _done| accumulated.push_str(token)).await;
    match &result {
        Ok(_) => health.record_success(&health_key, start.elapsed().as_secs_f64() * 1000.0),
        Err(_) => health.record_failure(&health_key),
    }
    result.map(|_| accumulated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider_registry::ProviderCapabilities;

    fn provider(id: &str, coding: bool, context_length: u64, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            name: id.to_string(),
            provider_type: "openai_compatible".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            models: vec![model.to_string()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities { coding, context_length, ..ProviderCapabilities::default() },
            created_at: 0,
        }
    }

    #[tokio::test]
    async fn test_connection_fails_clearly_for_unimplemented_adapter() {
        // No provider_type string currently maps to Unimplemented (see
        // provider_registry::adapter_kind_for's fallback to
        // OpenAiCompatible for anything unrecognized) - exercise the
        // Unimplemented branch directly via test_connection_for_kind,
        // same workaround provider_registry's own tests use for the same
        // reachability gap.
        let result = test_connection_for_kind(AdapterKind::Unimplemented, "some-future-provider", String::new(), String::new()).await;
        assert!(result.is_err(), "an unimplemented adapter must fail the connection test loudly, not report false silently");
    }

    #[tokio::test]
    async fn test_connection_dispatches_to_the_real_adapter_for_each_implemented_kind() {
        // A closed local port - connection refused immediately, no DNS
        // lookup delay, deterministic Ok(false) rather than a hang.
        let unreachable = "http://127.0.0.1:1".to_string();
        for provider_type in ["openai_compatible", "anthropic", "gemini"] {
            let result = test_connection(provider_type, unreachable.clone(), String::new()).await;
            assert_eq!(
                result,
                Ok(false),
                "{provider_type} must dispatch to its real adapter and report Ok(false) for an unreachable endpoint, not error out"
            );
        }
    }

    #[test]
    fn adapter_kind_routes_known_openai_compatible_services_through_shared_adapter() {
        for provider_type in [
            "openai", "openai_compatible", "openrouter", "deepseek", "groq",
            "together", "fireworks", "deepinfra", "lmstudio", "vllm", "llamacpp", "custom",
        ] {
            assert_eq!(
                provider_registry::adapter_kind_for(provider_type),
                AdapterKind::OpenAiCompatible,
                "{provider_type} should route through the shared OpenAI-compatible adapter"
            );
        }
    }

    #[test]
    fn adapter_kind_keeps_ollama_on_its_own_path() {
        assert_eq!(provider_registry::adapter_kind_for("ollama"), AdapterKind::Ollama);
    }

    #[test]
    fn adapter_kind_routes_anthropic_through_its_own_native_adapter() {
        assert_eq!(provider_registry::adapter_kind_for("anthropic"), AdapterKind::Anthropic);
    }

    #[test]
    fn adapter_kind_routes_gemini_through_its_own_native_adapter() {
        assert_eq!(provider_registry::adapter_kind_for("gemini"), AdapterKind::Gemini);
    }

    #[test]
    fn health_key_preserves_ollama_and_isolates_others() {
        let ollama = provider_registry::default_ollama_provider();
        assert_eq!(health_key_for(&ollama), "ollama");

        let cloud = provider("cloud-1", true, 128_000, "m");
        assert_eq!(health_key_for(&cloud), "provider:cloud-1");
    }

    #[test]
    fn resolve_provider_for_model_falls_back_to_ollama_with_no_db() {
        let resolved = resolve_provider_for_model(None, "qwen2.5-coder:7b");
        assert_eq!(resolved.provider_type, "ollama");
    }

    #[test]
    fn classify_task_coding_prompt() {
        assert_eq!(classify_task("Generate production Rust code for a parser"), TaskCapability::Coding);
    }

    #[test]
    fn classify_task_fast_prompt() {
        assert_eq!(classify_task("Summarize this document briefly"), TaskCapability::Fast);
    }

    #[test]
    fn classify_task_reasoning_prompt() {
        assert_eq!(classify_task("Design a complex distributed system architecture"), TaskCapability::Reasoning);
    }

    #[test]
    fn coding_task_selects_coding_capable_model_over_non_coding() {
        let providers = vec![
            provider("no-code", false, 200_000, "big-general"),
            provider("coder", true, 32_000, "small-coder"),
        ];
        let (chosen, model) = select_provider_and_model_for_task(&providers, TaskCapability::Coding).unwrap();
        assert_eq!(chosen.id, "coder");
        assert_eq!(model, "small-coder");
    }

    #[test]
    fn fast_task_selects_smallest_context_model() {
        let providers = vec![
            provider("big", true, 200_000, "big-model"),
            provider("small", true, 8_000, "small-model"),
        ];
        let (chosen, _) = select_provider_and_model_for_task(&providers, TaskCapability::Fast).unwrap();
        assert_eq!(chosen.id, "small");
    }

    #[test]
    fn reasoning_task_selects_largest_context_model() {
        let providers = vec![
            provider("small", true, 8_000, "small-model"),
            provider("large", true, 1_000_000, "large-model"),
        ];
        let (chosen, _) = select_provider_and_model_for_task(&providers, TaskCapability::Reasoning).unwrap();
        assert_eq!(chosen.id, "large");
    }

    #[test]
    fn select_returns_none_when_no_providers_have_models() {
        let providers = vec![provider_registry::default_ollama_provider()]; // empty .models
        assert!(select_provider_and_model_for_task(&providers, TaskCapability::Coding).is_none());
    }

    /// Proves generate_for_task's Ollama fallback path (no configured cloud
    /// providers, the common/default case - exactly what agent_v2 hits
    /// today) produces a real generation with no hardcoded model name,
    /// against a real running local Ollama instance. This is the direct
    /// replacement for the old intelligence::gateway::OllamaGateway path.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn generate_for_task_falls_back_to_real_ollama_with_no_hardcoded_model() {
        let health = HealthRegistry::default();
        let response = generate_for_task(
            &[], // no configured cloud providers - forces the Ollama fallback
            &health,
            TaskCapability::Coding,
            "You are a helpful assistant.",
            "Reply with exactly the word: hello",
        )
        .await
        .expect("should generate via the local Ollama fallback");

        assert!(!response.trim().is_empty(), "expected a non-empty real generation");
    }

    /// Proves the new fast-path `stream_chat` entry point (what
    /// `inline.rs::stream_inline_edit` now calls) produces a real,
    /// token-streamed generation via the resolved Ollama config, against a
    /// real running local Ollama instance.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn stream_chat_streams_real_tokens_for_resolved_ollama_config() {
        let health = HealthRegistry::default();
        let models = ollama::list_models().await.expect("Ollama must be reachable for this test");
        let model = models.first().expect("expected at least one local model").name.clone();
        let config = resolve_provider_for_model(None, &model);
        assert_eq!(config.provider_type, "ollama");

        let mut streamed = String::new();
        let response = stream_chat(
            &health,
            &config,
            &model,
            vec![ollama::ChatMessage { role: "user".to_string(), content: "Reply with exactly the word: hello".to_string() }],
            |token, _done| streamed.push_str(token),
        )
        .await
        .expect("should stream a real response");

        assert!(!response.trim().is_empty());
        assert_eq!(response, streamed, "accumulated response should match what was streamed token-by-token");
    }

    // ── FIM capability routing ──────────────────────────────────────────

    fn fim_provider(id: &str, fim: bool, context_length: u64) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            name: id.to_string(),
            provider_type: "openai_compatible".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            models: vec!["some-model".to_string()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities { fim, context_length, ..ProviderCapabilities::default() },
            created_at: 0,
        }
    }

    #[test]
    fn select_fim_provider_only_considers_providers_that_advertise_fim() {
        let providers = vec![
            fim_provider("no-fim", false, 32_000),
            fim_provider("has-fim", true, 32_000),
        ];
        let selected = select_fim_provider(&providers).expect("expected the fim-capable provider to be selected");
        assert_eq!(selected.id, "has-fim");
    }

    #[test]
    fn select_fim_provider_returns_none_when_no_provider_advertises_fim() {
        let providers = vec![fim_provider("no-fim-a", false, 8_000), fim_provider("no-fim-b", false, 200_000)];
        assert!(select_fim_provider(&providers).is_none(), "providers without FIM capability must not be selected");
    }

    #[test]
    fn select_fim_provider_prefers_smallest_context_among_fim_capable_providers() {
        let providers = vec![fim_provider("big", true, 200_000), fim_provider("small", true, 8_000)];
        let selected = select_fim_provider(&providers).unwrap();
        assert_eq!(selected.id, "small", "FIM selection should bias toward the fastest (smallest-context) capable provider");
    }

    #[test]
    fn select_fim_provider_ignores_ollama_entries() {
        // The built-in Ollama config now also has fim: true, but real
        // Ollama execution always goes through complete_fim's explicit
        // fallback path, never through select_fim_provider - this proves
        // that separation holds even if an "ollama" entry is present in
        // the loaded provider list (e.g. read back from the settings table).
        let providers = vec![provider_registry::default_ollama_provider()];
        assert!(select_fim_provider(&providers).is_none());
    }

    /// Rule: a provider that advertises FIM support but has no working FIM
    /// adapter must be rejected gracefully (a clear error, no panic, no
    /// silent mis-dispatch) rather than either crashing or falling through
    /// to Ollama unannounced. This never makes a network call, so it needs
    /// no live Ollama instance and is not `#[ignore]`d.
    #[tokio::test]
    async fn complete_fim_rejects_provider_that_advertises_fim_with_no_working_adapter() {
        let health = HealthRegistry::default();
        let providers = vec![fim_provider("cloud-fim", true, 32_000)];

        let result = complete_fim(&providers, &health, "<|fim_prefix|>x<|fim_suffix|>y<|fim_middle|>", 16, 0.1).await;

        let err = result.expect_err("a FIM-capable provider with no adapter must error, not silently succeed");
        let msg = err.to_string();
        assert!(msg.contains("cloud-fim"), "error should name the rejected provider: {msg}");
        assert!(msg.contains("no FIM adapter"), "error should explain why it was rejected: {msg}");
    }

    /// Ollama FIM flow: when no configured provider advertises FIM (the
    /// common/default case), complete_fim must fall through to a real,
    /// working local Ollama completion - proving the standard flow still
    /// functions end to end through the new capability-gated entry point.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn complete_fim_falls_back_to_real_ollama_when_no_provider_advertises_fim() {
        let health = HealthRegistry::default();
        let response = complete_fim(
            &[], // no configured providers at all - forces the Ollama fallback
            &health,
            "<|fim_prefix|>fn add(a: i32, b: i32) -> i32 {\n    <|fim_suffix|>\n}<|fim_middle|>",
            32,
            0.1,
        )
        .await
        .expect("should complete via the real local Ollama FIM path");

        assert!(!response.trim().is_empty(), "expected a non-empty real FIM completion");
    }
}
