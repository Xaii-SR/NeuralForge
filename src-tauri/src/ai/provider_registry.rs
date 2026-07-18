use crate::ai::credential_store;
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which HTTP client a `provider_type` routes through. This is the ONE
/// place a provider's execution mechanics are classified from its
/// persisted `provider_type` string - every other module (routing,
/// capability clamping) consumes this classification rather than
/// re-deriving `provider_type == "ollama"`-style checks of its own.
/// OpenAiCompatible is deliberately the default for everything that isn't
/// Ollama or a protocol that genuinely differs from the OpenAI
/// chat-completions shape (OpenAI, OpenRouter, DeepSeek, Groq, Together,
/// Fireworks, DeepInfra, LM Studio, vLLM, llama.cpp, user-defined custom
/// endpoints). Do not add a new arm here per-company; only add one when a
/// provider's wire format genuinely cannot be expressed as OpenAI-compatible
/// chat completions (Gemini's native API is the remaining known case;
/// Anthropic had this problem too until `providers::anthropic` was added).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    Ollama,
    OpenAiCompatible,
    /// Anthropic's native `/v1/messages` API - see `providers::anthropic`.
    /// Its own variant (not OpenAiCompatible) because the wire format
    /// genuinely differs: `x-api-key`/`anthropic-version` headers instead
    /// of `Authorization: Bearer`, a top-level `system` field instead of a
    /// `system` role in `messages`, and Anthropic-specific SSE event types.
    Anthropic,
    /// Provider type is recognized but has no working adapter yet (Gemini's
    /// native API today). Fails loudly rather than mis-routing through the
    /// OpenAI-compatible client, which would silently produce wrong
    /// requests against that API.
    Unimplemented,
}

pub fn adapter_kind_for(provider_type: &str) -> AdapterKind {
    match provider_type {
        "ollama" => AdapterKind::Ollama,
        "anthropic" => AdapterKind::Anthropic,
        "gemini" => AdapterKind::Unimplemented,
        // openai, openai_compatible, openrouter, deepseek, groq, together,
        // fireworks, deepinfra, lmstudio, vllm, llamacpp, custom, mistral, ...
        _ => AdapterKind::OpenAiCompatible,
    }
}

/// The maximum capabilities a given adapter kind can ever truthfully
/// support, independent of what any individual `ProviderConfig` declares.
/// This is the single enforcement point behind `clamp_capabilities` - an
/// adapter kind can never be made to support more than what's listed here
/// without a real implementation backing it. `Unimplemented` permits
/// nothing: it has no working adapter, so no capability claim about it can
/// ever be true.
pub fn max_capabilities_for(kind: AdapterKind) -> ProviderCapabilities {
    match kind {
        AdapterKind::Ollama => ProviderCapabilities {
            chat: true,
            streaming: true,
            coding: true,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            fim: true,
            context_length: u64::MAX,
        },
        AdapterKind::OpenAiCompatible => ProviderCapabilities {
            chat: true,
            streaming: true,
            coding: true,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            // No adapter implements raw/FIM completion for OpenAI-compatible
            // endpoints yet (see ai::provider_router::complete_fim) - so no
            // provider of this kind may ever declare fim: true, regardless
            // of what's requested.
            fim: false,
            context_length: u64::MAX,
        },
        AdapterKind::Anthropic => ProviderCapabilities {
            chat: true,
            streaming: true,
            coding: true,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            // providers::anthropic has no raw/FIM completion adapter -
            // Anthropic's API has no equivalent endpoint to claim this for.
            fim: false,
            context_length: u64::MAX,
        },
        AdapterKind::Unimplemented => ProviderCapabilities {
            chat: false,
            streaming: false,
            coding: false,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            fim: false,
            context_length: 0,
        },
    }
}

/// Clamps `requested` capabilities to what `provider_type`'s adapter can
/// actually execute. This is the single enforcement point every
/// `ProviderConfig` construction path must call - a capability flag can
/// only ever come out `true` if both the caller requested it AND the
/// resolved adapter kind permits it. Silent sanitization, not a hard
/// error: a request for an unsupported capability degrades to "not
/// granted" rather than failing config creation outright.
pub fn clamp_capabilities(provider_type: &str, requested: ProviderCapabilities) -> ProviderCapabilities {
    let max = max_capabilities_for(adapter_kind_for(provider_type));
    ProviderCapabilities {
        chat: requested.chat && max.chat,
        streaming: requested.streaming && max.streaming,
        coding: requested.coding && max.coding,
        vision: requested.vision && max.vision,
        tool_calling: requested.tool_calling && max.tool_calling,
        function_calling: requested.function_calling && max.function_calling,
        embeddings: requested.embeddings && max.embeddings,
        fim: requested.fim && max.fim,
        context_length: requested.context_length.min(max.context_length),
    }
}

