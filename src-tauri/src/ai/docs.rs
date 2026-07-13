use std::path::Path;

/// Sanitizes a `name` parameter for safe filesystem usage.
/// Rejects or strips `..`, `/`, `\`, and non-alphanumeric characters.
fn sanitize_name(name: &str) -> Result<String, String> {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err("Invalid doc name: path traversal characters detected".to_string());
    }
    let safe: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    if safe.is_empty() {
        return Err("Invalid doc name: empty after sanitization".to_string());
    }
    Ok(safe)
}

/// Validates that a URL is safe for fetching (no SSRF to internal networks).
fn validate_url(url_str: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("Unsupported URL scheme: {scheme}"));
    }
    if let Some(host) = parsed.host_str() {
        let host_lower = host.to_lowercase();
        if host_lower == "localhost"
            || host_lower.starts_with("127.")
            || host_lower == "::1"
            || host_lower.starts_with("10.")
            || host_lower.starts_with("192.168.")
            || host_lower.starts_with("172.16.") || host_lower.starts_with("172.17.") || host_lower.starts_with("172.18.")
            || host_lower.starts_with("172.19.") || host_lower.starts_with("172.20.") || host_lower.starts_with("172.21.")
            || host_lower.starts_with("172.22.") || host_lower.starts_with("172.23.") || host_lower.starts_with("172.24.")
            || host_lower.starts_with("172.25.") || host_lower.starts_with("172.26.") || host_lower.starts_with("172.27.")
            || host_lower.starts_with("172.28.") || host_lower.starts_with("172.29.") || host_lower.starts_with("172.30.")
            || host_lower.starts_with("172.31.")
            || host_lower.starts_with("169.254.")
            || host_lower == "[::1]"
        {
            return Err("SSRF blocked: requests to internal/private networks are not allowed".to_string());
        }
    }
    Ok(parsed)
}

/// Downloads a documentation page from the given URL, converts HTML to
/// Markdown, and saves it locally to `.neuralforge/docs/{name}.md`.
/// Returns the local file path on success.
#[tauri::command]
pub async fn fetch_and_cache_doc(name: String, url: String) -> Result<String, String> {
    let safe_name = sanitize_name(&name)?;
    let _valid_url = validate_url(&url)?;

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

    let markdown = html2md::parse_html(&html);

    let output_dir = Path::new(".neuralforge").join("docs");
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create docs directory: {e}"))?;

    let file_path = output_dir.join(format!("{safe_name}.md"));
    std::fs::write(&file_path, &markdown)
        .map_err(|e| format!("Failed to write doc file: {e}"))?;

    let path_str = file_path.to_string_lossy().to_string();

    tracing::info!(
        target: "docs",
        event = "doc_cached",
        name = %safe_name,
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
    let safe_name = sanitize_name(&name)?;
    let file_path = Path::new(".neuralforge").join("docs").join(format!("{safe_name}.md"));
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read doc '{}': {}", safe_name, e))
}