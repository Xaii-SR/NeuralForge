use std::path::Path;

/// Downloads a documentation page from the given URL, converts HTML to
/// Markdown, and saves it locally to `.neuralforge/docs/{name}.md`.
/// Returns the local file path on success.
#[tauri::command]
pub async fn fetch_and_cache_doc(name: String, url: String) -> Result<String, String> {
    // Download the HTML page
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    // Convert HTML to Markdown
    let markdown = html2md::parse_html(&html);

    // Ensure the output directory exists
    let output_dir = Path::new(".neuralforge").join("docs");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create docs directory: {e}"))?;

    let file_path = output_dir.join(format!("{name}.md"));
    std::fs::write(&file_path, &markdown)
        .map_err(|e| format!("Failed to write doc file: {e}"))?;

    let path_str = file_path
        .to_string_lossy()
        .to_string();

    tracing::info!(
        target: "docs",
        event = "doc_cached",
        name = %name,
        url = %url,
        path = %path_str
    );

    Ok(path_str)
}

/// Returns a list of all cached documentation names (without .md extension).
#[tauri::command]
pub fn list_cached_docs() -> Result<Vec<String>, String> {
    let docs_dir = Path::new(".neuralforge").join("docs");
    if !docs_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    let entries = std::fs::read_dir(&docs_dir)
        .map_err(|e| format!("Failed to read docs dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            if let Some(stem) = path.file_stem() {
                names.push(stem.to_string_lossy().to_string());
            }
        }
    }
    Ok(names)
}

/// Reads the full Markdown content of a cached documentation file.
#[tauri::command]
pub fn read_cached_doc(name: String) -> Result<String, String> {
    let file_path = Path::new(".neuralforge").join("docs").join(format!("{name}.md"));
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read doc '{}': {}", name, e))
}