/// Persistent provider configuration stored in SQLite.
///
/// SECURITY NOTE (remediated): `api_key` here is the in-memory/IPC-facing
/// value only. The `settings` table row for `SETTINGS_KEY_PROVIDERS` never
/// contains a real key - `save_providers_raw` blanks `api_key` on every
/// provider before serializing, and `load_providers_raw` fills it back in
/// from the OS credential store (`ai::credential_store`, backed by the
/// `keyring` crate: Windows Credential Manager / macOS Keychain / Linux
/// libsecret) keyed by provider `id`. `add_provider_config`/
/// `update_provider_config` write the real key to the keychain before it
/// ever reaches `save_providers_raw`; `delete_provider_config` removes the
/// keychain entry alongside the config row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub capabilities: ProviderCapabilities,
    pub created_at: i64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        let provider_type = "openai_compatible".to_string();
        Self {
            id: Uuid::new_v4().to_string(),
            name: "New Provider".to_string(),
            capabilities: clamp_capabilities(&provider_type, ProviderCapabilities::default()),
            provider_type,
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            models: Vec::new(),
            enabled: true,
            is_default: false,
            created_at: epoch_secs(),
        }
    }
}

impl ProviderConfig {
    /// Classifies this config's execution mechanics from its persisted
    /// `provider_type`. The single point every routing decision should use
    /// instead of re-deriving `provider_type == "ollama"`-style checks.
    pub fn adapter_kind(&self) -> AdapterKind {
        adapter_kind_for(&self.provider_type)
    }
}

/// Model capabilities advertised by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub chat: bool,
    pub streaming: bool,
    pub coding: bool,
    pub vision: bool,
    pub tool_calling: bool,
    pub function_calling: bool,
    pub embeddings: bool,
    /// Raw/fill-in-middle completion support (Ollama's `/api/generate`
    /// with `raw: true`, or an equivalent legacy completion endpoint) -
    /// distinct from `chat`/`streaming`, which describe `/v1/chat/completions`-
    /// shaped requests. Defaults to false: only providers with a real,
    /// working FIM adapter should ever declare this. See
    /// `ai::provider_router::complete_fim`.
    pub fim: bool,
    pub context_length: u64,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            chat: true,
            streaming: true,
            coding: true,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            fim: false,
            context_length: 128000,
        }
    }
}

/// Active model configuration per-task type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
}

const SETTINGS_KEY_PROVIDERS: &str = "provider_configs";
const SETTINGS_KEY_ACTIVE_CHAT: &str = "active_model_chat";
const SETTINGS_KEY_ACTIVE_AGENT: &str = "active_model_agent";
const SETTINGS_KEY_ACTIVE_INLINE: &str = "active_model_inline";
const SETTINGS_KEY_ACTIVE_GHOST: &str = "active_model_ghost";

fn epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ═══════════════════════════════════════════════════════════════
// Persistence
// ═══════════════════════════════════════════════════════════════

/// The built-in local provider. Always present even with an empty/missing
/// `settings` row, and always present in `load_providers_raw`'s output (see
/// below) so model resolution always has a safe local fallback.
pub const DEFAULT_OLLAMA_ID: &str = "default-ollama";

pub fn default_ollama_provider() -> ProviderConfig {
    let provider_type = "ollama".to_string();
    ProviderConfig {
        id: DEFAULT_OLLAMA_ID.to_string(),
        name: "Ollama (Local)".to_string(),
        // Ollama is the only provider with a real, working FIM adapter
        // today (providers::ollama::generate_raw) - see
        // ai::provider_router::complete_fim. Routed through clamp_capabilities
        // like every other construction path, even though Ollama's max
        // capabilities already happen to permit fim: true - this keeps
        // there being exactly one place capability truth is decided.
        capabilities: clamp_capabilities(&provider_type, ProviderCapabilities { fim: true, ..ProviderCapabilities::default() }),
        provider_type,
        base_url: "http://localhost:11434".to_string(),
        api_key: String::new(),
        models: Vec::new(),
        enabled: true,
        is_default: true,
        created_at: 0,
    }
}

