use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Default patterns to always ignore.
const DEFAULT_IGNORES: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    "__pycache__",
    ".next",
    "out",
    ".turbo",
    "*.exe",
    "*.dll",
    "*.so",
    "*.dylib",
    "*.bin",
    "*.o",
    "*.obj",
    "*.class",
    "*.pyc",
    "*.pkg",
    "*.deb",
    "*.rpm",
    "*.zip",
    "*.tar.gz",
    "*.7z",
    "*.png",
    "*.jpg",
    "*.jpeg",
    "*.gif",
    "*.svg",
    "*.ico",
    "*.mp4",
    "*.mp3",
    "*.wav",
    "*.kn5",
    "*.dds",
    "*.pak",
    "*.tmp",
    "*.lock",
    "*.log",
    "Thumbs.db",
    ".DS_Store",
];

/// Maximum file size to scan in bytes (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Parses a `.gitignore`-style file and returns a set of patterns.
fn parse_ignore_file(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Checks whether a pattern matches a given path component.
fn pattern_matches(pattern: &str, name: &str) -> bool {
    // Exact match
    if pattern == name { return true; }
    // Glob match (simple: *.ext)
    if pattern.starts_with("*.") {
        let ext = &pattern[1..]; // ".ext"
        return name.ends_with(ext);
    }
    // Directory slash match
    if pattern.ends_with('/') {
        let dir = &pattern[..pattern.len()-1];
        return name == dir;
    }
    false
}

    /// Checks whether a relative path matches any ignore pattern.
fn is_ignored(relative: &Path, name: &str, patterns: &[String], base_ignores: &[&str]) -> bool {
    let rel_str = relative.to_string_lossy();
    let normalized = rel_str.replace('\\', "/");

    // Check all parent directories against directory-specific patterns (e.g. "secrets/")
    for ancestor in relative.ancestors().skip(1) {
        if let Some(a_name) = ancestor.file_name().and_then(|n| n.to_str()) {
            for p in patterns {
                if pattern_matches(p, a_name) { return true; }
            }
        }
    }

    // Check filename and full relative path
    for p in patterns {
        if pattern_matches(p, &normalized) || pattern_matches(p, name) {
            return true;
        }
    }

    for p in base_ignores {
        if pattern_matches(p, name) || pattern_matches(p, &normalized) { return true; }
    }

    false
}

/// Result of a workspace scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub files: Vec<PathBuf>,
    pub skipped_count: usize,
    pub total_bytes: u64,
}

/// Recursively scans a directory, respecting `.gitignore` and `.neuralforgeignore`.
pub fn scan_workspace(root: &Path) -> Result<ScanResult, String> {
    let mut files = Vec::new();
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;

    // Merge root-level ignore files
    let mut patterns: Vec<String> = Vec::new();
    for name in &[".gitignore", ".neuralforgeignore"] {
        let p = root.join(name);
        if p.exists() {
            patterns.extend(parse_ignore_file(&p));
        }
    }

    scan_dir(root, root, &patterns, &mut files, &mut skipped, &mut total_bytes)?;

    Ok(ScanResult { files, skipped_count: skipped, total_bytes })
}

fn scan_dir(
    root: &Path,
    dir: &Path,
    patterns: &[String],
    files: &mut Vec<PathBuf>,
    skipped: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        let relative = path.strip_prefix(root).unwrap_or(&path);

        // Skip ignore/config files
        if name == ".gitignore" || name == ".neuralforgeignore" {
            *skipped += 1;
            continue;
        }

        if is_ignored(relative, &name, patterns, DEFAULT_IGNORES) {
            *skipped += 1;
            continue;
        }

        if path.is_dir() {
            // Recurse, but skip symlink loops
            if path.is_symlink() {
                *skipped += 1;
                continue;
            }
            scan_dir(root, &path, patterns, files, skipped, total_bytes)?;
        } else {
            let meta = entry.metadata().map_err(|e| format!("Metadata error: {}", e))?;
            if meta.len() > MAX_FILE_SIZE {
                *skipped += 1;
                continue;
            }
            *total_bytes += meta.len();
            files.push(path.to_path_buf());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let mut d = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        d.push(format!("nf_scanner_test_{nanos}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ignores_node_modules() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::write(dir.join("src").join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("node_modules").join("pkg.js"), "export {}").unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].ends_with("main.rs"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn respects_gitignore() {
        let dir = temp_dir();
        fs::write(dir.join(".gitignore"), "secrets/\n*.log\n").unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join("secrets")).unwrap();
        fs::write(dir.join("src").join("lib.rs"), "pub fn a() {}").unwrap();
        fs::write(dir.join("secrets").join("key.txt"), "secret").unwrap();
        fs::write(dir.join("debug.log"), "log").unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].ends_with("lib.rs"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skips_binaries() {
        let dir = temp_dir();
        fs::write(dir.join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.join("app.exe"), [0x4D, 0x5A, 0x90].to_vec()).unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].ends_with("main.rs"));
        fs::remove_dir_all(&dir).unwrap();
    }
}