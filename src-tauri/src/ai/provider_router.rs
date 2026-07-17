//! Unified provider routing layer.
//!
//! This is the ONLY place that decides which HTTP adapter (Ollama vs. the
//! generic OpenAI-compatible client vs. a not-yet-implemented native client)
//! handles a given chat request. Every AI feature - chat, inline edit, future
//! agent/council work - must resolve a provider through this module rather
//! than constructing its own client. See ai/provider_registry.rs for the
//! persisted provider configs this module reads.

use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry::{self, ProviderConfig};
use crate::ai::providers::{ollama, openai_compatible};
use crate::core::errors::{AppError, AppResult};
use rusqlite::Connection;
use std::time::Instant;

/// Which HTTP client a provider_type routes through. OpenAiCompatible is
/// deliberately the default for everything that isn't Ollama or a protocol
/// that genuinely differs from the OpenAI chat-completions shape - see the
/// module doc on `openai_compatible` for the full list of services this
/// covers (OpenAI, OpenRouter, DeepSeek, Groq, Together, Fireworks,
/// DeepInfra, LM Studio, vLLM, llama.cpp, user-defined custom endpoints).
/// Do not add a new arm here per-company; only add one when a provider's
/// wire format genuinely cannot be expressed as OpenAI-compatible chat
/// completions (Anthropic and Gemini's native APIs are the known cases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    Ollama,
    OpenAiCompatible,
    /// Provider type is recognized but has no working adapter yet
    /// (Anthropic/Gemini native APIs). Fails loudly rather than
    /// mis-routing through the OpenAI-compatible client, which would
    /// silently produce wrong requests against those APIs.
    Unimplemented,
}

pub fn adapter_kind_for(provider_type: &str) -> AdapterKind {
    match provider_type {
        "ollama" => AdapterKind::Ollama,
        "anthropic" | "gemini" => AdapterKind::Unimplemented,
        // openai, openai_compatible, openrouter, deepseek, groq, together,
        // fireworks, deepinfra, lmstudio, vllm, llamacpp, custom, mistral, ...
        _ => AdapterKind::OpenAiCompatible,
    }
}

/// Health/cooldown key for a provider. Ollama keeps its historical "ollama"
/// key unchanged (existing UI and tests read this key directly); every other
/// provider gets its own independent cooldown tracked by config id, so one
/// degraded cloud provider never affects Ollama's or another provider's
/// health state.
pub fn health_key_for(config: &ProviderConfig) -> String {
    if config.provider_type == "ollama" {
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
    providers
        .into_iter()
        .find(|p| p.enabled && p.models.iter().any(|m| m == model))
        .unwrap_or_else(provider_registry::default_ollama_provider)
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

    let kind = adapter_kind_for(&config.provider_type);
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

    best.map(|(_, provider, model)| (provider.clone(), model.to_string()))
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

    #[test]
    fn adapter_kind_routes_known_openai_compatible_services_through_shared_adapter() {
        for provider_type in [
            "openai", "openai_compatible", "openrouter", "deepseek", "groq",
            "together", "fireworks", "deepinfra", "lmstudio", "vllm", "llamacpp", "custom",
        ] {
            assert_eq!(
                adapter_kind_for(provider_type),
                AdapterKind::OpenAiCompatible,
                "{provider_type} should route through the shared OpenAI-compatible adapter"
            );
        }
    }

    #[test]
    fn adapter_kind_keeps_ollama_on_its_own_path() {
        assert_eq!(adapter_kind_for("ollama"), AdapterKind::Ollama);
    }

    #[test]
    fn adapter_kind_marks_native_only_providers_unimplemented() {
        assert_eq!(adapter_kind_for("anthropic"), AdapterKind::Unimplemented);
        assert_eq!(adapter_kind_for("gemini"), AdapterKind::Unimplemented);
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
}