fn load_providers_raw(conn: &Connection) -> Vec<ProviderConfig> {
    let mut providers = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![SETTINGS_KEY_PROVIDERS],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|json| serde_json::from_str::<Vec<ProviderConfig>>(&json).ok())
        .unwrap_or_else(|| vec![default_ollama_provider()]);

    for provider in &mut providers {
        provider.api_key = credential_store::load_api_key(&provider.id);
    }
    providers
}

/// Public read accessor for other `ai::` modules (routing, capability-based
/// model selection) - `load_providers_raw` stays private so persistence
/// details (the `settings` table encoding) aren't leaked beyond this file.
pub fn load_providers(conn: &Connection) -> Vec<ProviderConfig> {
    load_providers_raw(conn)
}

fn save_providers_raw(conn: &Connection, providers: &[ProviderConfig]) -> AppResult<()> {
    // Never persist a real api_key to disk - it lives only in the OS
    // keychain (see this struct's SECURITY NOTE doc comment above).
    let redacted: Vec<ProviderConfig> = providers
        .iter()
        .cloned()
        .map(|mut p| {
            p.api_key = String::new();
            p
        })
        .collect();
    let json = serde_json::to_string(&redacted)
        .map_err(|e| AppError::Provider(format!("serialize providers: {e}")))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SETTINGS_KEY_PROVIDERS, json],
    )
    .map_err(|e| AppError::Provider(format!("save providers: {e}")))?;
    Ok(())
}

fn load_model_config(conn: &Connection, key: &str) -> Option<ModelConfig> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|json| serde_json::from_str::<ModelConfig>(&json).ok())
}

fn save_model_config(conn: &Connection, key: &str, config: &ModelConfig) -> AppResult<()> {
    let json = serde_json::to_string(config)
        .map_err(|e| AppError::Provider(format!("serialize model config: {e}")))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, json],
    )
    .map_err(|e| AppError::Provider(format!("save model config: {e}")))?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// Tauri Commands
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn list_provider_configs(db: tauri::State<'_, crate::database::DbState>) -> Result<Vec<ProviderConfig>, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    Ok(load_providers_raw(conn))
}

/// Builds a new `ProviderConfig` with capabilities clamped to what
/// `provider_type`'s adapter can actually execute - extracted from
/// `add_provider_config` so it's directly unit-testable without a Tauri
/// runtime/State. This is the "creating a config with an unsupported
/// capability" enforcement point: a caller cannot end up with, say,
/// `chat: true` on a `provider_type` that resolves to `AdapterKind::Unimplemented`.
fn build_provider_config(name: String, provider_type: String, base_url: String, api_key: String, is_default: bool) -> ProviderConfig {
    ProviderConfig {
        id: Uuid::new_v4().to_string(),
        name,
        capabilities: clamp_capabilities(&provider_type, ProviderCapabilities::default()),
        provider_type,
        base_url,
        api_key,
        models: Vec::new(),
        enabled: true,
        is_default,
        created_at: epoch_secs(),
    }
}

#[tauri::command]
pub fn add_provider_config(
    db: tauri::State<'_, crate::database::DbState>,
    name: String,
    provider_type: String,
    base_url: String,
    api_key: String,
) -> Result<ProviderConfig, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_providers_raw(conn);

    let new = build_provider_config(name, provider_type, base_url, api_key.clone(), providers.is_empty());
    credential_store::store_api_key(&new.id, &api_key)?;

    providers.push(new.clone());
    save_providers_raw(conn, &providers).map_err(|e| e.to_string())?;
    Ok(new)
}

#[tauri::command]
pub fn update_provider_config(
    db: tauri::State<'_, crate::database::DbState>,
    id: String,
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    models: Option<Vec<String>>,
) -> Result<ProviderConfig, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_providers_raw(conn);

    let provider = providers.iter_mut().find(|p| p.id == id).ok_or("provider not found")?;
    if let Some(n) = name { provider.name = n; }
    if let Some(u) = base_url { provider.base_url = u; }
    if let Some(k) = api_key {
        credential_store::store_api_key(&provider.id, &k)?;
        provider.api_key = k;
    }
    if let Some(e) = enabled { provider.enabled = e; }
    if let Some(m) = models { provider.models = m; }

    let result = provider.clone();
    save_providers_raw(conn, &providers).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command]
