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
/// chat completions. `Unimplemented` is currently empty - kept as a variant
/// (rather than removed) because it's the documented fail-loudly landing
/// spot for the next native-API provider that needs one, same as Anthropic
/// and Gemini both used it in turn before getting real adapters.
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
    /// Google's native Gemini API - see `providers::gemini`. Its own
    /// variant because the wire format differs from both OpenAiCompatible
    /// and Anthropic: the API key is a `?key=` query parameter, the
    /// endpoint path embeds `models/{model}:streamGenerateContent`, and the
    /// body uses `contents: [{role, parts: [{text}]}]` with role names
    /// "user"/"model" rather than "user"/"assistant".
    Gemini,
    /// Provider type is recognized but has no working adapter yet. Fails
    /// loudly rather than mis-routing through the OpenAI-compatible client,
    /// which would silently produce wrong requests against a genuinely
    /// different native API.
    Unimplemented,
}

pub fn adapter_kind_for(provider_type: &str) -> AdapterKind {
    match provider_type {
        "ollama" => AdapterKind::Ollama,
        "anthropic" => AdapterKind::Anthropic,
        "gemini" => AdapterKind::Gemini,
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
        AdapterKind::Gemini => ProviderCapabilities {
            chat: true,
            streaming: true,
            coding: true,
            vision: false,
            tool_calling: false,
            function_calling: false,
            embeddings: false,
            // providers::gemini has no raw/FIM completion adapter.
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
pub fn clamp_capabilities(
    provider_type: &str,
    requested: ProviderCapabilities,
) -> ProviderCapabilities {
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
/// SECURITY NOTE: `api_key` is internal routing state only. Tauri commands
/// return `ProviderConfigView`, which exposes `has_api_key` but never the
/// secret. New writes are verified in the OS credential store before the
/// redacted settings row is committed. Legacy plaintext rows are scrubbed
/// only after a verified keyring copy exists, so migration failure cannot
/// erase the only credential copy.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigView {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub has_api_key: bool,
    pub models: Vec<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub capabilities: ProviderCapabilities,
    pub created_at: i64,
}

impl From<&ProviderConfig> for ProviderConfigView {
    fn from(provider: &ProviderConfig) -> Self {
        Self {
            id: provider.id.clone(),
            name: provider.name.clone(),
            provider_type: provider.provider_type.clone(),
            base_url: provider.base_url.clone(),
            has_api_key: !provider.api_key.is_empty(),
            models: provider.models.clone(),
            enabled: provider.enabled,
            is_default: provider.is_default,
            capabilities: provider.capabilities.clone(),
            created_at: provider.created_at,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMode {
    Chat,
    Agent,
    Inline,
    Ghost,
}

impl AiMode {
    pub fn settings_key(self) -> &'static str {
        match self {
            Self::Chat => SETTINGS_KEY_ACTIVE_CHAT,
            Self::Agent => SETTINGS_KEY_ACTIVE_AGENT,
            Self::Inline => SETTINGS_KEY_ACTIVE_INLINE,
            Self::Ghost => SETTINGS_KEY_ACTIVE_GHOST,
        }
    }

    pub fn from_settings_key(key: &str) -> Option<Self> {
        match key {
            SETTINGS_KEY_ACTIVE_CHAT => Some(Self::Chat),
            SETTINGS_KEY_ACTIVE_AGENT => Some(Self::Agent),
            SETTINGS_KEY_ACTIVE_INLINE => Some(Self::Inline),
            SETTINGS_KEY_ACTIVE_GHOST => Some(Self::Ghost),
            _ => None,
        }
    }

    fn is_supported_by(self, provider: &ProviderConfig) -> bool {
        match self {
            Self::Chat => provider.capabilities.chat && provider.capabilities.streaming,
            Self::Agent | Self::Inline => {
                provider.capabilities.chat
                    && provider.capabilities.streaming
                    && provider.capabilities.coding
            }
            Self::Ghost => {
                provider.adapter_kind() == AdapterKind::Ollama && provider.capabilities.fim
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedModeModel {
    pub provider: ProviderConfig,
    pub model: String,
}

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
        capabilities: clamp_capabilities(
            &provider_type,
            ProviderCapabilities {
                fim: true,
                ..ProviderCapabilities::default()
            },
        ),
        provider_type,
        base_url: "http://localhost:11434".to_string(),
        api_key: String::new(),
        models: Vec::new(),
        enabled: true,
        is_default: true,
        created_at: 0,
    }
}

fn load_persisted_providers(conn: &Connection) -> Vec<ProviderConfig> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![SETTINGS_KEY_PROVIDERS],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|json| serde_json::from_str::<Vec<ProviderConfig>>(&json).ok())
    .unwrap_or_else(|| vec![default_ollama_provider()])
}

fn persist_provider_configs(conn: &Connection, providers: &[ProviderConfig]) -> AppResult<()> {
    let json = serde_json::to_string(providers)
        .map_err(|error| AppError::Provider(format!("serialize providers: {error}")))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SETTINGS_KEY_PROVIDERS, json],
    )
    .map_err(|error| AppError::Provider(format!("save providers: {error}")))?;
    Ok(())
}

fn load_providers_with_backend(
    conn: &Connection,
    backend: &impl credential_store::CredentialBackend,
) -> Vec<ProviderConfig> {
    if let Err(error) = crate::database::retry_pending_secure_scrub(conn) {
        tracing::warn!(
            target: "provider",
            event = "pending_sensitive_storage_scrub_deferred",
            error = %error
        );
    }
    let mut persisted = load_persisted_providers(conn);
    let mut hydrated = persisted.clone();
    let mut scrubbed_legacy_key = false;

    for (stored, runtime) in persisted.iter_mut().zip(hydrated.iter_mut()) {
        let legacy_key = stored.api_key.clone();
        let keyring_key = backend.load(&stored.id);
        match keyring_key {
            Ok(Some(api_key)) if legacy_key.is_empty() || api_key == legacy_key => {
                runtime.api_key = api_key;
                if !legacy_key.is_empty() {
                    stored.api_key.clear();
                    scrubbed_legacy_key = true;
                }
            }
            Ok(Some(_)) => {
                runtime.api_key = legacy_key;
            }
            Ok(None) if !legacy_key.is_empty() => {
                let verified = backend
                    .store(&stored.id, &legacy_key)
                    .and_then(|_| backend.load(&stored.id))
                    .map(|value| value.as_deref() == Some(legacy_key.as_str()))
                    .unwrap_or(false);
                if verified {
                    runtime.api_key = legacy_key;
                    stored.api_key.clear();
                    scrubbed_legacy_key = true;
                } else {
                    runtime.api_key = legacy_key;
                }
            }
            Ok(None) => runtime.api_key.clear(),
            Err(_) => runtime.api_key = legacy_key,
        }
    }

    if scrubbed_legacy_key {
        if let Err(error) = crate::database::mark_secure_scrub_pending(conn)
            .and_then(|_| persist_provider_configs(conn, &persisted))
            .and_then(|_| crate::database::secure_scrub_storage(conn))
        {
            tracing::warn!(
                target: "provider",
                event = "legacy_credential_scrub_deferred",
                error = %error
            );
        }
    }
    hydrated
}

fn load_providers_raw(conn: &Connection) -> Vec<ProviderConfig> {
    load_providers_with_backend(conn, &credential_store::KeyringCredentialBackend)
}

/// Public read accessor for other `ai::` modules (routing, capability-based
/// model selection) - `load_providers_raw` stays private so persistence
/// details (the `settings` table encoding) aren't leaked beyond this file.
pub fn load_providers(conn: &Connection) -> Vec<ProviderConfig> {
    load_providers_raw(conn)
}

pub fn load_provider_by_id(conn: &Connection, provider_id: &str) -> Result<ProviderConfig, String> {
    load_providers_raw(conn)
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| "provider not found".to_string())
}

fn save_providers_with_backend(
    conn: &Connection,
    providers: &[ProviderConfig],
    backend: &impl credential_store::CredentialBackend,
) -> AppResult<()> {
    for provider in providers {
        if provider.api_key.is_empty() {
            continue;
        }
        backend
            .store(&provider.id, &provider.api_key)
            .map_err(|error| AppError::Provider(format!("store provider credential: {error}")))?;
        let verified = backend
            .load(&provider.id)
            .map_err(|error| AppError::Provider(format!("verify provider credential: {error}")))?;
        if verified.as_deref() != Some(provider.api_key.as_str()) {
            return Err(AppError::Provider(
                "credential verification failed; provider settings were not changed".to_string(),
            ));
        }
    }

    let redacted: Vec<ProviderConfig> = providers
        .iter()
        .cloned()
        .map(|mut p| {
            p.api_key = String::new();
            p
        })
        .collect();
    persist_provider_configs(conn, &redacted)
}

fn save_providers_raw(conn: &Connection, providers: &[ProviderConfig]) -> AppResult<()> {
    save_providers_with_backend(conn, providers, &credential_store::KeyringCredentialBackend)
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
pub fn list_provider_configs(
    db: tauri::State<'_, crate::database::DbState>,
) -> Result<Vec<ProviderConfigView>, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    Ok(load_providers_raw(conn)
        .iter()
        .map(ProviderConfigView::from)
        .collect())
}

/// Builds a new `ProviderConfig` with capabilities clamped to what
/// `provider_type`'s adapter can actually execute - extracted from
/// `add_provider_config` so it's directly unit-testable without a Tauri
/// runtime/State. This is the "creating a config with an unsupported
/// capability" enforcement point: a caller cannot end up with, say,
/// `chat: true` on a `provider_type` that resolves to `AdapterKind::Unimplemented`.
fn build_provider_config(
    name: String,
    provider_type: String,
    base_url: String,
    api_key: String,
    is_default: bool,
) -> ProviderConfig {
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
) -> Result<ProviderConfigView, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_providers_raw(conn);

    let new = build_provider_config(
        name,
        provider_type,
        base_url,
        api_key.clone(),
        providers.is_empty(),
    );
    providers.push(new.clone());
    if let Err(error) = save_providers_raw(conn, &providers) {
        credential_store::delete_api_key(&new.id);
        return Err(error.to_string());
    }
    Ok(ProviderConfigView::from(&new))
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
) -> Result<ProviderConfigView, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_providers_raw(conn);

    if base_url.is_some() && api_key.as_deref().is_some_and(|key| !key.is_empty()) {
        return Err(
            "update the provider endpoint and credential in separate operations".to_string(),
        );
    }
    let provider = providers
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("provider not found")?;
    if let Some(n) = name {
        provider.name = n;
    }
    if let Some(u) = base_url {
        provider.base_url = u;
    }
    if let Some(k) = api_key {
        if !k.is_empty() {
            provider.api_key = k;
        }
    }
    if let Some(e) = enabled {
        provider.enabled = e;
    }
    if let Some(m) = models {
        provider.models = m;
    }

    let result = provider.clone();
    save_providers_raw(conn, &providers).map_err(|error| error.to_string())?;
    Ok(ProviderConfigView::from(&result))
}

