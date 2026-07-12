/// Returns a simulated ghost text suggestion for Fill-in-the-Middle autocomplete.
/// In production, this would call an FIM model (e.g., StarCoder/CodeLlama).
#[tauri::command]
pub fn fetch_ghost_suggestion(prefix: String, suffix: String, file_path: String) -> Result<String, String> {
    let suggestion = " // AI Suggestion".to_string();
    tracing::info!(target: "ai", event = "ghost_suggestion", file_path = %file_path, prefix_len = prefix.len(), suffix_len = suffix.len());
    Ok(suggestion)
}