use crate::core::errors::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Model info from Gemini's `GET /v1beta/models`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiModel {
    pub name: String,
    pub display_name: String,
}

/// Mirrors `anthropic::ChatMessage`/`openai_compatible::ChatMessage`'s shape.
/// Gemini has no "system" role either - `chat_stream` extracts
/// `role: "system"` entries into the request's top-level `systemInstruction`
/// field, same pattern as `anthropic::AnthropicProvider::chat_stream`'s
/// `system` extraction. Gemini's own role names are "user"/"model" rather
/// than "user"/"assistant"; `chat_stream` remaps "assistant" -> "model" so
/// callers can keep passing the same role strings they use everywhere else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Real client for Google's native Gemini API - distinct from both
/// `openai_compatible::OpenAiCompatibleProvider` and
/// `anthropic::AnthropicProvider` because Gemini's wire format differs from
/// both: the API key is a `?key=` query parameter (not an auth header), the
/// endpoint path embeds the model and method
/// (`models/{model}:streamGenerateContent`), and the request/response body
/// uses a `contents: [{role, parts: [{text}]}]` shape rather than a flat
/// `messages` array (see `provider_registry::AdapterKind::Unimplemented`'s
/// doc comment on why Gemini needed its own adapter rather than routing
/// through an existing one).
pub struct GeminiProvider {
    pub base_url: String,
    pub api_key: String,
    pub client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Gemini passes its API key as a `?key=` query parameter rather than a
    /// header (unlike Anthropic/OpenAI-compatible), and `reqwest::Error`'s
    /// `Display` impl can include the full request URL - including that
    /// query string - for transport-level failures. Every error string built
    /// from a `reqwest::Error` in this file must go through this first, so a
    /// failed request never echoes the user's own live key back into a UI
    /// error banner or an exported log file.
    fn redact_key(&self, msg: impl std::fmt::Display) -> String {
        msg.to_string().replace(&self.api_key, "[REDACTED]")
    }

    fn default_base_url() -> String {
        "https://generativelanguage.googleapis.com/v1beta".to_string()
    }

    pub fn with_default_base_url(api_key: String) -> Self {
        Self::new(Self::default_base_url(), api_key)
    }

