use crate::ai::benchmarks::BenchmarkResult;
use crate::ai::health::HealthRegistry;
use crate::ai::providers::ollama::OllamaModel;
use crate::ai::providers::ProviderId;
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone)]
pub struct Preferences {
    /// "speed" | "quality"
    pub goal: String,
    /// "free" | "cheap" | "quality_first"
    pub cost_preference: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            goal: "speed".to_string(),
            cost_preference: "free".to_string(),
        }
    }
}

const SETTINGS_KEY: &str = "ai_preferences";

pub fn save_preferences(conn: &Connection, prefs: &Preferences) -> AppResult<()> {
    let json = serde_json::to_string(prefs).map_err(|e| AppError::Provider(format!("failed to serialize preferences: {e}")))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SETTINGS_KEY, json],
    )
    .map_err(|e| AppError::Provider(format!("failed to save preferences: {e}")))?;
    Ok(())
}

pub fn load_preferences(conn: &Connection) -> Preferences {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", params![SETTINGS_KEY], |row| {
        row.get::<_, String>(0)
    })
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default()
}

/// Rough USD-per-1K-token pricing. Ollama is the only provider with a real,
/// working client right now (Phase 2 left cloud providers as unauthenticated
/// stubs - see providers::has_api_key), so these numbers only ever matter
/// once a cloud provider actually gets wired up; they exist now so the
/// scoring/estimation logic has real numbers to work with instead of being
/// built against a placeholder that would need reworking later.
fn price_per_1k_tokens(provider: &ProviderId) -> f64 {
    match provider {
        ProviderId::Ollama => 0.0,
        ProviderId::Groq => 0.0005,
        ProviderId::DeepSeek => 0.001,
        ProviderId::HuggingFace => 0.001,
        ProviderId::Gemini => 0.002,
        ProviderId::Mistral => 0.002,
        ProviderId::Together => 0.002,
        ProviderId::Fireworks => 0.002,
        ProviderId::Anthropic => 0.003,
        ProviderId::OpenRouter => 0.003,
        ProviderId::OpenAi => 0.005,
    }
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64 / 4).max(1)
}

#[derive(Serialize, Clone)]
pub struct CostEstimate {
    pub estimated_tokens: u64,
    pub estimated_cost_usd: f64,
    pub is_free: bool,
}

pub fn estimate_cost(provider: &ProviderId, prompt: &str) -> CostEstimate {
    // Rough input+output guess: output is assumed comparable in size to input
    // for a chat turn: this is a coarse heuristic, not a tokenizer.
    let tokens = estimate_tokens(prompt) * 2;
    let price = price_per_1k_tokens(provider);
    CostEstimate {
        estimated_tokens: tokens,
        estimated_cost_usd: (tokens as f64 / 1000.0) * price,
        is_free: price == 0.0,
    }
}

fn parse_param_count(parameter_size: &str) -> f64 {
    parameter_size.trim_end_matches(['B', 'b']).parse().unwrap_or(1.0)
}

