use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persistent provider configuration stored in SQLite.
///
/// SECURITY NOTE (audited, not yet remediated - flagged per engineering
/// constitution rather than silently shipped): `api_key` is stored as plain
/// text JSON in the `settings` table, the same table used for UI
/// preferences. No OS keychain / encrypted-at-rest layer exists anywhere in
/// this codebase (`Cargo.toml` has no `keyring` or equivalent dependency).
/// This is acceptable for local Ollama (no key) but is a real exposure for
/// any user who adds a cloud provider API key: the key is readable by
/// anything that can read the workspace's `index.db` file. Required
/// migration before cloud providers ship to non-technical users: move
/// `api_key` into the OS credential store (e.g. the `keyring` crate) and
/// store only a reference/id in this struct, mirroring how Windows
/// Credential Manager / macOS Keychain / libsecret are normally used from
/// Rust. Deliberately NOT done in this change per the "no large unrelated
/// refactor" constraint - this is a Level 3+ change of its own.
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
        Self {
            id: Uuid::new_v4().to_string(),
            name: "New Provider".to_string(),
            provider_type: "openai_compatible".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            api_key: String::new(),
            models: Vec::new(),
            enabled: true,
            is_default: false,
            capabilities: ProviderCapabilities::default(),
            created_at: epoch_secs(),
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
    ProviderConfig {
        id: DEFAULT_OLLAMA_ID.to_string(),
        name: "Ollama (Local)".to_string(),
        provider_type: "ollama".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: String::new(),
        models: Vec::new(),
        enabled: true,
        is_default: true,
        // Ollama is the only provider with a real, working FIM adapter
        // today (providers::ollama::generate_raw) - see
        // ai::provider_router::complete_fim.
        capabilities: ProviderCapabilities { fim: true, ..ProviderCapabilities::default() },
        created_at: 0,
    }
}

fn load_providers_raw(conn: &Connection) -> Vec<ProviderConfig> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![SETTINGS_KEY_PROVIDERS],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|json| serde_json::from_str::<Vec<ProviderConfig>>(&json).ok())
    .unwrap_or_else(|| vec![default_ollama_provider()])
}

/// Public read accessor for other `ai::` modules (routing, capability-based
/// model selection) - `load_providers_raw` stays private so persistence
/// details (the `settings` table encoding) aren't leaked beyond this file.
pub fn load_providers(conn: &Connection) -> Vec<ProviderConfig> {
    load_providers_raw(conn)
}

fn save_providers_raw(conn: &Connection, providers: &[ProviderConfig]) -> AppResult<()> {
    let json = serde_json::to_string(providers)
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

    let new = ProviderConfig {
        id: Uuid::new_v4().to_string(),
        name,
        provider_type,
        base_url,
        api_key,
        models: Vec::new(),
        enabled: true,
        is_default: providers.is_empty(),
        capabilities: ProviderCapabilities::default(),
        created_at: epoch_secs(),
    };

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
    if let Some(k) = api_key { provider.api_key = k; }
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
}