use crate::ai::providers::ollama::{self, ChatMessage};
use crate::core::errors::{AppError, AppResult};

/// Rough risk signal: how much of the file actually changes. Not a
/// sophisticated diff (no move/rename detection) - counting lines
/// added/removed is enough to distinguish "one-line tweak" from "rewrote
/// the whole file" for a human approval prompt, which is the only thing
/// this needs to support at Phase 5 foundation scope.
pub fn estimate_risk(original: &str, proposed: &str) -> String {
    let original_lines: Vec<&str> = original.lines().collect();
    let proposed_lines: Vec<&str> = proposed.lines().collect();

    let original_set: std::collections::HashSet<&str> = original_lines.iter().copied().collect();
    let proposed_set: std::collections::HashSet<&str> = proposed_lines.iter().copied().collect();

    let removed = original_lines.iter().filter(|l| !proposed_set.contains(*l)).count();
    let added = proposed_lines.iter().filter(|l| !original_set.contains(*l)).count();
    let total = original_lines.len().max(1);
    let changed_ratio = (added + removed) as f64 / total as f64;

    let level = if changed_ratio > 0.6 {
        "high"
    } else if changed_ratio > 0.2 {
        "medium"
    } else {
        "low"
    };

    format!("{level} risk: +{added}/-{removed} lines out of {total} original lines")
}

fn build_prompt(objective: &str, file_path: &str, current_content: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: "You are a careful coding assistant. You will be given a file's full \
                current content and an objective. Respond with ONLY the complete new file \
                content after making the requested change - no explanation, no markdown code \
                fences, no commentary before or after. If the objective is unclear or unsafe, \
                respond with the original content unchanged."
                .to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: format!(
                "File: {file_path}\nObjective: {objective}\n\nCurrent content:\n{current_content}"
            ),
        },
    ]
}

/// Simulation Mode: proposes a new version of the file's content without
/// writing anything to disk. The caller (a Tauri command) is responsible
/// for persisting the proposal and surfacing it for human approval - this
/// function has no side effects at all beyond the read-only LLM call.
pub async fn plan_change(objective: &str, file_path: &str, current_content: &str) -> AppResult<(String, String)> {
    let messages = build_prompt(objective, file_path, current_content);
    let mut proposed = String::new();

    ollama::chat_stream("deepseek-coder:latest", messages, |token, _done| {
        proposed.push_str(token);
    })
    .await
    .map_err(|e| AppError::Provider(format!("planning failed: {e}")))?;

    let proposed = proposed.trim().to_string();
    if proposed.is_empty() {
        return Err(AppError::Provider("model produced an empty proposal".to_string()));
    }

    let risk = estimate_risk(current_content, &proposed);
    Ok((proposed, risk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_risk_reports_low_for_small_change() {
        let original = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut lines: Vec<String> = (1..=20).map(|i| format!("line {i}")).collect();
        lines.push("// new comment".to_string());
        let proposed = lines.join("\n");

        let risk = estimate_risk(&original, &proposed);
        assert!(risk.starts_with("low risk"), "expected low risk, got: {risk}");
    }

    #[test]
    fn estimate_risk_reports_high_for_full_rewrite() {
        let original = "fn old() {}\n";
        let proposed = "completely different content\nwith multiple new lines\nand nothing shared\n";
        let risk = estimate_risk(original, proposed);
        assert!(risk.starts_with("high risk"), "expected high risk, got: {risk}");
    }

    #[test]
    fn estimate_risk_reports_zero_for_identical_content() {
        let content = "fn main() {}\n";
        let risk = estimate_risk(content, content);
        assert!(risk.contains("+0/-0"), "expected no changes, got: {risk}");
    }

    #[tokio::test]
    #[ignore = "requires a running local Ollama instance"]
    async fn plan_change_proposes_real_content_from_local_model() {
        let original = "fn main() {\n    println!(\"hello\");\n}\n";
        let (proposed, risk) = plan_change(
            "Add a one-line comment above the println! call explaining what it does",
            "main.rs",
            original,
        )
        .await
        .unwrap();

        assert!(!proposed.trim().is_empty());
        assert!(!risk.is_empty());
    }
}