pub fn resolve_mode_model(conn: &Connection, mode: AiMode) -> Result<ResolvedModeModel, String> {
    let providers = load_providers_raw(conn);
    let assignment = load_model_config(conn, mode.settings_key());

    if let Some(assignment) = assignment {
        let provider = providers
            .into_iter()
            .find(|provider| provider.id == assignment.provider_id)
            .ok_or_else(|| format!("the configured {:?} provider no longer exists", mode))?;
        if !provider.enabled {
            return Err(format!("the configured {:?} provider is disabled", mode));
        }
        if !mode.is_supported_by(&provider) {
            return Err(format!(
                "{} does not support the capabilities required by {:?}",
                provider.name, mode
            ));
        }
        if !provider
            .models
            .iter()
            .any(|model| model == &assignment.model)
        {
            return Err(format!(
                "{} is not configured for provider {}",
                assignment.model, provider.name
            ));
        }
        return Ok(ResolvedModeModel {
            provider,
            model: assignment.model,
        });
    }

    let provider = providers
        .into_iter()
        .filter(|provider| provider.enabled && mode.is_supported_by(provider))
        .find(|provider| provider.is_default && !provider.models.is_empty())
        .or_else(|| {
            load_providers_raw(conn).into_iter().find(|provider| {
                provider.enabled && mode.is_supported_by(provider) && !provider.models.is_empty()
            })
        })
        .ok_or_else(|| format!("no configured provider supports {:?}", mode))?;
    let model = provider.models[0].clone();
    Ok(ResolvedModeModel { provider, model })
}