    /// Health check: GET {base_url}/models?key={api_key}
    pub async fn health_check(&self) -> bool {
        self.client
            .get(format!("{}/models", self.base_url))
            .query(&[("key", &self.api_key)])
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List models from GET /models?key={api_key}
    pub async fn list_models(&self) -> AppResult<Vec<GeminiModel>> {
        let resp = self
            .client
            .get(format!("{}/models", self.base_url))
            .query(&[("key", &self.api_key)])
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("Gemini endpoint unreachable: {}", self.redact_key(e))))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!("Gemini returned status {}", resp.status())));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Provider(format!("bad /models response: {e}")))?;

        let models: Vec<GeminiModel> = body
            .get("models")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| GeminiModel {
                        name: m.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                        display_name: m
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    /// Remaps chat-convention role names to Gemini's own ("assistant" ->
    /// "model"; "user" passes through unchanged). "system" is handled
    /// separately by the caller before this is used.
    fn gemini_role(role: &str) -> &str {
        if role == "assistant" {
            "model"
        } else {
            role
        }
    }

    /// Streaming chat completion via POST
    /// /models/{model}:streamGenerateContent?alt=sse&key={api_key}. Any
    /// `role: "system"` entries in `messages` are pulled into the request's
    /// top-level `systemInstruction` (Gemini has no "system" role inside
    /// `contents`); all other messages pass through as `user`/`model` turns.
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
        let mut contents: Vec<Value> = Vec::new();
        for m in messages {
            if m.role == "system" {
                if !system_prompt.is_empty() {
                    system_prompt.push('\n');
                }
                system_prompt.push_str(&m.content);
            } else {
                contents.push(serde_json::json!({
                    "role": Self::gemini_role(&m.role),
                    "parts": [{ "text": m.content }],
                }));
            }
        }

        let mut body = serde_json::json!({ "contents": contents });
        if !system_prompt.is_empty() {
            body["systemInstruction"] = serde_json::json!({ "parts": [{ "text": system_prompt }] });
        }

        let resp = self
            .client
            .post(format!("{}/models/{model}:streamGenerateContent", self.base_url))
            .query(&[("alt", "sse"), ("key", &self.api_key)])
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("chat request failed: {}", self.redact_key(e))))?;

        if !resp.status().is_success() {
            return Err(AppError::Provider(format!(
                "Gemini returned status {} for model {model}",
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

                    if let Some(candidates) = parsed.get("candidates").and_then(|c| c.as_array()) {
                        for candidate in candidates {
                            if let Some(parts) = candidate
                                .get("content")
                                .and_then(|c| c.get("parts"))
                                .and_then(|p| p.as_array())
                            {
                                for part in parts {
                                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                                        if !t.is_empty() {
                                            on_token(t, false);
                                            token_count += 1;
                                        }
                                    }
                                }
                            }
                            if candidate.get("finishReason").and_then(|v| v.as_str()).is_some() {
                                on_token("", true);
                            }
                        }
                    }
                    if let Some(total) = parsed
                        .get("usageMetadata")
                        .and_then(|u| u.get("candidatesTokenCount"))
                        .and_then(|v| v.as_u64())
                    {
                        total_tokens = total_tokens.max(total);
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
        let provider = GeminiProvider::new(
            "https://generativelanguage.googleapis.com/v1beta/".to_string(),
            "test-key".to_string(),
        );
        assert_eq!(provider.base_url, "https://generativelanguage.googleapis.com/v1beta");
    }

    #[test]
    fn with_default_base_url_points_at_the_real_api() {
        let provider = GeminiProvider::with_default_base_url("test-key".to_string());
        assert_eq!(provider.base_url, "https://generativelanguage.googleapis.com/v1beta");
    }

    #[test]
    fn gemini_role_remaps_assistant_to_model_and_passes_user_through() {
        assert_eq!(GeminiProvider::gemini_role("assistant"), "model");
        assert_eq!(GeminiProvider::gemini_role("user"), "user");
    }

    /// Regression test for a real credential-leak bug found during the final
    /// release audit: reqwest::Error's Display can include the full request
    /// URL (query string and all) for transport-level failures, and Gemini's
    /// API key rides in that query string (`?key=...`) rather than a header
    /// like Anthropic/OpenAI-compatible use. A failed request must never echo
    /// the live key back into an error string a user could see or export.
    #[tokio::test]
    async fn list_models_network_failure_never_echoes_the_api_key() {
        let provider = GeminiProvider::new(
            "http://127.0.0.1:1".to_string(),
            "SECRET_TEST_VALUE_LEAK_CHECK_12345".to_string(),
        );
        let err = provider
            .list_models()
            .await
            .expect_err("connecting to a closed local port must fail");
        let msg = err.to_string();
        assert!(
            !msg.contains("SECRET_TEST_VALUE_LEAK_CHECK_12345"),
            "error message leaked the raw API key: {msg}"
        );
        assert!(msg.contains("[REDACTED]"), "expected a redaction marker in: {msg}");
    }

    /// Requires a real Gemini API key with quota. Not run by default.
    #[tokio::test]
    #[ignore = "requires a real Gemini API key"]
    async fn live_gemini_chat() {
        let api_key = std::env::var("GEMINI_API_KEY").expect("set GEMINI_API_KEY to run this test");
        let provider = GeminiProvider::with_default_base_url(api_key);

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly: hello".to_string(),
        }];

        let mut accumulated = String::new();
        let (tokens, _) = provider
            .chat_stream("gemini-1.5-flash", messages, |token, _done| {
                accumulated.push_str(token);
            })
            .await
            .expect("should stream from the real Gemini API");

        assert!(!accumulated.trim().is_empty());
        assert!(tokens > 0);
    }
}
