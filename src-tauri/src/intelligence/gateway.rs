use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct GatewayRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaMessageResponse {
    pub message: Message,
    pub done: bool,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

pub struct OllamaGateway {
    client: Client,
    base_url: String,
}

impl OllamaGateway {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to initialize Ollama HTTP client: {}", e))?;

        Ok(Self {
            client,
            base_url: "http://127.0.0.1:11434".to_string(),
        })
    }

    /// Executes a chat completion request. If `api_key` is present, routes to
    /// OpenAI-compatible cloud API (OpenAI / OpenRouter). Otherwise defaults to
    /// the local Ollama daemon.
    pub async fn execute_chat(&self, request: GatewayRequest) -> Result<String, String> {
        if let Some(ref api_key) = request.api_key {
            if !api_key.is_empty() {
                return self.cloud_chat(&request).await;
            }
        }
        self.local_chat(&request).await
    }

    async fn local_chat(&self, request: &GatewayRequest) -> Result<String, String> {
        let endpoint = format!("{}/api/chat", self.base_url);

        let response = self
            .client
            .post(&endpoint)
            .json(request)
            .send()
            .await
            .map_err(|e| format!("Network failure communicating with Ollama: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(format!("Ollama API Error ({}): {}", status, error_body));
        }

        let ollama_response: OllamaMessageResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama JSON response: {}", e))?;

        Ok(ollama_response.message.content)
    }

    async fn cloud_chat(&self, request: &GatewayRequest) -> Result<String, String> {
        let api_key = request.api_key.as_deref().unwrap_or("");
        let endpoint = "https://api.openai.com/v1/chat/completions";

        let response = self
            .client
            .post(endpoint)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| format!("Cloud API network failure: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(format!("Cloud API Error ({}): {}", status, error_body));
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse cloud API response: {}", e))?;

        Ok(openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default())
    }
}