use crate::core::errors::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use tauri::{AppHandle, Emitter};

const BASE_URL: &str = "http://localhost:11434";
const OFFICIAL_LIBRARY_URL: &str = "https://ollama.com/library?sort=popular";
/// A local model may need time to load and generate, but a workflow must not
/// remain indefinitely pending when the Ollama service stops responding.
const CHAT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

#[derive(Serialize, Type, Clone)]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: u64,
    pub parameter_size: String,
    pub quantization_level: String,
    pub context_length: u64,
    pub family: String,
}

/// A model family advertised by Ollama's official public library. Individual
/// tags remain selectable through the validated exact-model input because a
/// family can have many changing tags and quantizations.
#[derive(Serialize, Type, Clone, Debug, PartialEq, Eq)]
pub struct OfficialOllamaModel {
    pub name: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<RawModel>,
}

#[derive(Deserialize)]
struct RawModel {
    name: String,
    size: u64,
    details: RawDetails,
}

#[derive(Deserialize)]
struct RawDetails {
    family: String,
    parameter_size: String,
    quantization_level: String,
    #[serde(default)]
    context_length: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Model references are data sent to the local Ollama API, never shell
/// fragments. Keep the accepted grammar deliberately small while supporting
/// official namespaces and tags such as `qwen3:8b` and `acme/model:latest`.
pub fn valid_model_reference(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && !name.starts_with('/')
        && !name.contains("..")
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '/' | ':')
        })
}

fn extract_official_library_models(html: &str) -> Vec<OfficialOllamaModel> {
    let mut models = BTreeSet::new();
    let marker = "href=\"/library/";
    let mut remainder = html;
    while let Some(start) = remainder.find(marker) {
        let after_marker = &remainder[start + marker.len()..];
        let Some(end) = after_marker.find('"') else {
            break;
        };
        let candidate = &after_marker[..end];
        // The trusted catalog lists family IDs here. Ignore links such as
        // `/library` itself and anything that does not satisfy the exact
        // local pull grammar.
        if valid_model_reference(candidate) {
            models.insert(candidate.to_string());
        }
        remainder = &after_marker[end + 1..];
    }
    models
        .into_iter()
        .map(|name| OfficialOllamaModel { name })
        .collect()
}

/// Reads only the hard-coded official Ollama library URL. The response is
/// treated as untrusted catalog data: it cannot alter endpoints, commands,
/// filesystem roots, or provider settings; invalid entries are discarded.
pub async fn list_official_library_models() -> AppResult<Vec<OfficialOllamaModel>> {
    let response = client()
        .get(OFFICIAL_LIBRARY_URL)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
        .map_err(|error| {
            AppError::Provider(format!("official Ollama catalog unavailable: {error}"))
        })?;
    if !response.status().is_success() {
        return Err(AppError::Provider(format!(
            "official Ollama catalog returned status {}",
            response.status()
        )));
    }
    let body = response.text().await.map_err(|error| {
        AppError::Provider(format!("could not read official Ollama catalog: {error}"))
    })?;
    if body.len() > 3_000_000 {
        return Err(AppError::Provider(
            "official Ollama catalog response was unexpectedly large".to_string(),
        ));
    }
    let models = extract_official_library_models(&body);
    if models.is_empty() {
        return Err(AppError::Provider(
            "official Ollama catalog did not contain valid model entries".to_string(),
        ));
    }
    Ok(models)
}

pub async fn health_check() -> bool {
    health_check_at(BASE_URL).await
}

pub async fn health_check_at(base_url: &str) -> bool {
    client()
        .get(endpoint(base_url, "api/version"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn list_models() -> AppResult<Vec<OllamaModel>> {
    list_models_at(BASE_URL).await
}

pub async fn list_models_at(base_url: &str) -> AppResult<Vec<OllamaModel>> {
    let resp = client()
        .get(endpoint(base_url, "api/tags"))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("Ollama unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Provider(format!(
            "Ollama returned status {}",
            resp.status()
        )));
    }

    let tags: TagsResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Provider(format!("bad /api/tags response: {e}")))?;

    Ok(tags
        .models
        .into_iter()
        .map(|m| OllamaModel {
            name: m.name,
            size_bytes: m.size,
            parameter_size: m.details.parameter_size,
            quantization_level: m.details.quantization_level,
            context_length: m.details.context_length.unwrap_or(0),
            family: m.details.family,
        })
        .collect())
}

