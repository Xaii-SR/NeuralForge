use crate::core::errors::{AppError, AppResult};
use crate::core::events::AI_RESPONSE_TOKEN;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const BASE_URL: &str = "http://localhost:11434";

#[derive(Serialize, Clone)]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: u64,
    pub parameter_size: String,
    pub quantization_level: String,
    pub context_length: u64,
    pub family: String,
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

pub async fn health_check() -> bool {
    client()
        .get(format!("{BASE_URL}/api/version"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn list_models() -> AppResult<Vec<OllamaModel>> {
    let resp = client()
        .get(format!("{BASE_URL}/api/tags"))
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

pub async fn remove_model(name: &str) -> AppResult<()> {
    let resp = client()
        .delete(format!("{BASE_URL}/api/delete"))
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
    let resp = client()
        .post(format!("{BASE_URL}/api/pull"))
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

/// Pure streaming logic, decoupled from Tauri so it's testable against a
/// real Ollama instance without needing an AppHandle.
pub async fn chat_stream<F>(model: &str, messages: Vec<ChatMessage>, mut on_token: F) -> AppResult<()>
where
    F: FnMut(&str, bool),
{
    let resp = client()
        .post(format!("{BASE_URL}/api/chat"))
        .json(&serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        }))
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
            let done = parsed.get("done").and_then(|v| v.as_bool()).unwrap_or(false);

            on_token(token, done);
        }
    }

    Ok(())
}

pub async fn chat(app: &AppHandle, request_id: &str, model: &str, messages: Vec<ChatMessage>) -> AppResult<()> {
    let request_id = request_id.to_string();
    let app = app.clone();
    chat_stream(model, messages, move |token, done| {
        let _ = app.emit(
            AI_RESPONSE_TOKEN,
            serde_json::json!({
                "request_id": request_id,
                "token": token,
                "done": done,
            }),
        );
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

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

        chat_stream("deepseek-coder:latest", messages, |token, done| {
            accumulated.push_str(token);
            if done {
                saw_done = true;
            }
        })
        .await
        .expect("chat_stream should succeed against a running Ollama instance");

        assert!(saw_done, "expected a final done:true chunk");
        assert!(!accumulated.trim().is_empty(), "expected non-empty streamed content");
    }
}