/// Pure scoring: no I/O, fully testable. "speed" goal prefers benchmarked TPS
/// (falling back to smaller parameter count as a proxy when unbenchmarked);
/// "quality" goal prefers larger parameter count as a proxy for capability
/// (no quality benchmark exists yet - this is an honest heuristic, not a
/// real quality score). Returns candidates sorted best-first.
pub fn score_models(
    models: &[OllamaModel],
    benchmarks: &HashMap<String, BenchmarkResult>,
    prefs: &Preferences,
) -> Vec<(f64, String, String)> {
    let mut scored: Vec<(f64, String, String)> = models
        .iter()
        .map(|m| {
            let benchmark = benchmarks.get(&m.name);
            let (score, reason) = if prefs.goal == "speed" {
                match benchmark.and_then(|b| b.tokens_per_second) {
                    Some(tps) => (tps, format!("fastest benchmarked model ({tps:.1} tok/s)")),
                    None => {
                        let params = parse_param_count(&m.parameter_size);
                        (1.0 / params.max(0.1), format!("smallest available model (~{} params, not yet benchmarked)", m.parameter_size))
                    }
                }
            } else {
                let params = parse_param_count(&m.parameter_size);
                (params, format!("largest available model (~{} params) for best quality", m.parameter_size))
            };
            (score, m.name.clone(), reason)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

#[derive(Serialize, Clone)]
pub struct AutoSelection {
    pub provider: String,
    pub model: String,
    pub reason: String,
    pub estimated_cost_usd: f64,
    pub is_free: bool,
}

/// Combines model scoring with provider health and cost into a single
/// selection + human-readable reason. Only Ollama candidates exist today
/// (see price_per_1k_tokens doc comment), but the combination logic itself
/// (score -> health check -> cost annotate -> reason string) is what a
/// second real provider would plug into.
pub fn select_model(
    models: &[OllamaModel],
    benchmarks: &HashMap<String, BenchmarkResult>,
    health: &HealthRegistry,
    prefs: &Preferences,
    prompt: &str,
) -> AppResult<AutoSelection> {
    if models.is_empty() {
        return Err(AppError::Provider("no local models available to select from".to_string()));
    }

    let scored = score_models(models, benchmarks, prefs);
    let (_, model, reason) = scored.into_iter().next().unwrap();

    let cost = estimate_cost(&ProviderId::Ollama, prompt);
    let health_note = if health.is_healthy("ollama") {
        String::new()
    } else {
        " (warning: ollama is currently degraded after repeated failures)".to_string()
    };

    Ok(AutoSelection {
        provider: "Ollama".to_string(),
        model,
        reason: format!("{reason} - {} goal, {} cost preference, local/free{health_note}", prefs.goal, prefs.cost_preference),
        estimated_cost_usd: cost.estimated_cost_usd,
        is_free: cost.is_free,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(name: &str, params: &str) -> OllamaModel {
        OllamaModel {
            name: name.to_string(),
            size_bytes: 0,
            parameter_size: params.to_string(),
            quantization_level: "Q4_0".to_string(),
            context_length: 4096,
            family: "test".to_string(),
        }
    }

    fn benchmark(name: &str, tps: f64) -> BenchmarkResult {
        BenchmarkResult {
            model: name.to_string(),
            tokens_per_second: Some(tps),
            latency_ms: 100.0,
            vram_required_mb: 1000,
            reliable: true,
            benchmarked_at: 0,
        }
    }

    #[test]
    fn speed_goal_prefers_higher_benchmarked_tps() {
        let models = vec![model("slow-model", "7B"), model("fast-model", "1B")];
        let mut benchmarks = HashMap::new();
        benchmarks.insert("slow-model".to_string(), benchmark("slow-model", 10.0));
        benchmarks.insert("fast-model".to_string(), benchmark("fast-model", 80.0));

        let prefs = Preferences { goal: "speed".to_string(), cost_preference: "free".to_string() };
        let scored = score_models(&models, &benchmarks, &prefs);
        assert_eq!(scored[0].1, "fast-model");
    }

    #[test]
    fn speed_goal_without_benchmarks_prefers_smaller_model() {
        let models = vec![model("big", "70B"), model("small", "1B")];
        let prefs = Preferences { goal: "speed".to_string(), cost_preference: "free".to_string() };
        let scored = score_models(&models, &HashMap::new(), &prefs);
        assert_eq!(scored[0].1, "small");
    }

    #[test]
    fn quality_goal_prefers_larger_model() {
        let models = vec![model("small", "1B"), model("big", "70B")];
        let prefs = Preferences { goal: "quality".to_string(), cost_preference: "quality_first".to_string() };
        let scored = score_models(&models, &HashMap::new(), &prefs);
        assert_eq!(scored[0].1, "big");
    }

    #[test]
    fn select_model_errors_on_empty_model_list() {
        let prefs = Preferences::default();
        let health = HealthRegistry::default();
        let result = select_model(&[], &HashMap::new(), &health, &prefs, "hello");
        assert!(result.is_err());
    }

    #[test]
    fn select_model_reports_zero_cost_for_local_ollama() {
        let models = vec![model("m1", "1B")];
        let prefs = Preferences::default();
        let health = HealthRegistry::default();
        let selection = select_model(&models, &HashMap::new(), &health, &prefs, "hello world").unwrap();
        assert!(selection.is_free);
        assert_eq!(selection.estimated_cost_usd, 0.0);
        assert_eq!(selection.model, "m1");
    }

    #[test]
    fn estimate_cost_scales_with_prompt_length_for_paid_provider() {
        let short = estimate_cost(&ProviderId::OpenAi, "hi");
        let long = estimate_cost(&ProviderId::OpenAi, &"word ".repeat(1000));
        assert!(long.estimated_cost_usd > short.estimated_cost_usd);
        assert!(!short.is_free);
    }

    #[test]
    fn preferences_roundtrip_through_settings_table() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_router_settings_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();

        let conn = crate::database::open_for_workspace(&dir).unwrap();
        assert_eq!(load_preferences(&conn).goal, "speed"); // default before any save

        let prefs = Preferences { goal: "quality".to_string(), cost_preference: "quality_first".to_string() };
        save_preferences(&conn, &prefs).unwrap();
        let loaded = load_preferences(&conn);
        assert_eq!(loaded.goal, "quality");
        assert_eq!(loaded.cost_preference, "quality_first");

        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