/// Raw (non-chat) completion via `/api/generate`, used for fill-in-middle
/// (FIM) ghost-text prompting. This is a genuinely different Ollama API
/// shape than `chat_stream`'s `/api/chat` (a raw prompt string in, not a
/// message list), so it can't be expressed through `chat_stream` - this
/// completes this adapter's own surface rather than introducing a second
/// HTTP client elsewhere. `raw: true` tells Ollama to use the prompt
/// verbatim without applying the model's chat template, required for FIM
/// tokens like `<|fim_prefix|>` to reach the model unmodified.
pub async fn generate_raw(
    model: &str,
    prompt: &str,
    num_predict: u32,
    temperature: f32,
) -> AppResult<String> {
    generate_raw_at(BASE_URL, model, prompt, num_predict, temperature).await
}

pub async fn generate_raw_at(
    base_url: &str,
    model: &str,
    prompt: &str,
    num_predict: u32,
    temperature: f32,
) -> AppResult<String> {
    let resp = client()
        .post(endpoint(base_url, "api/generate"))
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "raw": true,
            "options": { "num_predict": num_predict, "temperature": temperature }
        }))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("Ollama unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Provider(format!(
            "Ollama returned status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Provider(format!("bad /api/generate response: {e}")))?;

    if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
        return Err(AppError::Provider(err.to_string()));
    }

    Ok(body
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub async fn remove_model(name: &str) -> AppResult<()> {
    remove_model_at(BASE_URL, name).await
}

pub async fn remove_model_at(base_url: &str, name: &str) -> AppResult<()> {
    if !valid_model_reference(name) {
        return Err(AppError::CommandRejected(
            "invalid Ollama model reference".to_string(),
        ));
    }
    let resp = client()
        .delete(endpoint(base_url, "api/delete"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("delete request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Provider(format!(
            "Ollama returned status {} removing {name}",
            resp.status()
        )));
    }
    Ok(())
}

pub async fn pull_model(app: &AppHandle, name: &str) -> AppResult<()> {
    pull_model_at(app, BASE_URL, name).await
}

pub async fn pull_model_at(app: &AppHandle, base_url: &str, name: &str) -> AppResult<()> {
    if !valid_model_reference(name) {
        return Err(AppError::CommandRejected(
            "invalid Ollama model reference".to_string(),
        ));
    }
    let resp = client()
        .post(endpoint(base_url, "api/pull"))
        .json(&serde_json::json!({ "name": name, "stream": true }))
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("pull request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Provider(format!(
            "Ollama returned status {} pulling {name}",
            resp.status()
        )));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Provider(e.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                let _ = app.emit(
                    "MODEL_PULL_PROGRESS",
                    serde_json::json!({
                        "name": name,
                        "status": parsed.get("status").and_then(|v| v.as_str()).unwrap_or(""),
                        "completed": parsed.get("completed").and_then(|v| v.as_u64()).unwrap_or(0),
                        "total": parsed.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
                    }),
                );
            }
        }
    }

    tracing::info!(target: "ai", event = "model_pulled", model = name);
    Ok(())
}

/// Ollama's final done:true chunk includes real generation stats - using
/// these for TPS/latency is far more accurate than approximating from
/// whitespace-split word counts on the client side.
#[derive(Default, Clone, Copy)]
pub struct ChatStats {
    pub eval_count: Option<u64>,
    pub eval_duration_ns: Option<u64>,
    pub total_duration_ns: Option<u64>,
}

impl ChatStats {
    pub fn tokens_per_second(&self) -> Option<f64> {
        match (self.eval_count, self.eval_duration_ns) {
            (Some(count), Some(duration_ns)) if duration_ns > 0 => {
                Some(count as f64 / (duration_ns as f64 / 1_000_000_000.0))
            }
            _ => None,
        }
    }
}

