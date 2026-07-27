use crate::ai::completion;
use crate::ai::health::HealthRegistry;
use crate::ai::provider_registry::{self, AdapterKind, AiMode};
use crate::ai::provider_router;
use crate::ai::providers::ollama;
use crate::ai::request_registry::{self, RequestRegistry};
use crate::core::errors::{AppError, AppResult};
use crate::core::state::AppState;
use crate::database::DbState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize)]
pub struct InlineStreamPayload {
    pub request_id: String,
    pub workspace_generation: u64,
    pub file_path: String,
    pub document_version: u64,
    pub selection_start_line: u32,
    pub selection_start_column: u32,
    pub selection_end_line: u32,
    pub selection_end_column: u32,
    pub chunk: String,
    pub done: bool,
    pub status: Option<String>,
    pub error: Option<String>,
}

/// Generates inline code edits based on a user's prompt and selected code.
/// Streams real tokens via the `inline-stream` Tauri event, routed through
/// `ai::provider_router::stream_chat` - the same unified dispatch every
/// other AI feature uses. Selected editor text is restricted to an enabled
/// local Ollama provider; cloud transmission requires an explicit consented
/// workflow and is not part of this command.
#[tauri::command]
pub async fn stream_inline_edit(
    app: AppHandle,
    health: State<'_, HealthRegistry>,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
    requests: State<'_, RequestRegistry>,
    request_id: String,
    workspace_generation: u64,
    prompt: String,
    selected_text: String,
    file_path: String,
    document_version: u64,
    selection_start_line: u32,
    selection_start_column: u32,
    selection_end_line: u32,
    selection_end_column: u32,
    share_workspace_context: bool,
) -> Result<(), String> {
    let cancelled = requests.begin(&request_id)?;
    let payload = |chunk: String, done: bool, status: Option<String>, error: Option<String>| {
        InlineStreamPayload {
            request_id: request_id.clone(),
            workspace_generation,
            file_path: file_path.clone(),
            document_version,
            selection_start_line,
            selection_start_column,
            selection_end_line,
            selection_end_column,
            chunk,
            done,
            status,
            error,
        }
    };

    let outcome: AppResult<String> = async {
        let resolved = crate::database::with_workspace_conn_at_generation(
            &state,
            &db,
            workspace_generation,
            |_root, conn| {
                provider_registry::resolve_mode_model(conn, AiMode::Inline)
                    .map_err(AppError::Provider)
            },
        )?;
        if resolved.provider.adapter_kind() != AdapterKind::Ollama
            && !share_workspace_context
        {
            return Err(AppError::CommandRejected(
                "Inline Edit cannot send selected workspace code to a cloud provider without explicit workspace-context consent".to_string(),
            ));
        }

        let instruction = format!(
            "Modify the following code according to the instruction. Output ONLY the raw modified code. No markdown fences, no explanations.\n\nInstruction: {}\n\nCode:\n{}\n\nModified code:",
            prompt, selected_text
        );
        let messages = vec![ollama::ChatMessage {
            role: "user".to_string(),
            content: instruction,
        }];
        let app_for_stream = app.clone();
        let cancelled_for_stream = cancelled.clone();
        let state_for_stream = &state;
        request_registry::run_cancellable(
            &cancelled,
            provider_router::stream_chat(
                &health,
                &resolved.provider,
                &resolved.model,
                messages,
                move |token, _adapter_done| {
                    if token.is_empty()
                        || request_registry::is_cancelled(&cancelled_for_stream)
                        || !state_for_stream.matches_workspace_generation(workspace_generation)
                    {
                        return;
                    }
                    let _ = app_for_stream.emit(
                        "inline-stream",
                        payload(token.to_string(), false, None, None),
                    );
                },
            ),
        )
        .await
    }
    .await;

    if let Ok(generated) = &outcome {
        if state.matches_workspace_generation(workspace_generation)
            && !request_registry::is_cancelled(&cancelled)
        {
            tracing::info!(target: "ai", event = "inline_edit_completed", file_path = %file_path);
            completion::stream_inline_diff(
                app.clone(),
                request_id.clone(),
                &selected_text,
                generated,
            )
            .await;
        }
    }

    let (status, error) = match &outcome {
        Ok(_) if !state.matches_workspace_generation(workspace_generation) => (
            "error",
            Some("workspace changed while Inline Edit was running".to_string()),
        ),
        Ok(_) if request_registry::is_cancelled(&cancelled) => ("cancelled", None),
        Ok(_) => ("success", None),
        Err(AppError::CommandRejected(message)) if message == "request cancelled" => {
            ("cancelled", None)
        }
        Err(error) => {
            tracing::warn!(target: "ai", event = "inline_edit_failed", file_path = %file_path, error = %error);
            ("error", Some(error.to_string()))
        }
    };
    let _ = app.emit(
        "inline-stream",
        payload(
            String::new(),
            true,
            Some(status.to_string()),
            error,
        ),
    );
    requests.finish(&request_id);
    Ok(())
}