pub fn delete_provider_config(
    db: tauri::State<'_, crate::database::DbState>,
    id: String,
) -> Result<(), String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_providers_raw(conn);
    if providers.len() <= 1 {
        return Err("cannot delete the last provider".to_string());
    }
    providers.retain(|p| p.id != id);
    credential_store::delete_api_key(&id);
    save_providers_raw(conn, &providers).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_default_model(
    db: tauri::State<'_, crate::database::DbState>,
    key: String,
    provider_id: String,
    provider_name: String,
    model: String,
) -> Result<(), String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let config = ModelConfig { provider_id, provider_name, model };
    save_model_config(conn, &key, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_model_config(
    db: tauri::State<'_, crate::database::DbState>,
    key: String,
) -> Result<Option<ModelConfig>, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    Ok(load_model_config(conn, &key))
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db() -> Connection {
        let mut d = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        d.push(format!("nf_prov_test_{nanos}"));
        fs::create_dir_all(&d).unwrap();
        crate::database::open_for_workspace(&d).unwrap()
    }

    #[test]
    fn default_ollama_provider_exists() {
        let conn = temp_db();
        let providers = load_providers_raw(&conn);
        assert!(providers.iter().any(|p| p.provider_type == "ollama"));
    }

    #[test]
    fn add_and_list_providers() {
        let conn = temp_db();
        let mut providers = load_providers_raw(&conn);

        let new = ProviderConfig {
            id: "custom-1".into(),
            name: "My Custom".into(),
            provider_type: "openai_compatible".into(),
            base_url: "http://localhost:1234/v1".into(),
            api_key: "test".into(),
            models: vec!["model-a".into()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities::default(),
            created_at: 0,
        };

        providers.push(new);
        save_providers_raw(&conn, &providers).unwrap();

        let reloaded = load_providers_raw(&conn);
        assert!(reloaded.iter().any(|p| p.name == "My Custom"));
    }

    #[test]
    fn model_config_roundtrip() {
        let conn = temp_db();
        let config = ModelConfig {
            provider_id: "p1".into(),
            provider_name: "Ollama".into(),
            model: "deepseek-coder".into(),
        };
        save_model_config(&conn, SETTINGS_KEY_ACTIVE_CHAT, &config).unwrap();
        let loaded = load_model_config(&conn, SETTINGS_KEY_ACTIVE_CHAT).unwrap();
        assert_eq!(loaded.model, "deepseek-coder");
    }

    // ── Capability clamping (Phase 5 hardening) ─────────────────────────

    fn all_true_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            chat: true,
            streaming: true,
            coding: true,
            vision: true,
            tool_calling: true,
            function_calling: true,
            embeddings: true,
            fim: true,
            context_length: 1_000_000,
        }
    }

    /// Test 1 (Mismatch): a config requesting a capability its adapter
    /// doesn't support must be auto-corrected (sanitized to false), not
    /// stored as requested. OpenAI-compatible has no working FIM adapter,
    /// so `fim: true` on an openai_compatible provider must not survive
    /// clamping even though every other requested capability is real.
    #[test]
    fn clamp_capabilities_sanitizes_unsupported_capability_for_openai_compatible() {
        let clamped = clamp_capabilities("openai_compatible", all_true_capabilities());
        assert!(clamped.chat, "chat is genuinely supported and must pass through");
        assert!(clamped.streaming, "streaming is genuinely supported and must pass through");
        assert!(clamped.coding, "coding is genuinely supported and must pass through");
        assert!(!clamped.fim, "fim must be sanitized to false - no OpenAI-compatible FIM adapter exists");
    }

    /// Test 1 (Mismatch), end-to-end through the real construction path:
    /// `build_provider_config` (what `add_provider_config` calls) must
    /// never produce a mismatched config, even if a future caller starts
    /// passing attacker-controlled or just-wrong capability data in.
    #[test]
    fn build_provider_config_never_produces_a_capability_adapter_mismatch() {
        let config = build_provider_config(
            "Custom".into(),
            "openai_compatible".into(),
            "http://localhost:1234/v1".into(),
            String::new(),
            false,
        );
        assert!(!config.capabilities.fim, "a freshly built openai_compatible config must not claim fim support");
    }

    /// Test 2 (Adapter Enforcement): `AdapterKind::Unimplemented` (Gemini
    /// today) must reject every single capability, regardless of what was
    /// requested - this is the exact bug the Phase 5 audit found: a config
    /// with `provider_type: "gemini"` previously kept `chat: true`/
    /// `streaming: true`/`coding: true` from `ProviderCapabilities::default()`
    /// despite having no working adapter. Anthropic used to be in this list
    /// too, until `providers::anthropic` gave it a real adapter - see
    /// `anthropic_adapter_gets_real_capabilities_not_zeroed_out` below.
    #[test]
    fn unimplemented_adapter_rejects_every_requested_capability() {
        for provider_type in ["gemini"] {
            let clamped = clamp_capabilities(provider_type, all_true_capabilities());
            assert!(!clamped.chat, "{provider_type} must not claim chat support");
            assert!(!clamped.streaming, "{provider_type} must not claim streaming support");
            assert!(!clamped.coding, "{provider_type} must not claim coding support");
            assert!(!clamped.vision, "{provider_type} must not claim vision support");
            assert!(!clamped.tool_calling, "{provider_type} must not claim tool_calling support");
            assert!(!clamped.function_calling, "{provider_type} must not claim function_calling support");
            assert!(!clamped.embeddings, "{provider_type} must not claim embeddings support");
            assert!(!clamped.fim, "{provider_type} must not claim fim support");
            assert_eq!(clamped.context_length, 0, "{provider_type} must not claim any usable context length");
        }
    }

    /// Test 2, via the real construction path: creating a provider with
    /// `provider_type: "gemini"` through `build_provider_config` must come
    /// out with every capability false, not the old
    /// `ProviderCapabilities::default()` (chat/streaming/coding: true).
    #[test]
    fn build_provider_config_gives_unimplemented_adapter_zero_capabilities() {
        let config = build_provider_config("Gemini".into(), "gemini".into(), "https://generativelanguage.googleapis.com".into(), "test".into(), false);
        assert!(!config.capabilities.chat);
        assert!(!config.capabilities.streaming);
        assert!(!config.capabilities.coding);
    }

    /// Anthropic now has a real adapter (`providers::anthropic`) - a config
    /// built with `provider_type: "anthropic"` must get real capabilities,
    /// not the zeroed-out `Unimplemented` treatment Gemini still gets.
    #[test]
    fn anthropic_adapter_gets_real_capabilities_not_zeroed_out() {
        let config = build_provider_config("Claude".into(), "anthropic".into(), "https://api.anthropic.com".into(), "sk-test".into(), false);
        assert!(config.capabilities.chat);
        assert!(config.capabilities.streaming);
        assert!(config.capabilities.coding);
        assert!(!config.capabilities.fim, "Anthropic has no raw/FIM completion adapter");
        assert_eq!(config.adapter_kind(), AdapterKind::Anthropic);
    }

    /// Test 3 (Ollama Regression): the default Ollama provider's existing
    /// capabilities (chat/streaming/coding: true, fim: true - set
    /// explicitly because Ollama really does have a working FIM adapter)
    /// must survive clamping unchanged. This is the regression guard that
    /// the hardening pass didn't break the one fully-functional path.
    #[test]
    fn default_ollama_provider_capabilities_unaffected_by_clamping() {
        let ollama = default_ollama_provider();
        assert!(ollama.capabilities.chat);
        assert!(ollama.capabilities.streaming);
        assert!(ollama.capabilities.coding);
        assert!(ollama.capabilities.fim, "Ollama genuinely has a working FIM adapter and must keep declaring it");
        assert_eq!(ollama.adapter_kind(), AdapterKind::Ollama);
    }

    #[test]
    fn adapter_kind_classification_matches_expected_routing() {
        assert_eq!(adapter_kind_for("ollama"), AdapterKind::Ollama);
        assert_eq!(adapter_kind_for("openai_compatible"), AdapterKind::OpenAiCompatible);
        assert_eq!(adapter_kind_for("openai"), AdapterKind::OpenAiCompatible);
        assert_eq!(adapter_kind_for("anthropic"), AdapterKind::Anthropic);
        assert_eq!(adapter_kind_for("gemini"), AdapterKind::Unimplemented);
    }

    #[test]
    fn provider_config_adapter_kind_method_matches_free_function() {
        let ollama = default_ollama_provider();
        assert_eq!(ollama.adapter_kind(), adapter_kind_for(&ollama.provider_type));
    }
}