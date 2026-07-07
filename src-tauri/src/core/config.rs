use crate::core::errors::AppResult;
use std::path::Path;

pub const MEMORY_DIR_NAME: &str = ".neuralforge";
pub const MEMORY_SUBDIR_NAME: &str = "memory";

pub const MEMORY_FILES: &[&str] = &[
    "architecture.md",
    "decisions.md",
    "coding_rules.md",
    "project_rules.md",
    "known_bugs.md",
    "agent_history.md",
    "current_state.md",
];

fn header_for(file_name: &str) -> String {
    let title = file_name.trim_end_matches(".md").replace('_', " ");
    let title = title
        .split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("# {title}\n\n")
}

/// Creates .neuralforge/memory/ with the 7 template files if missing.
/// Never overwrites a file that already exists.
pub fn ensure_memory_scaffold(workspace_root: &Path) -> AppResult<()> {
    let memory_dir = workspace_root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
    std::fs::create_dir_all(&memory_dir)?;

    for file_name in MEMORY_FILES {
        let path = memory_dir.join(file_name);
        if !path.exists() {
            std::fs::write(&path, header_for(file_name))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_workspace() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("neuralforge_config_test_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ensure_memory_scaffold_creates_all_seven_files() {
        let root = temp_workspace();
        ensure_memory_scaffold(&root).unwrap();

        let memory_dir = root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
        for file_name in MEMORY_FILES {
            let path = memory_dir.join(file_name);
            assert!(path.exists(), "expected {file_name} to be created");
            let content = fs::read_to_string(&path).unwrap();
            assert!(content.starts_with('#'), "expected {file_name} to have a header");
        }

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_memory_scaffold_does_not_overwrite_existing_content() {
        let root = temp_workspace();
        let memory_dir = root.join(MEMORY_DIR_NAME).join(MEMORY_SUBDIR_NAME);
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(memory_dir.join("decisions.md"), "custom content").unwrap();

        ensure_memory_scaffold(&root).unwrap();

        let content = fs::read_to_string(memory_dir.join("decisions.md")).unwrap();
        assert_eq!(content, "custom content");

        fs::remove_dir_all(&root).unwrap();
    }
}
