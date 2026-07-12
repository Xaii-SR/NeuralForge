use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    raw: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: Option<String>,
    error: Option<String>,
}

/// Returns a ghost text suggestion using Ollama's local FIM model.
/// Formats the editor context into a Fill-in-the-Middle prompt and calls
/// Ollama's `/api/generate` endpoint. Falls back to empty string on failure.
#[tauri::command]
pub async fn fetch_ghost_suggestion(
    prefix: String,
    suffix: String,
    file_path: String,
) -> Result<String, String> {
    let fim_prompt = format!(
        "<|fim_prefix|>{}<|fim_suffix|>{}<|fim_middle|>",
        prefix, suffix
    );

    let client = reqwest::Client::new();
    let payload = OllamaRequest {
        model: "qwen2.5-coder:1.5b".to_string(),
        prompt: fim_prompt,
        stream: false,
        raw: true,
    };

    let result = client
        .post("http://localhost:11434/api/generate")
        .json(&payload)
        .send()
        .await;

    match result {
        Ok(resp) => {
            if let Ok(body) = resp.json::<OllamaResponse>().await {
                if let Some(err) = body.error {
                    tracing::warn!(target: "ai", event = "ollama_error", error = %err);
                    return Ok(String::new());
                }
                if let Some(text) = body.response {
                    // Strip markdown code fences if present
                    let cleaned = text
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim()
                        .to_string();
                    tracing::info!(target: "ai", event = "ghost_suggestion_ok", file_path = %file_path);
                    return Ok(cleaned);
                }
            }
            Ok(String::new())
        }
        Err(e) => {
            tracing::warn!(target: "ai", event = "ollama_connection_failed", error = %e);
            Ok(String::new())
        }
    }
}