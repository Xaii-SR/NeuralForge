use scraper::{Html, Selector};

/// Searches the web using DuckDuckGo Lite (HTML endpoint, no API key required).
/// Returns formatted results with URLs and snippets.
#[tauri::command]
pub async fn search_web(query: String) -> Result<String, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", query);
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Search request failed: {e}"))?;

    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let document = Html::parse_document(&html);

    // Select all result containers
    let result_selector = Selector::parse(".result")
        .map_err(|_| "Failed to parse result selector".to_string())?;
    let url_selector = Selector::parse(".result__url")
        .map_err(|_| "Failed to parse URL selector".to_string())?;
    let snippet_selector = Selector::parse(".result__snippet")
        .map_err(|_| "Failed to parse snippet selector".to_string())?;

    let mut output = String::new();
    let mut count = 0;

    for result in document.select(&result_selector) {
        if count >= 5 {
            break;
        }

        let url_text = result
            .select(&url_selector)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_else(|| "[no url]".to_string());

        let snippet_text = result
            .select(&snippet_selector)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_else(|| "[no snippet]".to_string());

        output.push_str(&format!("[Source: {}]\n{}\n\n", url_text, snippet_text));
        count += 1;
    }

    if output.is_empty() {
        output = "No search results found.".to_string();
    }

    tracing::info!(target: "web", event = "search_completed", query = %query, result_count = count);
    Ok(output)
}