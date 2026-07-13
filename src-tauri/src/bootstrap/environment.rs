use std::time::Duration;
use serde::Deserialize;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

pub fn enforce_environment_gate() -> Result<(), String> {
    println!("[BOOTSTRAP] Verifying local AI environment...");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let res = client.get("http://127.0.0.1:11434/api/tags").send();

    match res {
        Ok(response) if response.status().is_success() => {
            let tags: OllamaTagsResponse = response.json().map_err(|_| "Failed to parse Ollama models json")?;

            let has_qwen = tags.models.iter().any(|m| m.name.starts_with("qwen"));
            let has_deepseek = tags.models.iter().any(|m| m.name.starts_with("deepseek"));

            if !has_qwen || !has_deepseek {
                return Err("CRITICAL BOOT FAILURE: Required models (qwen, deepseek) are missing. Please run `npm run setup`.".to_string());
            }

            println!("[BOOTSTRAP] Environment verified. Proceeding with application boot.");
            Ok(())
        }
        _ => {
            Err("CRITICAL BOOT FAILURE: Ollama daemon is not running on 127.0.0.1:11434. Please run `npm run setup`.".to_string())
        }
    }
}