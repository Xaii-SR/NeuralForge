use serde::{Deserialize, Serialize};
use specta::Type;

/// Extension manifest schema (extension.json). "runtime" determines how the
/// entry point is invoked: "python" -> `python <entry_point>`, "node" ->
/// `node <entry_point>`. No native/binary runtime is supported - every
/// extension is an interpreted script, which keeps the process-isolation
/// story simple (no arbitrary native code ever gets loaded into or run
/// alongside the host).
#[derive(Type, Deserialize, Serialize, Clone)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub entry_point: String,
    pub runtime: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Serialize, Type, Clone)]
pub struct InstalledExtension {
    pub manifest: ExtensionManifest,
    pub dir: String,
    pub enabled: bool,
}

pub fn interpreter_for(runtime: &str) -> Option<&'static str> {
    match runtime {
        "python" => Some("python"),
        "node" => Some("node"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_manifest() {
        let json = r#"{
            "name": "python-repl",
            "version": "0.1.0",
            "author": "NeuralForge",
            "description": "Execute Python code",
            "entry_point": "main.py",
            "runtime": "python",
            "permissions": ["execute"]
        }"#;
        let manifest: ExtensionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "python-repl");
        assert_eq!(manifest.runtime, "python");
        assert_eq!(manifest.permissions, vec!["execute"]);
    }

    #[test]
    fn description_and_permissions_default_when_absent() {
        let json = r#"{
            "name": "minimal",
            "version": "0.1.0",
            "author": "someone",
            "entry_point": "main.py",
            "runtime": "python"
        }"#;
        let manifest: ExtensionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.description, "");
        assert!(manifest.permissions.is_empty());
    }

    #[test]
    fn interpreter_for_known_and_unknown_runtimes() {
        assert_eq!(interpreter_for("python"), Some("python"));
        assert_eq!(interpreter_for("node"), Some("node"));
        assert_eq!(interpreter_for("rust"), None);
    }
}
