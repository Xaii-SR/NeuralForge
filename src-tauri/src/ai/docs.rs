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