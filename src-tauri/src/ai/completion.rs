use once_cell::sync::Lazy;
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const DEBOUNCE_MS: u64 = 75;
static LAST_REQUEST_MS: AtomicU64 = AtomicU64::new(0);
pub fn should_debounce() -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let last = LAST_REQUEST_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < DEBOUNCE_MS { return true; }
    LAST_REQUEST_MS.store(now, Ordering::Relaxed); false
}
pub fn reset_debounce() { LAST_REQUEST_MS.store(0, Ordering::Relaxed); }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub enum FimTemplate { StarCoder, CodeLlama }
#[derive(Clone, Debug, Serialize, Type)]
pub struct FimPrompt { pub prefix: String, pub suffix: String, pub template: FimTemplate }
impl FimPrompt {
    pub fn format(&self) -> String {
        match self.template {
            FimTemplate::StarCoder => format!("<fim-prefix>{}<fim-suffix>{}<fim-middle>", self.prefix, self.suffix),
            FimTemplate::CodeLlama => format!("<PRE> {} <SUF> {} <MID>", self.prefix, self.suffix),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CacheEntry { pub file_path: String, pub suffix: String, pub completion: String }
pub static PREDICTION_CACHE: Lazy<Mutex<Option<CacheEntry>>> = Lazy::new(|| Mutex::new(None));
pub fn cache_prediction(file_path: &str, suffix: &str, completion: &str) {
    if let Ok(mut guard) = PREDICTION_CACHE.lock() {
        *guard = Some(CacheEntry { file_path: file_path.to_string(), suffix: suffix.to_string(), completion: completion.to_string() });
    }
}
pub fn check_prediction_cache(file_path: &str, new_suffix: &str) -> Option<(String, bool, String)> {
    let guard = PREDICTION_CACHE.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.file_path != file_path { return None; }
    if entry.suffix.starts_with(new_suffix) && new_suffix.len() < entry.suffix.len() {
        let consumed = new_suffix.len();
        let remaining = &entry.completion[consumed.min(entry.completion.len())..];
        return Some((remaining.to_string(), true, entry.suffix[consumed..].to_string()));
    }
    None
}

#[derive(Serialize, Type, Clone, Debug)]
pub struct PredictionContext { pub prefix: String, pub suffix: String, pub file_path: String, pub cursor_line: usize, pub cursor_column: usize, pub cursor_line_content: String }
#[derive(Serialize, Type, Clone, Debug)]
pub struct PredictionResponse { pub context: PredictionContext, pub fim_prompt: String, pub cached: bool, pub completion: String, pub remaining_suffix: String }
#[derive(Serialize, Clone, Debug)]
pub struct GhostTextStreamPayload { pub token: String, pub done: bool, pub request_id: String }

pub fn extract_prediction_window(file_path: String, content: String, cursor_line: usize, cursor_column: usize) -> PredictionContext {
    const PL: usize = 50; const SL: usize = 20;
    let lines: Vec<&str> = content.lines().collect(); let total = lines.len();
    let cursor = cursor_line.min(total.saturating_sub(1));
    let cc = lines.get(cursor).unwrap_or(&"").to_string();
    let prefix: String = lines[cursor.saturating_sub(PL)..cursor].join("\n");
    let ss = (cursor + 1).min(total); let se = (ss + SL).min(total);
    let suffix: String = if ss < se { lines[ss..se].join("\n") } else { String::new() };
    PredictionContext { prefix, suffix, file_path, cursor_line: cursor, cursor_column, cursor_line_content: cc }
}
pub fn build_fim_prompt(ctx: &PredictionContext, template: FimTemplate) -> FimPrompt {
    FimPrompt { prefix: ctx.prefix.clone(), suffix: ctx.suffix.clone(), template }
}

pub fn run_prediction_pipeline(file_path: String, content: String, cursor_line: usize, cursor_column: usize, template: FimTemplate) -> PredictionResponse {
    let ctx = extract_prediction_window(file_path.clone(), content, cursor_line, cursor_column);
    let suffix = ctx.suffix.clone();
    if let Some((completion, cached, remaining)) = check_prediction_cache(&file_path, &suffix) {
        if cached { reset_debounce(); return PredictionResponse { fim_prompt: String::new(), context: ctx.clone(), cached: true, completion, remaining_suffix: remaining }; }
    }
    let formatted = build_fim_prompt(&ctx, template).format();
    PredictionResponse { fim_prompt: formatted, context: ctx, cached: false, completion: String::new(), remaining_suffix: suffix }
}

// ── Streaming Diff Engine ─────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
pub enum DiffOp { Insert, Delete, Equal }

#[derive(Clone, Debug, Serialize, Type)]
pub struct DiffLinePayload {
    pub op: DiffOp,
    pub line: String,
    pub line_index: usize,
    pub request_id: String,
}

/// Compares original code and generated text line-by-line, emitting structured
/// `DiffLinePayload` records via the `"inline-diff-stream"` Tauri event.
pub async fn stream_inline_diff(
    app: AppHandle,
    request_id: String,
    original_code: &str,
    generated_text: &str,
) {
    let orig_lines: Vec<&str> = original_code.lines().collect();
    let gen_lines: Vec<&str> = generated_text.lines().collect();
    let mut gen_used: Vec<bool> = vec![false; gen_lines.len()];
    let mut diff_ops: Vec<DiffLinePayload> = Vec::new();

    for (i, orig_line) in orig_lines.iter().enumerate() {
        let matched = gen_lines.iter().enumerate().skip(i).find(|(_, gl)| *gl == orig_line);
        if let Some((j, _)) = matched {
            gen_used[j] = true;
            diff_ops.push(DiffLinePayload { op: DiffOp::Equal, line: orig_line.to_string(), line_index: i, request_id: request_id.clone() });
        } else {
            diff_ops.push(DiffLinePayload { op: DiffOp::Delete, line: orig_line.to_string(), line_index: i, request_id: request_id.clone() });
        }
    }
    for (j, gen_line) in gen_lines.iter().enumerate() {
        if !gen_used[j] {
            diff_ops.push(DiffLinePayload { op: DiffOp::Insert, line: gen_line.to_string(), line_index: orig_lines.len() + j, request_id: request_id.clone() });
        }
    }
    for op in &diff_ops {
        let _ = app.emit("inline-diff-stream", op);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let _ = app.emit("inline-diff-stream", &DiffLinePayload { op: DiffOp::Equal, line: String::new(), line_index: usize::MAX, request_id });
}

// ── Core Prediction Pipeline ──────────────────────────────────────────────

pub async fn async_stream_completion(app: AppHandle, request_id: String, file_path: String, content: String, cursor_line: usize, cursor_column: usize, _template: FimTemplate) {
    if should_debounce() { return; }
    let ctx = extract_prediction_window(file_path, content, cursor_line, cursor_column);
    let fp = ctx.file_path.clone(); let sfx = ctx.suffix.clone();
    let pl = ctx.prefix.lines().last().unwrap_or("").to_string(); let sl = sfx.lines().next().unwrap_or("").to_string();
    drop(ctx);
    if let Some((completion, _, _)) = check_prediction_cache(&fp, &sfx) {
        let _ = app.emit("ghost-text-stream", GhostTextStreamPayload { token: completion, done: true, request_id: request_id.clone() }); return;
    }
    let _ = app.emit("ghost-text-stream", GhostTextStreamPayload { token: "[waiting...]".into(), done: false, request_id: request_id.clone() });
    let resp = format!("// for:\n// prefix: {} ... suffix: {}\nfn r() -> i32 {{ 42 }}", pl, sl);
    for ch in resp.chars() {
        let _ = app.emit("ghost-text-stream", GhostTextStreamPayload { token: ch.to_string(), done: false, request_id: request_id.clone() });
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let _ = app.emit("ghost-text-stream", GhostTextStreamPayload { token: String::new(), done: true, request_id });
    cache_prediction(&fp, &sfx, &resp);
}

#[tauri::command]
pub fn get_ghost_text_prediction(file_path: String, content: String, cursor_line: usize, cursor_column: usize) -> PredictionContext {
    extract_prediction_window(file_path, content, cursor_line, cursor_column)
}
#[tauri::command]
pub fn get_prediction_with_fim(file_path: String, content: String, cursor_line: usize, cursor_column: usize, template: String) -> PredictionResponse {
    let t = match template.to_lowercase().as_str() { "codellama" => FimTemplate::CodeLlama, _ => FimTemplate::StarCoder };
    run_prediction_pipeline(file_path, content, cursor_line, cursor_column, t)
}
#[tauri::command]
pub fn store_prediction_result(file_path: String, suffix: String, completion: String) { cache_prediction(&file_path, &suffix, &completion); }
#[tauri::command]
pub async fn request_async_completion(app: AppHandle, request_id: String, file_path: String, content: String, cursor_line: usize, cursor_column: usize, template: String) {
    let t = match template.to_lowercase().as_str() { "codellama" => FimTemplate::CodeLlama, _ => FimTemplate::StarCoder };
    async_stream_completion(app, request_id, file_path, content, cursor_line, cursor_column, t).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn extract_window_basic() { let c = (0..200).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n"); let x = extract_prediction_window("t.rs".into(), c, 100, 5); assert_eq!(x.prefix.lines().count(), 50); assert_eq!(x.suffix.lines().count(), 20); }
    #[test] fn extract_window_start() { let x = extract_prediction_window("t.rs".into(), "a\nb".into(), 0, 0); assert_eq!(x.prefix.lines().count(), 0); }
    #[test] fn extract_window_end() { let x = extract_prediction_window("t.rs".into(), "a\nb".into(), 1, 0); assert_eq!(x.suffix.lines().count(), 0); }
    #[test] fn extract_window_empty() { let x = extract_prediction_window("e.rs".into(), String::new(), 0, 0); assert_eq!(x.prefix, ""); }
    #[test] fn fim_starcoder() { let p = FimPrompt { prefix: "fn a(".into(), suffix: " -> i32 {}".into(), template: FimTemplate::StarCoder }; let f = p.format(); assert!(f.contains("<fim-prefix>")); assert!(f.contains("<fim-suffix>")); }
    #[test] fn fim_codellama() { let p = FimPrompt { prefix: "def a():".into(), suffix: "  pass".into(), template: FimTemplate::CodeLlama }; let f = p.format(); assert!(f.contains("<PRE>")); }
    #[test] fn cache_miss_first() { PREDICTION_CACHE.lock().unwrap().take(); assert!(check_prediction_cache("t.rs", "x").is_none()); }
    #[test] fn cache_hit() { cache_prediction("t.rs", "abcdef", "ghijkl"); let r = check_prediction_cache("t.rs", "abc").unwrap(); assert!(r.1); assert_eq!(r.0, "jkl"); }
    #[test] fn cache_miss_diff_file() { cache_prediction("o.rs", "abc", "xyz"); assert!(check_prediction_cache("t.rs", "abc").is_none()); }
    #[test] fn pipeline_miss() { PREDICTION_CACHE.lock().unwrap().take(); let r = run_prediction_pipeline("t.rs".into(), "a\nb\nc".into(), 1, 0, FimTemplate::StarCoder); assert!(!r.cached); assert!(!r.fim_prompt.is_empty()); }
    #[test] fn pipeline_hit() { cache_prediction("t.rs", "c", "ompletion"); let r = run_prediction_pipeline("t.rs".into(), "a\nb\nc".into(), 2, 0, FimTemplate::StarCoder); assert!(r.cached); assert_eq!(r.completion, "ompletion"); }
    #[test] fn debounce_pass() { reset_debounce(); assert!(!should_debounce()); }
    #[test] fn debounce_block() { reset_debounce(); should_debounce(); assert!(should_debounce()); reset_debounce(); }
}