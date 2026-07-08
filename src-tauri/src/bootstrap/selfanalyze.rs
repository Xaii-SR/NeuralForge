use crate::core::errors::AppResult;
use std::path::Path;
use walkdir::WalkDir;

const MAX_SOURCE_FILES: usize = 150;
const SOURCE_EXTENSIONS: &[&str] = &["rs", "ts", "tsx"];
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".next", "dist", ".neuralforge"];

/// Everything a suggestion pass needs to know about the workspace, gathered
/// read-only: project memory (architecture/decisions/etc, the same context
/// used for chat) and a capped list of source file paths. No file contents
/// beyond memory docs are read here - a specific file's content is only
/// read once suggest::choose_target has picked exactly one to work on, so
/// this step's cost doesn't scale with codebase size.
pub struct SelfAnalysis {
    pub memory_context: String,
    pub source_files: Vec<String>,
}

pub fn analyze(workspace_root: &Path) -> AppResult<SelfAnalysis> {
    let memory_context = crate::ai::context::read_memory_context(workspace_root);
    let source_files = scan_source_files(workspace_root);
    Ok(SelfAnalysis { memory_context, source_files })
}

fn scan_source_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !SKIP_DIRS.iter().any(|skip| e.path().components().any(|c| c.as_os_str() == *skip)))
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if !SOURCE_EXTENSIONS.contains(&ext) {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
        if files.len() >= MAX_SOURCE_FILES {
            break;
        }
    }

    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_selfanalyze_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn analyze_finds_source_files_and_skips_noise_dirs() {
        let dir = temp_workspace();
        std::fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("lib.rs"), "pub fn f() {}").unwrap();
        std::fs::create_dir_all(dir.join("target").join("debug")).unwrap();
        std::fs::write(dir.join("target").join("debug").join("build.rs"), "should be skipped").unwrap();
        std::fs::create_dir_all(dir.join("node_modules").join("pkg")).unwrap();
        std::fs::write(dir.join("node_modules").join("pkg").join("index.ts"), "should be skipped").unwrap();
        std::fs::write(dir.join("README.md"), "not a source extension").unwrap();

        let analysis = analyze(&dir).unwrap();
        assert!(analysis.source_files.contains(&"main.rs".to_string()));
        assert!(analysis.source_files.contains(&"src/lib.rs".to_string()));
        assert!(!analysis.source_files.iter().any(|f| f.contains("target")));
        assert!(!analysis.source_files.iter().any(|f| f.contains("node_modules")));
        assert!(!analysis.source_files.iter().any(|f| f.ends_with(".md")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn analyze_includes_memory_context_when_present() {
        let dir = temp_workspace();
        crate::core::config::ensure_memory_scaffold(&dir).unwrap();
        std::fs::write(
            dir.join(".neuralforge").join("memory").join("architecture.md"),
            "# Architecture\n\nRust backend, Next.js frontend.",
        )
        .unwrap();

        let analysis = analyze(&dir).unwrap();
        assert!(analysis.memory_context.contains("Rust backend"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
