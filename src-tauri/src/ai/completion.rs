use serde::Serialize;
use specta::Type;

/// The isolated text boundaries around the cursor position, ready for
/// localized low-latency completion pipelines.
#[derive(Serialize, Type, Clone, Debug)]
pub struct PredictionContext {
    /// Lines above the cursor (prefix window, up to 50 lines).
    pub prefix: String,
    /// Lines below the cursor (suffix window, up to 20 lines).
    pub suffix: String,
    /// Full file path for context.
    pub file_path: String,
    /// The line on which the cursor sits.
    pub cursor_line: usize,
    /// The column within the cursor line.
    pub cursor_column: usize,
    /// The cursor line content itself, for inline completion.
    pub cursor_line_content: String,
}

/// Extracts a localized window around the cursor position from file content.
/// Slices exactly 50 lines above (prefix) and 20 lines below (suffix) the
/// cursor line, so the completion engine never parses the full codebase file.
pub fn extract_prediction_window(
    file_path: String,
    content: String,
    cursor_line: usize,
    cursor_column: usize,
) -> PredictionContext {
    const PREFIX_LINES: usize = 50;
    const SUFFIX_LINES: usize = 20;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Clamp cursor_line to valid range (0-indexed)
    let cursor = cursor_line.min(total_lines.saturating_sub(1));
    let cursor_content = lines.get(cursor).unwrap_or(&"").to_string();

    // Prefix: up to 50 lines above the cursor
    let prefix_start = cursor.saturating_sub(PREFIX_LINES);
    let prefix: String = lines[prefix_start..cursor].join("\n");

    // Suffix: up to 20 lines below the cursor (avoid empty range if cursor is at end)
    let suffix_start = (cursor + 1).min(total_lines);
    let suffix_end = (suffix_start + SUFFIX_LINES).min(total_lines);
    let suffix: String = if suffix_start < suffix_end {
        lines[suffix_start..suffix_end].join("\n")
    } else {
        String::new()
    };

    PredictionContext {
        prefix,
        suffix,
        file_path,
        cursor_line: cursor,
        cursor_column,
        cursor_line_content: cursor_content,
    }
}

#[tauri::command]
pub fn get_ghost_text_prediction(
    file_path: String,
    content: String,
    cursor_line: usize,
    cursor_column: usize,
) -> PredictionContext {
    extract_prediction_window(file_path, content, cursor_line, cursor_column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_window_basic() {
        let content = (0..200).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let ctx = extract_prediction_window("test.rs".into(), content, 100, 5);
        // Prefix: lines 50..100 = 50 lines
        assert_eq!(ctx.prefix.lines().count(), 50, "should have 50 prefix lines");
        // Suffix: lines 101..121 = 20 lines
        assert_eq!(ctx.suffix.lines().count(), 20, "should have 20 suffix lines");
        assert_eq!(ctx.cursor_line, 100);
        assert_eq!(ctx.cursor_column, 5);
        assert!(ctx.cursor_line_content.contains("line 100"));
    }

    #[test]
    fn extract_window_at_file_start() {
        let content = (0..10).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let ctx = extract_prediction_window("test.rs".into(), content, 0, 0);
        assert_eq!(ctx.prefix.lines().count(), 0, "no prefix lines at file start");
        assert_eq!(ctx.cursor_line, 0);
        assert_eq!(ctx.cursor_line_content, "line 0");
    }

    #[test]
    fn extract_window_at_file_end() {
        let content = (0..30).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        let ctx = extract_prediction_window("test.rs".into(), content, 29, 3);
        assert_eq!(ctx.suffix.lines().count(), 0, "no suffix lines at file end");
        assert_eq!(ctx.cursor_line, 29);
    }

    #[test]
    fn extract_window_clamps_out_of_bounds() {
        let content = "only one line".to_string();
        let ctx = extract_prediction_window("test.rs".into(), content, 999, 0);
        assert_eq!(ctx.cursor_line, 0);
        assert_eq!(ctx.cursor_line_content, "only one line");
    }

    #[test]
    fn extract_window_empty_content() {
        let ctx = extract_prediction_window("empty.rs".into(), String::new(), 0, 0);
        assert_eq!(ctx.prefix, "");
        assert_eq!(ctx.suffix, "");
        assert_eq!(ctx.cursor_line_content, "");
    }
}