pub fn resolve_provider_model(
    conn: &Connection,
    provider_id: &str,
    model: &str,
    mode: AiMode,
) -> Result<ResolvedModeModel, String> {
    let provider = load_providers_raw(conn)
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| "provider not found".to_string())?;
    if !provider.enabled {
        return Err("provider is disabled".to_string());
    }
    if !mode.is_supported_by(&provider) {
        return Err(format!(
            "{} does not support the capabilities required by {:?}",
            provider.name, mode
        ));
    }
    if provider.adapter_kind() != AdapterKind::Ollama
        && !provider.models.iter().any(|candidate| candidate == model)
    {
        return Err(format!("{model} is not configured for {}", provider.name));
    }
    Ok(ResolvedModeModel {
        provider,
        model: model.to_string(),
    })
}

#[tauri::command]
pub fn migrate_legacy_api_key(
    db: tauri::State<'_, crate::database::DbState>,
    provider_hint: String,
    api_key: String,
) -> Result<(), String> {
    if api_key.is_empty() {
        return Ok(());
    }
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mut providers = load_persisted_providers(conn);
    let mut matches = providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| {
            provider.id == provider_hint
                || provider.provider_type == provider_hint
                || (provider_hint == "openai" && provider.provider_type == "openai_compatible")
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        let cloud_providers = providers
            .iter()
            .enumerate()
            .filter(|(_, provider)| provider.adapter_kind() != AdapterKind::Ollama)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if cloud_providers.len() == 1 {
            matches = cloud_providers;
        }
    }
    if matches.len() != 1 {
        return Err("legacy credential could not be mapped to exactly one provider".to_string());
    }
    let provider = &mut providers[matches[0]];
    let backend = credential_store::KeyringCredentialBackend;
    let existing = credential_store::CredentialBackend::load(&backend, &provider.id)?;
    if let Some(existing) = existing {
        if existing != api_key {
            return Err(
                "legacy credential differs from the existing keyring value; migration was deferred"
                    .to_string(),
            );
        }
    } else {
        credential_store::CredentialBackend::store(&backend, &provider.id, &api_key)?;
    }
    let verified = credential_store::CredentialBackend::load(&backend, &provider.id)?;
    if verified.as_deref() != Some(api_key.as_str()) {
        return Err("legacy credential verification failed".to_string());
    }
    provider.api_key.clear();
    crate::database::mark_secure_scrub_pending(conn)
        .and_then(|_| persist_provider_configs(conn, &providers))
        .and_then(|_| crate::database::secure_scrub_storage(conn))
        .map_err(|error| error.to_string())
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
    save_providers_raw(conn, &providers).map_err(|error| error.to_string())?;
    credential_store::delete_api_key_result(&id)
        .map_err(|error| format!("provider removed, but credential cleanup failed: {error}"))
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
    let mode = AiMode::from_settings_key(&key)
        .ok_or_else(|| "unknown AI mode assignment key".to_string())?;
    resolve_provider_model(conn, &provider_id, &model, mode)?;
    let config = ModelConfig {
        provider_id,
        provider_name,
        model,
    };
    save_model_config(conn, &key, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_model_config(
    db: tauri::State<'_, crate::database::DbState>,
    key: String,
) -> Result<Option<ModelConfig>, String> {
    let guard = db.conn.lock().unwrap();
    let conn = guard.as_ref().ok_or("no workspace open")?;
    let mode = AiMode::from_settings_key(&key)
        .ok_or_else(|| "unknown AI mode assignment key".to_string())?;
    Ok(load_model_config(conn, mode.settings_key()))
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::credential_store::CredentialBackend;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryCredentialBackend {
        values: Mutex<HashMap<String, String>>,
        fail_store: bool,
    }

    impl credential_store::CredentialBackend for MemoryCredentialBackend {
        fn store(&self, provider_id: &str, api_key: &str) -> Result<(), String> {
            if self.fail_store {
                return Err("injected store failure".to_string());
            }
            self.values
                .lock()
                .unwrap()
                .insert(provider_id.to_string(), api_key.to_string());
            Ok(())
        }

        fn load(&self, provider_id: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(provider_id).cloned())
        }

        fn delete(&self, provider_id: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(provider_id);
            Ok(())
        }
    }

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

    fn assert_sqlite_storage_excludes(conn: &Connection, sentinel: &str) {
        let database_path: String = conn
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        for path in [
            std::path::PathBuf::from(&database_path),
            std::path::PathBuf::from(format!("{database_path}-wal")),
        ] {
            if let Ok(bytes) = fs::read(&path) {
                assert!(
                    !bytes
                        .windows(sentinel.len())
                        .any(|window| window == sentinel.as_bytes()),
                    "{} retained credential plaintext",
                    path.display(),
                );
            }
        }
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
        let mut providers = load_persisted_providers(&conn);
        let backend = MemoryCredentialBackend::default();

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
        save_providers_with_backend(&conn, &providers, &backend).unwrap();

        let reloaded = load_providers_with_backend(&conn, &backend);
        assert!(reloaded.iter().any(|p| p.name == "My Custom"));
    }

    fn legacy_provider(api_key: &str) -> ProviderConfig {
        ProviderConfig {
            id: "legacy-provider".into(),
            name: "Legacy".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://example.invalid/v1".into(),
            api_key: api_key.into(),
            models: vec!["model-a".into()],
            enabled: true,
            is_default: true,
            capabilities: ProviderCapabilities::default(),
            created_at: 0,
        }
    }

    #[test]
    fn provider_view_serialization_never_contains_api_key() {
        let provider = legacy_provider("NF_PROVIDER_SENTINEL");
        let serialized = serde_json::to_string(&ProviderConfigView::from(&provider)).unwrap();
        assert!(!serialized.contains("NF_PROVIDER_SENTINEL"));
        assert!(!serialized.contains("\"api_key\""));
        assert!(serialized.contains("\"has_api_key\":true"));
    }

    #[test]
    fn successful_legacy_migration_is_verified_then_scrubbed() {
        let conn = temp_db();
        persist_provider_configs(&conn, &[legacy_provider("NF_MIGRATION_SENTINEL")]).unwrap();
        let backend = MemoryCredentialBackend::default();

        let loaded = load_providers_with_backend(&conn, &backend);
        assert_eq!(loaded[0].api_key, "NF_MIGRATION_SENTINEL");
        assert_eq!(
            backend.load("legacy-provider").unwrap().as_deref(),
            Some("NF_MIGRATION_SENTINEL")
        );
        let persisted = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTINGS_KEY_PROVIDERS],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert!(!persisted.contains("NF_MIGRATION_SENTINEL"));
        assert_sqlite_storage_excludes(&conn, "NF_MIGRATION_SENTINEL");
    }

    #[test]
    fn failed_legacy_migration_preserves_the_only_copy() {
        let conn = temp_db();
        persist_provider_configs(&conn, &[legacy_provider("NF_ONLY_COPY_SENTINEL")]).unwrap();
        let backend = MemoryCredentialBackend {
            fail_store: true,
            ..MemoryCredentialBackend::default()
        };

        let loaded = load_providers_with_backend(&conn, &backend);
        assert_eq!(loaded[0].api_key, "NF_ONLY_COPY_SENTINEL");
        let persisted = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTINGS_KEY_PROVIDERS],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert!(persisted.contains("NF_ONLY_COPY_SENTINEL"));
    }

    #[test]
    fn restart_after_keyring_write_resumes_scrub_without_overwrite() {
        let conn = temp_db();
        persist_provider_configs(&conn, &[legacy_provider("NF_RESTART_SENTINEL")]).unwrap();
        let backend = MemoryCredentialBackend::default();
        backend
            .store("legacy-provider", "NF_RESTART_SENTINEL")
            .unwrap();

        let loaded = load_providers_with_backend(&conn, &backend);
        assert_eq!(loaded[0].api_key, "NF_RESTART_SENTINEL");
        let persisted = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTINGS_KEY_PROVIDERS],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert!(!persisted.contains("NF_RESTART_SENTINEL"));
        assert_sqlite_storage_excludes(&conn, "NF_RESTART_SENTINEL");
    }

    #[test]
    fn interrupted_physical_scrub_retries_when_database_reopens() {
        let conn = temp_db();
        let database_path: String = conn
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let workspace = std::path::Path::new(&database_path)
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap()
            .to_path_buf();
        persist_provider_configs(&conn, &[legacy_provider("NF_INTERRUPTED_SENTINEL")]).unwrap();
        crate::database::mark_secure_scrub_pending(&conn).unwrap();
        persist_provider_configs(&conn, &[legacy_provider("")]).unwrap();
        drop(conn);

        let reopened = crate::database::open_for_workspace(&workspace).unwrap();
        assert_sqlite_storage_excludes(&reopened, "NF_INTERRUPTED_SENTINEL");
    }

    #[test]
    fn mismatched_keyring_value_never_erases_the_legacy_copy() {
        let conn = temp_db();
        persist_provider_configs(&conn, &[legacy_provider("NF_VALID_LEGACY")]).unwrap();
        let backend = MemoryCredentialBackend::default();
        backend
            .store("legacy-provider", "NF_STALE_KEYRING")
            .unwrap();

        let loaded = load_providers_with_backend(&conn, &backend);
        assert_eq!(loaded[0].api_key, "NF_VALID_LEGACY");
        let persisted = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTINGS_KEY_PROVIDERS],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert!(persisted.contains("NF_VALID_LEGACY"));
        assert_eq!(
            backend.load("legacy-provider").unwrap().as_deref(),
            Some("NF_STALE_KEYRING"),
        );
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

    #[test]
    fn four_modes_resolve_provider_scoped_duplicate_model_ids() {
        let conn = temp_db();
        let make_provider = |id: &str, model: &str| ProviderConfig {
            id: id.into(),
            name: id.into(),
            provider_type: "openai_compatible".into(),
            base_url: format!("https://{id}.invalid/v1"),
            api_key: String::new(),
            models: vec![model.into()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities::default(),
            created_at: 0,
        };
        let mut ghost = default_ollama_provider();
        ghost.models = vec!["shared-model".into()];
        persist_provider_configs(
            &conn,
            &[
                make_provider("chat-provider", "shared-model"),
                make_provider("agent-provider", "shared-model"),
                make_provider("inline-provider", "shared-model"),
                ghost,
            ],
        )
        .unwrap();
        for (mode, provider_id) in [
            (AiMode::Chat, "chat-provider"),
            (AiMode::Agent, "agent-provider"),
            (AiMode::Inline, "inline-provider"),
            (AiMode::Ghost, DEFAULT_OLLAMA_ID),
        ] {
            save_model_config(
                &conn,
                mode.settings_key(),
                &ModelConfig {
                    provider_id: provider_id.into(),
                    provider_name: provider_id.into(),
                    model: "shared-model".into(),
                },
            )
            .unwrap();
            let resolved = resolve_mode_model(&conn, mode).unwrap();
            assert_eq!(resolved.provider.id, provider_id);
            assert_eq!(resolved.model, "shared-model");
        }
    }

    #[test]
    fn configured_mode_never_falls_back_when_provider_is_disabled() {
        let conn = temp_db();
        let provider = ProviderConfig {
            id: "disabled-provider".into(),
            name: "Disabled".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://disabled.invalid/v1".into(),
            api_key: String::new(),
            models: vec!["model-a".into()],
            enabled: false,
            is_default: false,
            capabilities: ProviderCapabilities::default(),
            created_at: 0,
        };
        persist_provider_configs(&conn, &[provider, default_ollama_provider()]).unwrap();
        save_model_config(
            &conn,
            AiMode::Chat.settings_key(),
            &ModelConfig {
                provider_id: "disabled-provider".into(),
                provider_name: "Disabled".into(),
                model: "model-a".into(),
            },
        )
        .unwrap();
        let error = resolve_mode_model(&conn, AiMode::Chat).unwrap_err();
        assert!(error.contains("disabled"));
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
        assert!(
            clamped.chat,
            "chat is genuinely supported and must pass through"
        );
        assert!(
            clamped.streaming,
            "streaming is genuinely supported and must pass through"
        );
        assert!(
            clamped.coding,
            "coding is genuinely supported and must pass through"
        );
        assert!(
            !clamped.fim,
            "fim must be sanitized to false - no OpenAI-compatible FIM adapter exists"
        );
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
        assert!(
            !config.capabilities.fim,
            "a freshly built openai_compatible config must not claim fim support"
        );
    }

    /// Test 2 (Adapter Enforcement): `AdapterKind::Unimplemented` must
    /// reject every single capability, regardless of what was requested -
    /// this is the exact bug the Phase 5 audit found (a config for a
    /// no-adapter provider type previously kept `chat: true`/
    /// `streaming: true`/`coding: true` from `ProviderCapabilities::default()`
    /// despite having no working adapter). No live `provider_type` string
    /// maps to `Unimplemented` any more - Anthropic and Gemini both used to,
    /// until `providers::anthropic`/`providers::gemini` gave them real
    /// adapters (see the `*_adapter_gets_real_capabilities_not_zeroed_out`
    /// tests below) - so this exercises `max_capabilities_for` directly to
    /// keep covering the contract for whatever provider lands here next.
    #[test]
    fn unimplemented_adapter_rejects_every_requested_capability() {
        let max = max_capabilities_for(AdapterKind::Unimplemented);
        assert!(!max.chat, "Unimplemented must not claim chat support");
        assert!(
            !max.streaming,
            "Unimplemented must not claim streaming support"
        );
        assert!(!max.coding, "Unimplemented must not claim coding support");
        assert!(!max.vision, "Unimplemented must not claim vision support");
        assert!(
            !max.tool_calling,
            "Unimplemented must not claim tool_calling support"
        );
        assert!(
            !max.function_calling,
            "Unimplemented must not claim function_calling support"
        );
        assert!(
            !max.embeddings,
            "Unimplemented must not claim embeddings support"
        );
        assert!(!max.fim, "Unimplemented must not claim fim support");
        assert_eq!(
            max.context_length, 0,
            "Unimplemented must not claim any usable context length"
        );
    }

    /// Gemini now has a real adapter (`providers::gemini`) - a config built
    /// with `provider_type: "gemini"` must get real capabilities, not the
    /// zeroed-out `Unimplemented` treatment it used to get.
    #[test]
    fn gemini_adapter_gets_real_capabilities_not_zeroed_out() {
        let config = build_provider_config(
            "Gemini".into(),
            "gemini".into(),
            "https://generativelanguage.googleapis.com/v1beta".into(),
            "test-key".into(),
            false,
        );
        assert!(config.capabilities.chat);
        assert!(config.capabilities.streaming);
        assert!(config.capabilities.coding);
        assert!(
            !config.capabilities.fim,
            "Gemini has no raw/FIM completion adapter"
        );
        assert_eq!(config.adapter_kind(), AdapterKind::Gemini);
    }

    /// Anthropic now has a real adapter (`providers::anthropic`) - a config
    /// built with `provider_type: "anthropic"` must get real capabilities,
    /// not the zeroed-out `Unimplemented` treatment Gemini still gets.
    #[test]
    fn anthropic_adapter_gets_real_capabilities_not_zeroed_out() {
        let config = build_provider_config(
            "Claude".into(),
            "anthropic".into(),
            "https://api.anthropic.com".into(),
            "sk-test".into(),
            false,
        );
        assert!(config.capabilities.chat);
        assert!(config.capabilities.streaming);
        assert!(config.capabilities.coding);
        assert!(
            !config.capabilities.fim,
            "Anthropic has no raw/FIM completion adapter"
        );
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
        assert!(
            ollama.capabilities.fim,
            "Ollama genuinely has a working FIM adapter and must keep declaring it"
        );
        assert_eq!(ollama.adapter_kind(), AdapterKind::Ollama);
    }

    #[test]
    fn adapter_kind_classification_matches_expected_routing() {
        assert_eq!(adapter_kind_for("ollama"), AdapterKind::Ollama);
        assert_eq!(
            adapter_kind_for("openai_compatible"),
            AdapterKind::OpenAiCompatible
        );
        assert_eq!(adapter_kind_for("openai"), AdapterKind::OpenAiCompatible);
        assert_eq!(adapter_kind_for("anthropic"), AdapterKind::Anthropic);
        assert_eq!(adapter_kind_for("gemini"), AdapterKind::Gemini);
    }

    #[test]
    fn provider_config_adapter_kind_method_matches_free_function() {
        let ollama = default_ollama_provider();
        assert_eq!(
            ollama.adapter_kind(),
            adapter_kind_for(&ollama.provider_type)
        );
    }

    // ── Wave 3 focused contract tests ───────────────────────────────────

    #[test]
    fn resolved_provider_model_fails_on_model_not_configured_for_provider() {
        let conn = temp_db();
        let provider = ProviderConfig {
            id: "cloud-1".into(),
            name: "Cloud".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://cloud.invalid/v1".into(),
            api_key: "sk-test".into(),
            models: vec!["model-x".into()],
            enabled: true,
            is_default: false,
            capabilities: all_true_capabilities(),
            created_at: 0,
        };
        persist_provider_configs(&conn, &[provider]).unwrap();
        let error = resolve_provider_model(&conn, "cloud-1", "model-y", AiMode::Chat).unwrap_err();
        assert!(error.contains("model-y") && error.contains("not configured"));
    }

    #[test]
    fn ghost_mode_requires_fim_capability() {
        let conn = temp_db();
        let provider = ProviderConfig {
            id: "cloud-no-fim".into(),
            name: "NoFIM".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://nofim.invalid/v1".into(),
            api_key: String::new(),
            models: vec!["m".into()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities {
                fim: false,
                ..all_true_capabilities()
            },
            created_at: 0,
        };
        persist_provider_configs(&conn, &[provider]).unwrap();
        let error = resolve_mode_model(&conn, AiMode::Ghost).unwrap_err();
        assert!(error.contains("no configured provider supports"));
    }

    #[test]
    fn inline_mode_requires_coding_capability() {
        let conn = temp_db();
        let provider = ProviderConfig {
            id: "no-coding".into(),
            name: "NoCoding".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://nocoding.invalid/v1".into(),
            api_key: String::new(),
            models: vec!["m".into()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities {
                coding: false,
                ..ProviderCapabilities::default()
            },
            created_at: 0,
        };
        persist_provider_configs(&conn, &[provider]).unwrap();
        let error = resolve_mode_model(&conn, AiMode::Inline).unwrap_err();
        assert!(error.contains("no configured provider supports"));
    }

    #[test]
    fn resolve_provider_model_fails_on_deleted_provider() {
        let conn = temp_db();
        persist_provider_configs(&conn, &[default_ollama_provider()]).unwrap();
        let error =
            resolve_provider_model(&conn, "ghost-provider", "model", AiMode::Chat).unwrap_err();
        assert!(error.contains("not found"));
    }

    #[test]
    fn custom_ollama_endpoint_is_used_for_discovery() {
        let conn = temp_db();
        let custom = ProviderConfig {
            id: "custom-ollama".into(),
            name: "Custom Ollama".into(),
            provider_type: "ollama".into(),
            base_url: "http://10.0.0.1:11434".into(),
            api_key: String::new(),
            models: vec!["custom-model".into()],
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities {
                fim: true,
                chat: true,
                streaming: true,
                ..ProviderCapabilities::default()
            },
            created_at: 0,
        };
        persist_provider_configs(&conn, &[custom]).unwrap();
        let resolved = resolve_mode_model(&conn, AiMode::Ghost).unwrap();
        assert_eq!(resolved.provider.base_url, "http://10.0.0.1:11434");
    }

    #[test]
    fn ghost_mode_prefers_ollama_over_non_fim_providers() {
        let conn = temp_db();
        let chat_provider = ProviderConfig {
            id: "chat-prov".into(),
            name: "Chat".into(),
            provider_type: "openai_compatible".into(),
            base_url: "https://chat.invalid/v1".into(),
            api_key: String::new(),
            models: vec!["m".into()],
            enabled: true,
            is_default: true,
            capabilities: ProviderCapabilities {
                fim: false,
                ..ProviderCapabilities::default()
            },
            created_at: 0,
        };
        let mut ollama = default_ollama_provider();
        ollama.models = vec!["local-model".into()];
        persist_provider_configs(&conn, &[chat_provider, ollama]).unwrap();
        let resolved = resolve_mode_model(&conn, AiMode::Ghost).unwrap();
        assert_eq!(resolved.provider.id, DEFAULT_OLLAMA_ID);
    }

    #[test]
    fn ai_mode_settings_key_mapping_is_bijective() {
        for mode in [AiMode::Chat, AiMode::Agent, AiMode::Inline, AiMode::Ghost] {
            let key = mode.settings_key();
            let recovered = AiMode::from_settings_key(key);
            assert_eq!(
                recovered,
                Some(mode),
                "settings_key roundtrip failed for {:?}",
                mode
            );
        }
    }
}
