pub mod anthropic;
pub mod ollama;
pub mod openai_compatible;

use serde::Serialize;
use specta::Type;

#[derive(Serialize, Type, Clone, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Ollama,
    OpenAi,
    Anthropic,
    Gemini,
    DeepSeek,
    Groq,
    Mistral,
    Together,
    Fireworks,
    OpenRouter,
    HuggingFace,
}

#[derive(Serialize, Type, Clone)]
pub struct ProviderMetadata {
    pub id: ProviderId,
    pub name: String,
    pub is_local: bool,
    pub requires_api_key: bool,
    pub configured: bool,
}

/// Phase 2 stub per blueprint's "Authentication handler stub" requirement.
/// Real credential storage (OS keychain / encrypted SQLite) is a later phase -
/// nothing here ever stores a key, it only reports whether one is configured.
fn has_api_key(_id: &ProviderId) -> bool {
    false
}

pub fn registry() -> Vec<ProviderMetadata> {
    let cloud = [
        (ProviderId::OpenAi, "OpenAI"),
        (ProviderId::Anthropic, "Anthropic"),
        (ProviderId::Gemini, "Gemini"),
        (ProviderId::DeepSeek, "DeepSeek"),
        (ProviderId::Groq, "Groq"),
        (ProviderId::Mistral, "Mistral"),
        (ProviderId::Together, "Together"),
        (ProviderId::Fireworks, "Fireworks"),
        (ProviderId::OpenRouter, "OpenRouter"),
        (ProviderId::HuggingFace, "HuggingFace"),
    ];

    let mut providers = vec![ProviderMetadata {
        id: ProviderId::Ollama,
        name: "Ollama".to_string(),
        is_local: true,
        requires_api_key: false,
        configured: true,
    }];

    providers.extend(cloud.into_iter().map(|(id, name)| {
        let configured = has_api_key(&id);
        ProviderMetadata {
            id,
            name: name.to_string(),
            is_local: false,
            requires_api_key: true,
            configured,
        }
    }));

    providers
}
