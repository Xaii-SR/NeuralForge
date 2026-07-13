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
}

#[derive(Debug, Deserialize)]
pub struct OllamaMessageResponse {
    pub message: Message,
    pub done: bool,
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

    /// Executes a standard, non-streaming chat completion request against the local Ollama daemon.
    pub async fn execute_chat(&self, request: GatewayRequest) -> Result<String, String> {
        let endpoint = format!("{}/api/chat", self.base_url);

        let response = self
            .client
            .post(&endpoint)
            .json(&request)
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
}