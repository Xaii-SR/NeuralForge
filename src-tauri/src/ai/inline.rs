use crate::ai::completion;
use crate::ai::health::HealthRegistry;
use crate::ai::providers::ollama;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize)]
pub struct InlineStreamPayload {
    pub chunk: String,
    pub done: bool,
    pub error: Option<String>,
}

/// Generates inline code edits based on a user's prompt and selected code.
/// Streams real tokens from Ollama via the `inline-stream` Tauri event,
/// reusing the same health-cooldown gate as chat_with_model_core in ai::mod.
#[tauri::command]
pub async fn stream_inline_edit(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    prompt: String,
    selected_text: String,
    file_path: String,
) -> Result<(), String> {
    if !health.is_healthy("ollama") {
        let _ = app.emit(
            "inline-stream",
            InlineStreamPayload {
                chunk: String::new(),
                done: true,
                error: Some("Ollama is in cooldown after repeated failures - try again shortly".to_string()),
            },
        );
        return Ok(());
    }

    let models = match ollama::list_models().await {
        Ok(m) => m,
        Err(e) => {
            let _ = app.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true, error: Some(e.to_string()) });
            return Ok(());
        }
    };
    let Some(model) = models.first().map(|m| m.name.clone()) else {
        let _ = app.emit(
            "inline-stream",
            InlineStreamPayload { chunk: String::new(), done: true, error: Some("no local Ollama models available".to_string()) },
        );
        return Ok(());
    };

    let instruction = format!(
        "Modify the following code according to the instruction. Output ONLY the raw modified code. No markdown fences, no explanations.\n\nInstruction: {}\n\nCode:\n{}\n\nModified code:",
        prompt, selected_text
    );
    let messages = vec![ollama::ChatMessage { role: "user".to_string(), content: instruction }];

    let accumulated = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let accumulated_for_stream = accumulated.clone();
    let app_for_stream = app.clone();
    let start = std::time::Instant::now();
    let result = ollama::chat_stream(&model, messages, move |token, done| {
        if !token.is_empty() {
            if let Ok(mut buf) = accumulated_for_stream.lock() {
                buf.push_str(token);
            }
            let _ = app_for_stream.emit("inline-stream", InlineStreamPayload { chunk: token.to_string(), done: false, error: None });
        }
        if done {
            let _ = app_for_stream.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true, error: None });
        }
    })
    .await;

    match &result {
        Ok(_) => {
            health.record_success("ollama", start.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(target: "ai", event = "inline_edit_completed", file_path = %file_path);

            // Real per-line diff for accurate insert/delete decorations,
            // computed now that the full response is known.
            let generated = accumulated.lock().map(|b| b.clone()).unwrap_or_default();
            let request_id = format!(
                "inline-{}",
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
            );
            completion::stream_inline_diff(app.clone(), request_id, &selected_text, &generated).await;
        }
        Err(e) => {
            health.record_failure("ollama");
            tracing::warn!(target: "ai", event = "inline_edit_failed", file_path = %file_path, error = %e);
            let _ = app.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true, error: Some(e.to_string()) });
        }
    }

    Ok(())
}
