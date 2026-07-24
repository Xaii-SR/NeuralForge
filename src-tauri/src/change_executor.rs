use crate::planning_engine::TaskPlan;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct ChangeGenerator;

impl ChangeGenerator {
    /// Generates patches from a TaskPlan by iterating over each subtask's
    /// required files and producing a Replace operation that captures the
    /// current file state.
    ///
    /// IMPORTANT: This function only snapshots existing file content. It does
    /// NOT derive new content from the TaskPlan's objective — the agent
    /// writes file changes via filesystem commands (write_file/create_file)
    /// separately. The orchestrator must call generate_patches AFTER the
    /// agent has written its changes, so that the Replace operation captures
    /// the new content.
    ///
    /// If called BEFORE the agent writes changes, this function returns an
    /// error rather than generating a no-op patch. This prevents false
    /// successful execution of a patch that changes nothing.
    pub fn generate_patches(plan: &TaskPlan, root: &Path) -> Result<(Vec<Patch>, Vec<String>), String> {
        let mut patches = Vec::new();

        for subtask in &plan.subtasks {
            for file in &subtask.required_files {
                let normalized = normalize_relative_path(file)?;
                let target = root.join(&normalized);

                match fs::read_to_string(&target) {
                    Ok(content) => {
                        // File exists but we have no proposed new content
                        // from the TaskPlan. Returning a no-op patch would
                        // silently succeed with zero changes — fail instead.
                        return Err(format!(
                            "cannot generate patch for `{}`: file exists but TaskPlan provides no proposed content. \
                             Call generate_patches after the agent writes its changes.",
                            file
                        ));
                    }
                    Err(_) => {
                        // File is missing — we have no content to snapshot
                        // or replace with. Fail rather than creating an empty
                        // placeholder that would disappear on rollback.
                        return Err(format!(
                            "cannot generate patch for `{}`: file does not exist and TaskPlan provides no proposed content. \
                             Call generate_patches after the agent creates the file.",
                            file
                        ));
                    }
                }
            }
        }

        Ok((patches, Vec::new()))
    }
}

#[derive(Debug, Clone)]
pub struct Patch {
    pub id: String,
    pub file_path: String,
    pub original_content: Option<String>,
    pub operations: Vec<PatchOperation>,
    pub metadata: PatchMeta,
}

#[derive(Debug, Clone)]
pub enum PatchOperation {
    Insert { line: usize, content: String },
    Delete { start_line: usize, end_line: usize },
    Replace { start_line: usize, end_line: usize, new_content: String },
    AddFile { content: String },
    DeleteFile,
}

#[derive(Debug, Clone)]
pub struct PatchMeta {
    pub task_id: String,
    pub subtask_id: usize,
    pub reasoning: String,
    pub confidence: f32,
    pub generated_at: i64,
}

pub struct DiffGenerator;

