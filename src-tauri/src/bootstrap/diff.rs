/// Cap on rows*cols for the LCS table - past this a file is large enough
/// that a full O(n*m) diff isn't worth the memory, so we fall back to a
/// summary rather than hang or blow up on something the LLM's suggestion
/// pass happened to point at.
const MAX_DIFF_CELLS: usize = 4_000_000;

enum DiffLine<'a> {
    Context(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// Classic LCS-based line diff via dynamic programming, backtracked into a
/// sequence of context/removed/added lines. No context-window collapsing
/// (unlike real unified diff format) since these are whole small source
/// files, not multi-thousand-line blobs - showing every line is more
/// readable for a human reviewing a self-proposed change than hunks would be.
fn lcs_diff<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<DiffLine<'a>> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(DiffLine::Context(a[i]));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(DiffLine::Removed(a[i]));
            i += 1;
        } else {
            out.push(DiffLine::Added(b[j]));
            j += 1;
        }
    }
    while i < n {
        out.push(DiffLine::Removed(a[i]));
        i += 1;
    }
    while j < m {
        out.push(DiffLine::Added(b[j]));
        j += 1;
    }
    out
}

pub fn unified_diff(path: &str, original: &str, proposed: &str) -> String {
    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = proposed.lines().collect();

    let mut out = format!("--- a/{path}\n+++ b/{path}\n");

    if a.len().saturating_mul(b.len()) > MAX_DIFF_CELLS {
        out.push_str(&format!("(diff omitted: file too large for the built-in differ - {} -> {} lines)\n", a.len(), b.len()));
        return out;
    }

    for line in lcs_diff(&a, &b) {
        match line {
            DiffLine::Context(l) => out.push_str(&format!("  {l}\n")),
            DiffLine::Removed(l) => out.push_str(&format!("- {l}\n")),
            DiffLine::Added(l) => out.push_str(&format!("+ {l}\n")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_produces_only_context_lines() {
        let content = "fn main() {\n    println!(\"hi\");\n}\n";
        let diff = unified_diff("main.rs", content, content);
        let body_lines: Vec<&str> = diff.lines().skip(2).collect();
        assert!(body_lines.iter().all(|l| l.starts_with("  ")), "unexpected non-context line in: {diff}");
    }

    #[test]
    fn single_line_change_shows_one_removal_and_one_addition() {
        let original = "line one\nline two\nline three\n";
        let proposed = "line one\nline TWO\nline three\n";
        let diff = unified_diff("f.txt", original, proposed);
        assert!(diff.contains("- line two"));
        assert!(diff.contains("+ line TWO"));
        assert!(diff.contains("  line one"));
        assert!(diff.contains("  line three"));
    }

    #[test]
    fn oversized_input_falls_back_to_summary_instead_of_full_diff() {
        let big_a = "x\n".repeat(3000);
        let big_b = "y\n".repeat(3000);
        let diff = unified_diff("big.rs", &big_a, &big_b);
        assert!(diff.contains("diff omitted"));
    }
}
