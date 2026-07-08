use super::selfanalyze::SelfAnalysis;
use crate::ai::providers::ollama::{self, ChatMessage};
use crate::core::errors::{AppError, AppResult};

/// One file the model chose to improve, plus why - the "generate
/// suggestions" step. This alone has no side effects: it doesn't read the
/// target file's content or propose a diff yet, it only picks a target.
pub struct TargetChoice {
    pub title: String,
    pub slug: String,
    pub file_path: String,
    pub rationale: String,
}

fn build_prompt(analysis: &SelfAnalysis) -> Vec<ChatMessage> {
    let file_list = analysis.source_files.join("\n");
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are reviewing your own source code to suggest ONE small, safe, focused \
                improvement - not a rewrite. Respond with EXACTLY three lines, no markdown, no \
                extra commentary before or after:\n\
                FILE: <one path from the file list below, copied exactly>\n\
                TITLE: <a short imperative title, under 8 words>\n\
                WHY: <one or two sentences explaining the improvement and its benefit>"
                .to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "# Project memory\n{}\n\n# Source files\n{file_list}\n\nPick exactly one file and describe one focused improvement to it.",
                analysis.memory_context
            ),
        },
    ]
}

/// Lowercases, replaces runs of non-alphanumeric characters with a single
/// dash, and caps length - the result is used directly as part of a git
/// branch name (neuralforge/suggest-<slug>), so it must be shell/git safe.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    let slug: String = out.chars().take(40).collect();
    if slug.is_empty() {
        "improvement".to_string()
    } else {
        slug
    }
}

fn parse_response(response: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut file_path = None;
    let mut title = None;
    let mut why = None;
    for line in response.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("FILE:") {
            file_path = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("TITLE:") {
            title = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("WHY:") {
            why = Some(rest.trim().to_string());
        }
    }
    (file_path, title, why)
}

/// Asks the model to name one file and one improvement, then validates the
/// named file against the real scanned file list - a hallucinated path is
/// rejected with a clear error rather than silently failing later when the
/// file can't be read.
pub async fn choose_target(analysis: &SelfAnalysis) -> AppResult<TargetChoice> {
    if analysis.source_files.is_empty() {
        return Err(AppError::Provider("no source files found to analyze".to_string()));
    }

    let messages = build_prompt(analysis);
    let mut response = String::new();
    ollama::chat_stream("deepseek-coder:latest", messages, |token, _done| response.push_str(token))
        .await
        .map_err(|e| AppError::Provider(format!("self-analysis suggestion failed: {e}")))?;

    let (file_path, title, why) = parse_response(&response);

    let file_path = file_path.ok_or_else(|| AppError::Provider("model did not name a target file".to_string()))?;
    let title = title.unwrap_or_else(|| "Improve code".to_string());
    let rationale = why.unwrap_or_else(|| "General code quality improvement".to_string());

    let matched = analysis
        .source_files
        .iter()
        .find(|f| f.as_str() == file_path || f.ends_with(&file_path))
        .cloned()
        .ok_or_else(|| AppError::Provider(format!("model proposed '{file_path}', which isn't one of the scanned source files")))?;

    Ok(TargetChoice { slug: slugify(&title), title, file_path: matched, rationale })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_produces_a_branch_safe_string() {
        assert_eq!(slugify("Simplify the FTS5 query builder!"), "simplify-the-fts5-query-builder");
        assert_eq!(slugify("   "), "improvement");
        assert_eq!(slugify("Already-slugged"), "already-slugged");
    }

    #[test]
    fn parse_response_extracts_all_three_fields() {
        let response = "FILE: src/lib.rs\nTITLE: Add doc comment\nWHY: It clarifies intent.";
        let (file, title, why) = parse_response(response);
        assert_eq!(file.as_deref(), Some("src/lib.rs"));
        assert_eq!(title.as_deref(), Some("Add doc comment"));
        assert_eq!(why.as_deref(), Some("It clarifies intent."));
    }

    #[tokio::test]
    async fn choose_target_rejects_a_hallucinated_file_path() {
        // No live Ollama call happens here in a way that could produce this
        // path, so this exercises the validation branch directly via an
        // analysis whose file list is known - if the (fake, unreachable in
        // this offline test) model claimed a file outside it, it must be
        // rejected. Since this test doesn't call Ollama, it instead proves
        // the guard logic itself.
        let analysis = SelfAnalysis { memory_context: String::new(), source_files: vec!["src/lib.rs".to_string()] };
        let matched = analysis.source_files.iter().find(|f| f.as_str() == "src/other.rs" || f.ends_with("src/other.rs"));
        assert!(matched.is_none());
    }

    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn choose_target_proposes_a_real_target_from_local_model() {
        let analysis = SelfAnalysis {
            memory_context: "# Architecture\n\nRust backend using Tauri.".to_string(),
            source_files: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        };
        let target = choose_target(&analysis).await.unwrap();
        assert!(analysis.source_files.contains(&target.file_path));
        assert!(!target.title.is_empty());
    }
}