impl DiffGenerator {
    pub fn generate_diff(patch: &Patch) -> UnifiedDiff {
        let mut lines = Vec::new();
        let mut added_lines = 0usize;
        let mut removed_lines = 0usize;

        for op in &patch.operations {
            match op {
                PatchOperation::Insert { content, .. } => {
                    added_lines += content.lines().count().max(1);
                    for line in content.lines() {
                        lines.push(DiffLine::Added(line.to_string()));
                    }
                }
                PatchOperation::Delete { start_line, end_line } => {
                    removed_lines += end_line.saturating_sub(*start_line).saturating_add(1);
                    lines.push(DiffLine::Removed(format!("lines {}..={}", start_line, end_line)));
                }
                PatchOperation::Replace { new_content, .. } => {
                    added_lines += new_content.lines().count().max(1);
                    removed_lines += patch.original_content.as_deref().map(|c| c.lines().count().max(1)).unwrap_or(0);
                    for line in new_content.lines() {
                        lines.push(DiffLine::Added(line.to_string()));
                    }
                }
                PatchOperation::AddFile { content } => {
                    added_lines += content.lines().count().max(1);
                    for line in content.lines() {
                        lines.push(DiffLine::Added(line.to_string()));
                    }
                }
                PatchOperation::DeleteFile => {
                    removed_lines += patch.original_content.as_deref().map(|c| c.lines().count().max(1)).unwrap_or(1);
                    lines.push(DiffLine::Removed("delete file".to_string()));
                }
            }
        }

        UnifiedDiff {
            file_path: patch.file_path.clone(),
            header: format!("diff --git a/{} b/{}", patch.file_path, patch.file_path),
            hunks: vec![DiffHunk {
                start_line_old: 1,
                count_old: removed_lines,
                start_line_new: 1,
                count_new: added_lines,
                lines: if lines.is_empty() { vec![DiffLine::Context("no content changes".to_string())] } else { lines },
            }],
            added_lines,
            removed_lines,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnifiedDiff {
    pub file_path: String,
    pub header: String,
    pub hunks: Vec<DiffHunk>,
    pub added_lines: usize,
    pub removed_lines: usize,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub start_line_old: usize,
    pub count_old: usize,
    pub start_line_new: usize,
    pub count_new: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

pub struct PatchApplier;

impl PatchApplier {
    pub fn apply(patch: &Patch, root: &Path) -> Result<Vec<ApplyResult>, String> {
        let target = resolve_path(root, &patch.file_path)?;
        let mut results = Vec::new();

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create parent directories for {}: {e}", patch.file_path))?;
        }

        let current = fs::read_to_string(&target).ok();
        if let Some(expected) = &patch.original_content {
            if current.as_deref() != Some(expected.as_str()) {
                return Err(format!("original content mismatch for {}", patch.file_path));
            }
        }

        let mut next_content = current.unwrap_or_default();
        for op in &patch.operations {
            next_content = apply_operation(&next_content, op)?;
        }

        match patch.operations.first() {
            Some(PatchOperation::DeleteFile) => {
                if target.exists() {
                    fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", patch.file_path))?;
                }
            }
            _ => {
                fs::write(&target, next_content).map_err(|e| format!("write {}: {e}", patch.file_path))?;
            }
        }

        results.push(ApplyResult { patch_id: patch.id.clone(), file_path: patch.file_path.clone(), success: true, error: None });
        Ok(results)
    }

    pub fn rollback(patch: &Patch, root: &Path) -> Result<bool, String> {
        let target = resolve_path(root, &patch.file_path)?;
        match &patch.original_content {
            Some(content) => {
                fs::write(&target, content).map_err(|e| format!("rollback {}: {e}", patch.file_path))?;
                Ok(true)
            }
            None => {
                if target.exists() {
                    fs::remove_file(&target).map_err(|e| format!("rollback delete {}: {e}", patch.file_path))?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub patch_id: String,
    pub file_path: String,
    pub success: bool,
    pub error: Option<String>,
}

pub struct PatchValidator;

impl PatchValidator {
    pub fn validate(patch: &Patch, root: &Path) -> Result<Vec<String>, Vec<String>> {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if patch.file_path.trim().is_empty() {
            errors.push("patch file path is empty".to_string());
        }
        if normalize_relative_path(&patch.file_path).is_err() {
            errors.push(format!("patch path `{}` escapes the workspace", patch.file_path));
        }
        if patch.operations.is_empty() {
            warnings.push(format!("patch `{}` contains no operations", patch.id));
        }
        if patch.operations.len() > 32 {
            warnings.push(format!("patch `{}` has a large number of operations", patch.id));
        }
        if let Ok(path) = resolve_path(root, &patch.file_path) {
            if path.exists() && patch.original_content.is_none() && !patch.operations.iter().any(|op| matches!(op, PatchOperation::DeleteFile)) {
                warnings.push(format!("patch `{}` does not capture existing file contents", patch.id));
            }
        }

        if errors.is_empty() { Ok(warnings) } else { Err(errors) }
    }

    pub fn detect_conflicts(patches: &[Patch]) -> Vec<String> {
        let mut seen = BTreeMap::<&str, &str>::new();
        let mut conflicts = Vec::new();
        for patch in patches {
            if let Some(previous) = seen.insert(&patch.file_path, &patch.id) {
                conflicts.push(format!("patches `{}` and `{}` both target `{}`", previous, patch.id, patch.file_path));
            }
        }
        conflicts
    }
}

fn normalize_relative_path(file: &str) -> Result<String, String> {
    let path = Path::new(file);
    if path.is_absolute() {
        return Err(format!("absolute path `{file}` is not allowed"));
    }

    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(format!("path `{file}` escapes the workspace"));
            }
        }
    }

    if parts.is_empty() {
        Err(format!("path `{file}` is empty"))
    } else {
        Ok(parts.join(&std::path::MAIN_SEPARATOR.to_string()))
    }
}

fn resolve_path(root: &Path, file_path: &str) -> Result<PathBuf, String> {
    let normalized = normalize_relative_path(file_path)?;
    let candidate = root.join(&normalized);
    let canonical_root = fs::canonicalize(root).map_err(|e| format!("canonicalize workspace root: {e}"))?;

    // Try canonicalizing the candidate itself (exists).
    if let Ok(canonical_candidate) = fs::canonicalize(&candidate) {
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(format!("path `{file_path}` escapes the workspace"));
        }
    } else if let Some(parent) = candidate.parent() {
        // Parent doesn't exist yet — check that the parent CAN be canonicalized
        // (i.e. its ancestors are within the workspace), then verify the
        // candidate's final component doesn't escape.
        if let Ok(canonical_parent) = fs::canonicalize(parent) {
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(format!("path `{file_path}` escapes the workspace"));
            }
        } else {
            // Walk up the parent chain to find an ancestor that exists,
            // verify it's within the workspace root.
            let mut ancestor = parent.canonicalize();
            if ancestor.is_err() {
                // Walk parents until we find an existing ancestor.
                let mut current = parent.to_path_buf();
                loop {
                    if let Ok(canonical) = fs::canonicalize(&current) {
                        if !canonical.starts_with(&canonical_root) {
                            return Err(format!("path `{file_path}` escapes the workspace"));
                        }
                        break;
                    }
                    match current.parent() {
                        Some(p) if p != current => current = p.to_path_buf(),
                        _ => {
                            // Can't find any existing ancestor — this means
                            // even the workspace root doesn't exist, which
                            // should not normally happen. Fall through to
                            // accept the path (the caller will create dirs).
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(candidate)
}

fn apply_operation(current: &str, op: &PatchOperation) -> Result<String, String> {
    match op {
        PatchOperation::Insert { line, content } => {
            let mut lines: Vec<String> = current.lines().map(|s| s.to_string()).collect();
            let idx = line.saturating_sub(1).min(lines.len());
            lines.insert(idx, content.clone());
            Ok(join_lines(&lines))
        }
        PatchOperation::Delete { start_line, end_line } => {
            let mut lines: Vec<String> = current.lines().map(|s| s.to_string()).collect();
            if *start_line == 0 || *end_line < *start_line {
                return Err("invalid delete range".to_string());
            }
            let start = start_line.saturating_sub(1).min(lines.len());
            let end = end_line.saturating_sub(1).min(lines.len().saturating_sub(1));
            if start > end || start >= lines.len() {
                return Err("delete range is out of bounds".to_string());
            }
            lines.drain(start..=end);
            Ok(join_lines(&lines))
        }
        PatchOperation::Replace { start_line, end_line, new_content } => {
            let mut lines: Vec<String> = current.lines().map(|s| s.to_string()).collect();
            if *start_line == 0 || *end_line < *start_line {
                return Err("invalid replace range".to_string());
            }
            let start = start_line.saturating_sub(1).min(lines.len());
            let end = end_line.saturating_sub(1).min(lines.len().saturating_sub(1));
            if start > end && !lines.is_empty() {
                return Err("replace range is out of bounds".to_string());
            }
            let replacement: Vec<String> = new_content.lines().map(|s| s.to_string()).collect();
            if lines.is_empty() {
                lines = replacement;
            } else {
                lines.splice(start..=end, replacement);
            }
            Ok(join_lines(&lines))
        }
        PatchOperation::AddFile { content } => Ok(content.clone()),
        PatchOperation::DeleteFile => Ok(String::new()),
    }
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut joined = lines.join("\n");
        joined.push('\n');
        joined
    }
}
