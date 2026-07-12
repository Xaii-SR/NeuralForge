use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
pub struct InlineStreamPayload {
    pub chunk: String,
    pub done: bool,
}

/// Generates inline code edits based on a user's prompt and selected code.
/// Emits tokens via the `inline-stream` Tauri event.
#[tauri::command]
pub async fn stream_inline_edit(
    app: tauri::AppHandle,
    prompt: String,
    selected_text: String,
    file_path: String,
) -> Result<(), String> {
    // Build the strict instruction prompt
    let _instruction = format!(
        "Modify the following code according to the instruction. Output ONLY the raw modified code. No markdown boxes, no explanations.\n\nInstruction: {}\n\nCode:\n{}\n\nModified code:",
        prompt, selected_text
    );

    // Emit a start marker
    let _ = app.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: false });

    // Simulate streaming response
    let response = format!(
        "// Inline edit for: {}\n// Instruction: {}\nfn edited_function() {{\n    println!(\"modified by NeuralForge\");\n}}\n",
        file_path, prompt
    );

    // Emit character by character to simulate streaming
    for ch in response.chars() {
        let _ = app.emit("inline-stream", InlineStreamPayload { chunk: ch.to_string(), done: false });
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }

    // Emit done marker
    let _ = app.emit("inline-stream", InlineStreamPayload { chunk: String::new(), done: true });

    tracing::info!(target: "ai", event = "inline_edit_completed", file_path = %file_path);
    Ok(())
}