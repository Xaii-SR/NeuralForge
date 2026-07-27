use std::fs;
use std::path::{Path, PathBuf};
use ignore::gitignore::GitignoreBuilder;

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

fn is_sensitive_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name.as_str(),
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | ".git-credentials"
                | "credentials.json"
                | "service-account.json"
                | "service_account.json"
                | "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
        )
        || name.contains("credentials")
        || name.contains("service-account")
        || name.contains("service_account")
        || name.starts_with("secret.")
        || name.starts_with("secrets.")
        || matches!(extension.as_str(), "pem" | "key" | "p12" | "pfx")
}

fn has_sensitive_directory(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            ".ssh"
                | ".gnupg"
                | ".aws"
                | ".azure"
                | ".kube"
                | ".docker"
                | "secrets"
                | ".secrets"
                | "credentials"
        )
    })
}

fn matches_default_ignore(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    DEFAULT_IGNORES.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else {
            path.components()
                .any(|component| component.as_os_str() == *pattern)
        }
    })
}

pub struct WorkspacePathPolicy {
    root: PathBuf,
}

impl WorkspacePathPolicy {
    pub fn new(root: &Path) -> Result<Self, String> {
        let policy = Self {
            root: root.to_path_buf(),
        };
        policy.build_ignores_for(Path::new(""))?;
        Ok(policy)
    }

    fn build_ignores_for(&self, relative: &Path) -> Result<ignore::gitignore::Gitignore, String> {
        let mut builder = GitignoreBuilder::new(&self.root);
        let mut ignore_files = vec![self.root.join(".gitignore")];
        let mut parents = relative
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .filter(|parent| !parent.as_os_str().is_empty())
            .collect::<Vec<_>>();
        parents.reverse();
        ignore_files.extend(
            parents
                .into_iter()
                .map(|parent| self.root.join(parent).join(".gitignore")),
        );
        ignore_files.push(self.root.join(".neuralforgeignore"));

        for path in ignore_files {
            if path.exists() {
                if let Some(error) = builder.add(&path) {
                    return Err(format!("Failed to parse {}: {error}", path.display()));
                }
            }
        }
        builder
            .build()
            .map_err(|error| format!("Failed to build workspace ignore policy: {error}"))
    }

    pub fn is_excluded(&self, path: &Path, is_dir: bool) -> bool {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();

        if matches_default_ignore(relative) || has_sensitive_directory(relative) {
            return true;
        }
        if !is_dir && (name == ".gitignore" || name == ".neuralforgeignore" || is_sensitive_name(relative)) {
            return true;
        }
        self.build_ignores_for(relative)
            .map(|ignores| {
                ignores
                    .matched_path_or_any_parents(relative, is_dir)
                    .is_ignore()
            })
            .unwrap_or(true)
    }
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
    let policy = WorkspacePathPolicy::new(root)?;
    scan_dir(root, &policy, &mut files, &mut skipped, &mut total_bytes)?;

    Ok(ScanResult { files, skipped_count: skipped, total_bytes })
}

fn scan_dir(
    dir: &Path,
    policy: &WorkspacePathPolicy,
    files: &mut Vec<PathBuf>,
    skipped: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir {:?}: {}", dir, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let path = entry.path();

        if policy.is_excluded(&path, path.is_dir()) {
            *skipped += 1;
            continue;
        }

        if path.is_dir() {
            // Recurse, but skip symlink loops
            if path.is_symlink() {
                *skipped += 1;
                continue;
            }
            scan_dir(&path, policy, files, skipped, total_bytes)?;
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
    fn gitignore_negation_restores_non_secret_file() {
        let dir = temp_dir();
        fs::write(dir.join(".gitignore"), "*.txt\n!keep.txt\n").unwrap();
        fs::write(dir.join("drop.txt"), "ignored").unwrap();
        fs::write(dir.join("keep.txt"), "kept").unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files, vec![dir.join("keep.txt")]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn secret_policy_cannot_be_negated() {
        let dir = temp_dir();
        fs::write(dir.join(".gitignore"), "!.env\n!private.pem\n").unwrap();
        fs::write(dir.join(".env"), "NF_SENTINEL=secret").unwrap();
        fs::write(dir.join("private.pem"), "NF_SENTINEL_PEM").unwrap();
        fs::write(dir.join("main.rs"), "fn main() {}").unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files, vec![dir.join("main.rs")]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn respects_nested_gitignore_files() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("src").join("generated")).unwrap();
        fs::write(
            dir.join("src").join(".gitignore"),
            "generated/\nprivate.txt\n",
        )
        .unwrap();
        fs::write(dir.join("src").join("private.txt"), "ignored").unwrap();
        fs::write(dir.join("src").join("keep.rs"), "pub fn keep() {}").unwrap();
        fs::write(
            dir.join("src").join("generated").join("generated.rs"),
            "ignored",
        )
        .unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files, vec![dir.join("src").join("keep.rs")]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sensitive_directories_cannot_be_negated() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join(".ssh")).unwrap();
        fs::write(dir.join(".gitignore"), "!.ssh/id_custom\n").unwrap();
        fs::write(dir.join(".ssh").join("id_custom"), "NF_PRIVATE_KEY").unwrap();
        fs::write(dir.join("main.rs"), "fn main() {}").unwrap();

        let result = scan_workspace(&dir).unwrap();
        assert_eq!(result.files, vec![dir.join("main.rs")]);
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
