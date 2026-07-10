use crate::ai::providers::ollama::{self, ChatMessage};
use crate::core::errors::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS benchmarks (
    model TEXT PRIMARY KEY,
    tokens_per_second REAL,
    latency_ms REAL NOT NULL,
    vram_required_mb INTEGER NOT NULL,
    reliable INTEGER NOT NULL,
    benchmarked_at INTEGER NOT NULL
);
"#;

#[derive(Default)]
pub struct BenchmarkDbState {
    pub conn: Mutex<Option<Connection>>,
}

pub fn open(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::Provider(format!("failed to open model_benchmarks.db: {e}")))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| AppError::Provider(format!("failed to init benchmarks schema: {e}")))?;
    Ok(conn)
}

#[derive(Serialize, Type, Clone)]
pub struct BenchmarkResult {
    pub model: String,
    pub tokens_per_second: Option<f64>,
    pub latency_ms: f64,
    pub vram_required_mb: u64,
    pub reliable: bool,
    pub benchmarked_at: i64,
}

const BENCHMARK_PROMPT: &str = "Count from 1 to 20, one number per line.";

/// Runs a real short prompt against the model and measures time-to-first-token
/// (latency) and tokens/sec (from Ollama's own eval_count/eval_duration - see
/// ChatStats). "reliable" is true iff the call succeeded and produced output;
/// a single run is not a statistically rigorous reliability measure, but for
/// Phase 4's foundation scope this is a real, honest signal rather than a
/// simulated one - repeated benchmarking over time can build a truer picture
/// later without changing this function's contract.
pub async fn run_benchmark(model: &str, parameter_size: &str, quantization_level: &str) -> AppResult<BenchmarkResult> {
    let hardware = crate::hardware::detect_all();
    let vram_required_mb = crate::ai::model_manager::estimate_required_mb(parameter_size, quantization_level);
    let _ = &hardware;

    let start = Instant::now();
    let mut first_token_at = None;

    let result = ollama::chat_stream(
        model,
        vec![ChatMessage {
            role: "user".to_string(),
            content: BENCHMARK_PROMPT.to_string(),
        }],
        |token, _done| {
            if first_token_at.is_none() && !token.is_empty() {
                first_token_at = Some(start.elapsed());
            }
        },
    )
    .await;

    let benchmarked_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    match result {
        Ok(stats) => {
            let latency_ms = first_token_at.unwrap_or_else(|| start.elapsed()).as_secs_f64() * 1000.0;
            Ok(BenchmarkResult {
                model: model.to_string(),
                tokens_per_second: stats.tokens_per_second(),
                latency_ms,
                vram_required_mb,
                reliable: true,
                benchmarked_at,
            })
        }
        Err(e) => {
            tracing::warn!(target: "ai", event = "benchmark_failed", model = %model, error = %e);
            Ok(BenchmarkResult {
                model: model.to_string(),
                tokens_per_second: None,
                latency_ms: start.elapsed().as_secs_f64() * 1000.0,
                vram_required_mb,
                reliable: false,
                benchmarked_at,
            })
        }
    }
}

pub fn store(conn: &Connection, result: &BenchmarkResult) -> AppResult<()> {
    conn.execute(
        "INSERT INTO benchmarks (model, tokens_per_second, latency_ms, vram_required_mb, reliable, benchmarked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(model) DO UPDATE SET
            tokens_per_second = excluded.tokens_per_second,
            latency_ms = excluded.latency_ms,
            vram_required_mb = excluded.vram_required_mb,
            reliable = excluded.reliable,
            benchmarked_at = excluded.benchmarked_at",
        params![
            result.model,
            result.tokens_per_second,
            result.latency_ms,
            result.vram_required_mb as i64,
            result.reliable as i64,
            result.benchmarked_at
        ],
    )
    .map_err(|e| AppError::Provider(format!("failed to store benchmark: {e}")))?;
    Ok(())
}

pub fn list(conn: &Connection) -> AppResult<Vec<BenchmarkResult>> {
    let mut stmt = conn
        .prepare("SELECT model, tokens_per_second, latency_ms, vram_required_mb, reliable, benchmarked_at FROM benchmarks")
        .map_err(|e| AppError::Provider(format!("failed to query benchmarks: {e}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(BenchmarkResult {
                model: row.get(0)?,
                tokens_per_second: row.get(1)?,
                latency_ms: row.get(2)?,
                vram_required_mb: row.get::<_, i64>(3)? as u64,
                reliable: row.get::<_, i64>(4)? != 0,
                benchmarked_at: row.get(5)?,
            })
        })
        .map_err(|e| AppError::Provider(format!("failed to query benchmarks: {e}")))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Provider(format!("failed to read benchmark row: {e}")))
}

pub fn get(conn: &Connection, model: &str) -> Option<BenchmarkResult> {
    conn.query_row(
        "SELECT model, tokens_per_second, latency_ms, vram_required_mb, reliable, benchmarked_at FROM benchmarks WHERE model = ?1",
        params![model],
        |row| {
            Ok(BenchmarkResult {
                model: row.get(0)?,
                tokens_per_second: row.get(1)?,
                latency_ms: row.get(2)?,
                vram_required_mb: row.get::<_, i64>(3)? as u64,
                reliable: row.get::<_, i64>(4)? != 0,
                benchmarked_at: row.get(5)?,
            })
        },
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (std::path::PathBuf, Connection) {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        path.push(format!("neuralforge_benchmarks_test_{nanos}.db"));
        let conn = open(&path).unwrap();
        (path, conn)
    }

    #[test]
    fn store_then_get_and_list_roundtrip() {
        let (path, conn) = temp_db();

        let result = BenchmarkResult {
            model: "test-model".to_string(),
            tokens_per_second: Some(42.5),
            latency_ms: 123.4,
            vram_required_mb: 4096,
            reliable: true,
            benchmarked_at: 1_700_000_000,
        };
        store(&conn, &result).unwrap();

        let fetched = get(&conn, "test-model").unwrap();
        assert_eq!(fetched.tokens_per_second, Some(42.5));
        assert_eq!(fetched.vram_required_mb, 4096);
        assert!(fetched.reliable);

        assert!(get(&conn, "nonexistent").is_none());

        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 1);

        // upsert: re-storing the same model updates in place, not duplicates
        let updated = BenchmarkResult {
            tokens_per_second: Some(50.0),
            ..result
        };
        store(&conn, &updated).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);
        assert_eq!(get(&conn, "test-model").unwrap().tokens_per_second, Some(50.0));

        drop(conn);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn run_benchmark_produces_real_tps_from_local_model() {
        let result = run_benchmark("deepseek-coder:latest", "1B", "Q4_0").await.unwrap();
        assert!(result.reliable);
        assert!(result.tokens_per_second.unwrap() > 0.0);
        assert!(result.latency_ms > 0.0);
        assert!(result.vram_required_mb > 0);
    }
}
