use crate::ai::completion;
use crate::ai::health::HealthRegistry;
use crate::ai::provider_router;
use crate::ai::providers::ollama;
use crate::database::DbState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize)]
pub struct InlineStreamPayload {
    pub chunk: String,
    pub done: bool,
    pub error: Option<String>,
}

/// Generates inline code edits based on a user's prompt and selected code.
/// Streams real tokens via the `inline-stream` Tauri event, routed through
/// `ai::provider_router::stream_chat` - the same unified dispatch every
/// other AI feature uses. Model discovery still lists real installed Ollama
/// models directly (that's this feature's own "which model" policy, not
/// provider communication); `provider_router::resolve_provider_for_model`
/// then decides which adapter actually serves that model id.
#[tauri::command]
pub async fn stream_inline_edit(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    db: State<'_, DbState>,
    prompt: String,
    selected_text: String,
    file_path: String,
) -> Result<(), String> {
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

    let config = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        provider_router::resolve_provider_for_model(guard.as_ref(), &model)
        // guard dropped here, before the streaming call's .await points -
        // a held MutexGuard can't cross an await (see provider_router's
        // doc comments on this exact constraint)
    };

    let instruction = format!(
        "Modify the following code according to the instruction. Output ONLY the raw modified code. No markdown fences, no explanations.\n\nInstruction: {}\n\nCode:\n{}\n\nModified code:",
        prompt, selected_text
    );
    let messages = vec![ollama::ChatMessage { role: "user".to_string(), content: instruction }];

    let app_for_stream = app.clone();
    let result = provider_router::stream_chat(&health, &config, &model, messages, move |token, done| {
        if !token.is_empty() {
            let _ = app_for_stream.emit("inline-stream", InlineStreamPayload { chunk: token.to_string(), done: false, error: None });
        }
        if done {
            let _ = app_for_stream.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true, error: None });
        }
    })
    .await;

    match result {
        Ok(generated) => {
            tracing::info!(target: "ai", event = "inline_edit_completed", file_path = %file_path);

            // Real per-line diff for accurate insert/delete decorations,
            // computed now that the full response is known.
            let request_id = format!(
                "inline-{}",
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
            );
            completion::stream_inline_diff(app.clone(), request_id, &selected_text, &generated).await;
        }
        Err(e) => {
            tracing::warn!(target: "ai", event = "inline_edit_failed", file_path = %file_path, error = %e);
            let _ = app.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true, error: Some(e.to_string()) });
        }
    }

    Ok(())
}