/// Pure streaming logic, decoupled from Tauri so it's testable against a
/// real Ollama instance without needing an AppHandle.
pub async fn chat_stream<F>(
    model: &str,
    messages: Vec<ChatMessage>,
    mut on_token: F,
) -> AppResult<ChatStats>
where
    F: FnMut(&str, bool),
{
    chat_stream_at(BASE_URL, model, messages, on_token).await
}

pub async fn chat_stream_at<F>(
    base_url: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    mut on_token: F,
) -> AppResult<ChatStats>
where
    F: FnMut(&str, bool),
{
    let resp = client()
        .post(endpoint(base_url, "api/chat"))
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))
        .timeout(CHAT_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| AppError::Provider(format!("chat request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Provider(format!(
            "Ollama returned status {} for model {model}",
            resp.status()
        )));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut stats = ChatStats::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Provider(e.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| AppError::Provider(format!("bad ollama chat chunk: {e}")))?;

            let token = parsed
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let done = parsed
                .get("done")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            on_token(token, done);

            if done {
                stats.eval_count = parsed.get("eval_count").and_then(|v| v.as_u64());
                stats.eval_duration_ns = parsed.get("eval_duration").and_then(|v| v.as_u64());
                stats.total_duration_ns = parsed.get("total_duration").and_then(|v| v.as_u64());
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_catalog_parser_accepts_only_safe_library_model_ids() {
        let html = r#"
            <a href="/library/qwen3">Qwen</a>
            <a href="/library/acme/model:latest">Namespaced</a>
            <a href="/library/../../not-a-model">Bad</a>
            <a href="/library/qwen3">Duplicate</a>
        "#;
        let names = extract_official_library_models(html)
            .into_iter()
            .map(|model| model.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["acme/model:latest", "qwen3"]);
    }

    #[test]
    fn model_reference_validation_rejects_shell_and_path_injection() {
        assert!(valid_model_reference("qwen3:8b"));
        assert!(valid_model_reference("org/model-name:latest"));
        assert!(!valid_model_reference("qwen3; rm -rf /"));
        assert!(!valid_model_reference("../qwen3"));
        assert!(!valid_model_reference("qwen3 name"));
    }

    #[test]
    fn chat_requests_have_a_finite_deadline() {
        assert_eq!(CHAT_REQUEST_TIMEOUT, std::time::Duration::from_secs(180));
    }

    /// Requires a real local Ollama instance with deepseek-coder:latest
    /// pulled - not mocked, verifies the actual streaming round trip.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn chat_stream_produces_real_tokens_from_local_model() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Reply with exactly the word: hello".to_string(),
        }];

        let mut accumulated = String::new();
        let mut saw_done = false;

        let stats = chat_stream("deepseek-coder:latest", messages, |token, done| {
            accumulated.push_str(token);
            if done {
                saw_done = true;
            }
        })
        .await
        .expect("chat_stream should succeed against a running Ollama instance");

        assert!(saw_done, "expected a final done:true chunk");
        assert!(
            !accumulated.trim().is_empty(),
            "expected non-empty streamed content"
        );
        assert!(
            stats.eval_count.is_some(),
            "expected Ollama to report eval_count"
        );
        assert!(
            stats.tokens_per_second().is_some(),
            "expected a computable TPS from real stats"
        );
    }

    /// Real round trip for the FIM/raw completion path used by ghost text
    /// and autocomplete, reached via `ai::provider_router::complete_fim`
    /// rather than a separate hand-rolled HTTP client in either feature
    /// module.
    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn generate_raw_produces_real_fim_completion() {
        let response = generate_raw(
            "deepseek-coder:latest",
            "<|fim_prefix|>fn add(a: i32, b: i32) -> i32 {\n    <|fim_suffix|>\n}<|fim_middle|>",
            32,
            0.1,
        )
        .await
        .expect("generate_raw should succeed against a running Ollama instance");

        assert!(
            !response.trim().is_empty(),
            "expected a non-empty real completion"
        );
    }
}
