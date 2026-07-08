use super::manifest::{ExtensionManifest, InstalledExtension};
use crate::core::errors::{AppError, AppResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn home_dir() -> AppResult<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";

    std::env::var(var)
        .map(PathBuf::from)
        .map_err(|_| AppError::Provider(format!("could not resolve home directory ({var} not set)")))
}

pub fn extensions_dir() -> AppResult<PathBuf> {
    Ok(home_dir()?.join(".neuralforge").join("extensions"))
}

const PYTHON_REPL_MANIFEST: &str = r#"{
  "name": "python-repl",
  "version": "0.1.0",
  "author": "NeuralForge",
  "description": "Execute Python code and capture its output",
  "entry_point": "main.py",
  "runtime": "python",
  "permissions": ["execute"]
}
"#;

const PYTHON_REPL_MAIN: &str = r#"import sys
import json
import io
import contextlib


def main():
    request = json.loads(sys.stdin.read())
    code = request.get("code", "")
    output = io.StringIO()
    error = None
    try:
        with contextlib.redirect_stdout(output):
            exec(code, {"__name__": "__main__"})
    except Exception as e:
        error = str(e)
    print(json.dumps({"success": error is None, "output": output.getvalue(), "error": error}))


if __name__ == "__main__":
    main()
"#;

const FILE_SEARCH_MANIFEST: &str = r#"{
  "name": "file-search",
  "version": "0.1.0",
  "author": "NeuralForge",
  "description": "Fuzzy filename search over the workspace file list",
  "entry_point": "main.py",
  "runtime": "python",
  "permissions": []
}
"#;

const FILE_SEARCH_MAIN: &str = r#"import sys
import json


def score(query, path):
    query = query.lower()
    path_lower = path.lower()
    if query not in path_lower:
        return None
    name = path_lower.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
    if query in name:
        return 100 - len(path)
    return 50 - len(path)


def main():
    request = json.loads(sys.stdin.read())
    query = request.get("query", "")
    files = request.get("files", [])
    scored = []
    for f in files:
        s = score(query, f)
        if s is not None:
            scored.append((s, f))
    scored.sort(key=lambda pair: -pair[0])
    results = [f for _, f in scored[:20]]
    print(json.dumps({"success": True, "output": results, "error": None}))


if __name__ == "__main__":
    main()
"#;

/// Writes the bundled example extensions into the extensions directory if
/// they aren't already present. Never overwrites - same non-destructive
/// discipline as core::config::ensure_memory_scaffold. Bundled extensions
/// are not special-cased anywhere else: once written, they're discovered
/// and loaded through the exact same scan() path as anything a user drops
/// in manually.
pub fn ensure_bundled_extensions(dir: &Path) -> AppResult<()> {
    write_bundled(dir, "python-repl", PYTHON_REPL_MANIFEST, PYTHON_REPL_MAIN)?;
    write_bundled(dir, "file-search", FILE_SEARCH_MANIFEST, FILE_SEARCH_MAIN)?;
    Ok(())
}

fn write_bundled(base: &Path, name: &str, manifest: &str, main_script: &str) -> AppResult<()> {
    let ext_dir = base.join(name);
    std::fs::create_dir_all(&ext_dir)?;
    let manifest_path = ext_dir.join("extension.json");
    if !manifest_path.exists() {
        std::fs::write(&manifest_path, manifest)?;
    }
    let main_path = ext_dir.join("main.py");
    if !main_path.exists() {
        std::fs::write(&main_path, main_script)?;
    }
    Ok(())
}

fn load_enabled_state(dir: &Path) -> HashMap<String, bool> {
    let path = dir.join("enabled_state.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_enabled_state(dir: &Path, state: &HashMap<String, bool>) -> AppResult<()> {
    let path = dir.join("enabled_state.json");
    let json = serde_json::to_string_pretty(state).map_err(|e| AppError::Provider(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Scans immediate subdirectories of `dir` for an extension.json. Missing or
/// malformed manifests are skipped, not fatal - one broken extension
/// shouldn't prevent every other one from loading.
pub fn scan(dir: &Path) -> AppResult<Vec<InstalledExtension>> {
    if !dir.exists() {
        return Ok(vec![]);
    }

    let enabled_state = load_enabled_state(dir);
    let mut extensions = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("extension.json");
        let Ok(content) = std::fs::read_to_string(&manifest_path) else { continue };
        let Ok(manifest) = serde_json::from_str::<ExtensionManifest>(&content) else { continue };
        let enabled = enabled_state.get(&manifest.name).copied().unwrap_or(true);
        extensions.push(InstalledExtension {
            manifest,
            dir: path.to_string_lossy().to_string(),
            enabled,
        });
    }

    extensions.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(extensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_ext_loader_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ensure_bundled_extensions_creates_both_and_is_idempotent() {
        let dir = temp_dir();
        ensure_bundled_extensions(&dir).unwrap();

        let found = scan(&dir).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|e| e.manifest.name == "python-repl"));
        assert!(found.iter().any(|e| e.manifest.name == "file-search"));
        assert!(found.iter().all(|e| e.enabled));

        // customize one, then re-run ensure_bundled_extensions - must not overwrite
        let main_path = dir.join("python-repl").join("main.py");
        std::fs::write(&main_path, "# customized by user").unwrap();
        ensure_bundled_extensions(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&main_path).unwrap(), "# customized by user");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_skips_directories_without_a_valid_manifest() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.join("broken")).unwrap();
        std::fs::write(dir.join("broken").join("extension.json"), "not valid json").unwrap();
        std::fs::create_dir_all(dir.join("no-manifest")).unwrap();

        let found = scan(&dir).unwrap();
        assert_eq!(found.len(), 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scan_on_missing_directory_returns_empty_not_error() {
        let mut dir = std::env::temp_dir();
        dir.push("neuralforge_definitely_does_not_exist_dir");
        let found = scan(&dir).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn enabled_state_roundtrip() {
        let dir = temp_dir();
        ensure_bundled_extensions(&dir).unwrap();

        let mut state = HashMap::new();
        state.insert("python-repl".to_string(), false);
        save_enabled_state(&dir, &state).unwrap();

        let found = scan(&dir).unwrap();
        let repl = found.iter().find(|e| e.manifest.name == "python-repl").unwrap();
        let search = found.iter().find(|e| e.manifest.name == "file-search").unwrap();
        assert!(!repl.enabled);
        assert!(search.enabled, "unmentioned extensions default to enabled");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
