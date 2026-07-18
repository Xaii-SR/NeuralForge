use crate::core::errors::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Anthropic's `/v1/messages` requires `max_tokens`; there is no "use the
/// model's default" option like OpenAI-compatible APIs offer. This mirrors
/// what most SDKs default to for a general chat completion.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Model info from Anthropic's `GET /v1/models`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicModel {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub display_name: String,
}

/// Mirrors `openai_compatible::ChatMessage`'s shape so callers/`provider_router`
/// can convert between them without a third message type. Anthropic's wire
/// format additionally requires "system" content out-of-band from the
/// `messages` array - `chat_stream` below extracts any `role: "system"`
/// entries into the top-level `system` field itself, so callers can keep
/// passing messages the same way they do for Ollama/OpenAI-compatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Real client for Anthropic's native `/v1/messages` API - distinct from
/// `openai_compatible::OpenAiCompatibleProvider` because Anthropic's wire
/// format (auth header name, required `anthropic-version` header, top-level
/// `system` field, and SSE event shapes) is not OpenAI chat-completions
/// compatible (see `provider_registry::AdapterKind::Unimplemented`'s doc
/// comment on why this needed its own adapter rather than routing through
/// the generic client).
pub struct AnthropicProvider {
    pub base_url: String,
    pub api_key: String,
    pub client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn default_base_url() -> String {
        "https://api.anthropic.com".to_string()
    }

    pub fn with_default_base_url(api_key: String) -> Self {
        Self::new(Self::default_base_url(), api_key)
    }

    /// Health check: GET {base_url}/v1/models
    pub async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/v1/models", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List models from GET /v1/models
    pub async fn list_models(&self) -> AppResult<Vec<AnthropicModel>> {
        let resp = self
            .client
            .get(format!("{}/v1/models", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("Anthropic endpoint unreachable: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!(
                "Anthropic returned status {}",
                resp.status()
            )));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("bad /v1/models response: {e}")))?;

        let models: Vec<AnthropicModel> = body
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| AnthropicModel {
                        id: m.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        kind: m.get("type").and_then(|v| v.as_str()).unwrap_or("model").to_string(),
                        display_name: m
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Streaming chat completion via POST /v1/messages. Any `role: "system"`
    /// entries in `messages` are pulled into the top-level `system` field
    /// (Anthropic has no "system" role inside the `messages` array); all
    /// other messages pass through as `user`/`assistant` turns unchanged.
    pub async fn chat_stream<F>(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        mut on_token: F,
    ) -> AppResult<(u64, u64)>
    where
        F: FnMut(&str, bool),
    {
        let mut system_prompt = String::new();
        let mut turns: Vec<Value> = Vec::new();
        for m in messages {
            if m.role == "system" {
                if !system_prompt.is_empty() {
                    system_prompt.push('\n');
                }
                system_prompt.push_str(&m.content);
            } else {
                turns.push(serde_json::json!({ "role": m.role, "content": m.content }));
            }
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": turns,
            "max_tokens": DEFAULT_MAX_TOKENS,
            "stream": true,
        });
        if !system_prompt.is_empty() {
            body["system"] = Value::String(system_prompt);
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("chat request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!(
                "Anthropic returned status {} for model {model}",
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

            // Anthropic SSE: "event: <type>\ndata: {json}\n\n" - we only
            // need the JSON payload's own "type" field, so event: lines
            // can be skipped the same way OpenAI's comment lines are.
            while let Some(pos) = buffer.find("\n\n") {
                let sse_block: String = buffer.drain(..=pos + 1).collect();
                for line in sse_block.lines() {
                    let line = line.trim();
                    if line.is_empty() || !line.starts_with("data:") {
                        continue;
                    }
                    let json_str = line.trim_start_matches("data:").trim();
                    let Ok(parsed) = serde_json::from_str::<Value>(json_str) else {
                        continue;
                    };
                    match parsed.get("type").and_then(|v| v.as_str()) {
                        Some("content_block_delta") => {
                            if let Some(text) = parsed
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|v| v.as_str())
                            {
                                if !text.is_empty() {
                                    on_token(text, false);
                                    token_count += 1;
                                }
                            }
                        }
                        Some("message_delta") => {
                            if let Some(out) = parsed
                                .get("usage")
                                .and_then(|u| u.get("output_tokens"))
                                .and_then(|v| v.as_u64())
                            {
                                total_tokens = total_tokens.max(out);
                            }
                        }
                        Some("message_stop") => {
                            on_token("", true);
                        }
                        _ => {}
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

    #[test]
    fn provider_has_correct_base_url() {
        let provider = AnthropicProvider::new("https://api.anthropic.com/".to_string(), "sk-ant-test".to_string());
        assert_eq!(provider.base_url, "https://api.anthropic.com");
    }

    #[test]
    fn with_default_base_url_points_at_the_real_api() {
        let provider = AnthropicProvider::with_default_base_url("sk-ant-test".to_string());
        assert_eq!(provider.base_url, "https://api.anthropic.com");
    }

    /// Requires a real Anthropic API key with credit. Not run by default.
    #[tokio::test]
    #[ignore = "requires a real Anthropic API key"]
    async fn live_anthropic_chat() {
        let api_key = std::env::var("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY to run this test");
        let provider = AnthropicProvider::with_default_base_url(api_key);

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly: hello".to_string(),
        }];

        let mut accumulated = String::new();
        let (tokens, _) = provider
            .chat_stream("claude-3-5-haiku-20241022", messages, |token, _done| {
                accumulated.push_str(token);
            })
            .await
            .expect("should stream from the real Anthropic API");

        assert!(!accumulated.trim().is_empty());
        assert!(tokens > 0);
    }
}
