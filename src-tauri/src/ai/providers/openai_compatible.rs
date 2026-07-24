use crate::core::errors::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Model info from an OpenAI-compatible /v1/models endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiModel {
    pub id: String,
    pub object: String,
    pub owned_by: String,
}

/// Chat message mirrors the standard OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Generic streaming chat client for ANY OpenAI-compatible API endpoint.
/// Supports: OpenAI, OpenRouter, Together, Groq, Fireworks, DeepInfra,
/// LM Studio, Ollama's openai-compat mode, vLLM, custom endpoints.
pub struct OpenAiCompatibleProvider {
    pub base_url: String,
    pub api_key: String,
    pub client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn api_url(&self, path: &str) -> String {
        let root = self.base_url.trim_end_matches('/');
        let has_versioned_path = ["/v1", "/v2", "/v3", "/v4"].iter().any(|segment| root.contains(segment));
        if has_versioned_path {
            format!("{}/{}", root, path)
        } else {
            format!("{}/v1/{}", root, path)
        }
    }

    /// Health check: GET {base_url}/v1/models
    pub async fn health_check(&self) -> bool {
        self.client
            .get(self.api_url("models"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List models from /v1/models
    pub async fn list_models(&self) -> AppResult<Vec<OpenAiModel>> {
        let resp = self
            .client
            .get(self.api_url("models"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("OpenAI-compatible endpoint unreachable: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!(
                "Provider returned status {}",
                resp.status()
            )));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("bad /v1/models response: {e}")))?;

        let models: Vec<OpenAiModel> = body
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| OpenAiModel {
                        id: m.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        object: m.get("object").and_then(|v| v.as_str()).unwrap_or("model").to_string(),
                        owned_by: m.get("owned_by").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Streaming chat completion via POST /v1/chat/completions
    pub async fn chat_stream<F>(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        mut on_token: F,
    ) -> AppResult<(u64, u64)>
    where
        F: FnMut(&str, bool),
    {
        let messages_json: Vec<Value> = messages
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let resp = self
            .client
            .post(self.api_url("chat/completions"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": model,
                "messages": messages_json,
                "stream": true,
            }))
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("chat request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!(
                "Provider returned status {} for model {model}",
                resp.status()
            )));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut token_count: u64 = 0;
        let mut total_tokens: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::Provider(e.to_string()))?;
            let text = String::from_utf8_lossy(&chunk).to_string();
            buffer.push_str(&text);

            // Process SSE lines: "data: {json}\n\n"
            while let Some(pos) = buffer.find("\n\n") {
                let sse_block: String = buffer.drain(..=pos + 1).collect();
                for line in sse_block.lines() {
                    let line = line.trim();
                    if line.is_empty() || !line.starts_with("data:") {
                        continue;
                    }
                    let json_str = line.trim_start_matches("data:").trim();
                    if json_str == "[DONE]" {
                        on_token("", true);
                        break;
                    }
                    if let Ok(parsed) = serde_json::from_str::<Value>(json_str) {
                        // OpenAI format: choices[0].delta.content
                        if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                            for choice in choices {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                        if !content.is_empty() {
                                            on_token(content, false);
                                            token_count += 1;
                                        }
                                    }
                                }
                                // Check for finish reason
                                if let Some(finish) = choice.get("finish_reason") {
                                    if finish.as_str() == Some("stop") || finish.as_str() == Some("length") {
                                        on_token("", true);
                                    }
                                }
                            }
                        }
                        // Track usage if present
                        if let Some(usage) = parsed.get("usage") {
                            total_tokens = total_tokens.max(
                                usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                            );
                        }
                    }
                }
            }
        }

        Ok((token_count, total_tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires a running LM Studio or other OpenAI-compatible server on localhost:1234.
    #[tokio::test]
    #[ignore = "requires a running OpenAI-compatible server"]
    async fn local_openai_compatible_chat() {
        let provider = OpenAiCompatibleProvider::new(
            "http://localhost:1234/v1".to_string(),
            "not-needed".to_string(),
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly: hello".to_string(),
        }];

        let mut accumulated = String::new();
        let (tokens, _) = provider
            .chat_stream("local-model", messages, |token, _done| {
                accumulated.push_str(token);
            })
            .await
            .expect("should stream from local server");

        assert!(!accumulated.trim().is_empty());
        assert!(tokens > 0);
    }

    #[test]
    fn provider_has_correct_base_url() {
        let provider = OpenAiCompatibleProvider::new(
            "https://api.openai.com/v1/".to_string(),
            "sk-test".to_string(),
        );
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn api_url_adds_v1_for_plain_roots() {
        let provider = OpenAiCompatibleProvider::new(
            "https://api.openai.com".to_string(),
            "sk-test".to_string(),
        );
        assert_eq!(provider.api_url("models"), "https://api.openai.com/v1/models");
    }

    #[test]
    fn api_url_does_not_duplicate_existing_version_segment() {
        let provider = OpenAiCompatibleProvider::new(
            "https://openrouter.ai/api/v1".to_string(),
            "sk-test".to_string(),
        );
        assert_eq!(provider.api_url("chat/completions"), "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn api_url_preserves_provider_specific_version_prefixes() {
        let provider = OpenAiCompatibleProvider::new(
            "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            "sk-test".to_string(),
        );
        assert_eq!(provider.api_url("models"), "https://ark.cn-beijing.volces.com/api/v3/models");
    }
}
