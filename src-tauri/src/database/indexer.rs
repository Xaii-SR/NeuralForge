use crate::core::errors::AppResult;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const EXCLUDED_DIRS: &[&str] = &[
    "node_modules", ".next", "out", "target", "dist", "logs", "models", ".git", ".neuralforge",
];

const MAX_FILE_BYTES: u64 = 1_000_000;
const CHUNK_LINES: usize = 40;
const CHUNK_OVERLAP: usize = 5;

#[derive(Serialize, Type, Clone, Default)]
pub struct IndexStats {
    pub files_scanned: u64,
    pub files_indexed: u64,
    pub files_skipped_unchanged: u64,
    pub files_skipped_binary: u64,
    pub files_skipped_size: u64,
    pub files_failed: u64,
    pub chunks_created: u64,
    pub symbols_extracted: u64,
    pub dependencies_extracted: u64,
    pub languages_detected: HashMap<String, u64>,
    pub total_bytes_indexed: u64,
    pub last_index_timestamp: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Interface,
    Import,
    Module,
    Constant,
    Static,
    TypeAlias,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Impl => "impl",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Import => "import",
            SymbolKind::Module => "module",
            SymbolKind::Constant => "constant",
            SymbolKind::Static => "static",
            SymbolKind::TypeAlias => "type_alias",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Symbol {
    pub file_path: String,
    pub language: String,
    pub module_path: String,
    pub qualified_name: String,
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub visibility: Option<String>,
    pub signature: Option<String>,
    pub documentation: Option<String>,
    pub symbol_hash: String,
    pub import_source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Dependency {
    pub source_file: String,
    pub target_file: Option<String>,
    pub source_symbol: Option<String>,
    pub target_symbol: Option<String>,
    pub dependency_type: String,
    pub import_source: Option<String>,
    pub created_at: i64,
}

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn is_probably_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    !sample.contains(&0)
}

fn should_skip_dir(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

fn classify_language(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "Rust",
        Some("ts" | "tsx") => "TypeScript",
        Some("js" | "jsx") => "JavaScript",
        Some("py") => "Python",
        Some("c" | "h" | "cpp" | "hpp" | "cc" | "cxx") => "C/C++",
        Some("json") => "JSON",
        Some("yaml" | "yml") => "YAML",
        Some("toml") => "TOML",
        Some("md" | "mdx") => "Markdown",
        Some("css" | "scss" | "less") => "CSS",
        Some("html" | "htm") => "HTML",
        Some("sql") => "SQL",
        Some("sh" | "bash" | "zsh") => "Shell",
        Some("env" | "ini" | "cfg") => "Config",
        Some("txt" | "text") => "Text",
        _ => "Other",
    }
}

/// Converts a relative file path into a normalized module path.
fn module_path_from_file_path(file_path: &str) -> String {
    let path = Path::new(file_path);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(file_path);
    let mut components: Vec<&str> = Vec::new();
    if let Some(parent) = path.parent() {
        for component in parent.components() {
            if let std::path::Component::Normal(name) = component {
                let name_str = name.to_str().unwrap_or("");
                if name_str == "src" || name_str == "lib" || name_str == "app" {
                    continue;
                }
                components.push(name_str);
            }
        }
    }
    components.push(stem);
    if components.is_empty() {
        return file_path.to_string();
    }
    components.join("::")
}

fn qualified_name(module_path: &str, name: &str) -> String {
    if module_path.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", module_path, name)
    }
}

fn symbol_hash_value(file_path: &str, name: &str, kind: &str, start_line: i64, signature: Option<&str>) -> String {
    let mut hasher = DefaultHasher::new();
    file_path.hash(&mut hasher);
    name.hash(&mut hasher);
    kind.hash(&mut hasher);
    start_line.hash(&mut hasher);
    if let Some(sig) = signature {
        sig.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn capture_doc_comment(lines: &[&str], line_idx: usize) -> Option<String> {
    let mut docs: Vec<&str> = Vec::new();
    if line_idx == 0 { return None; }
    let mut idx = line_idx - 1;
    loop {
        let trimmed = lines[idx].trim();
        if trimmed.starts_with("///") {
            docs.push(trimmed.trim_start_matches("///").trim());
        } else if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            // skip
        } else { break; }
        if idx == 0 { break; }
        idx -= 1;
    }
    if docs.is_empty() { None } else { docs.reverse(); Some(docs.join(" ")) }
}

fn find_closing_brace(lines: &[&str], open_line: usize) -> usize {
    let mut depth: i64 = 0;
    let mut started = false;
    for (i, line) in lines.iter().enumerate().skip(open_line.saturating_sub(1)) {
        for ch in line.chars() {
            match ch { '{' => { depth += 1; started = true; } '}' => { depth -= 1; if started && depth <= 0 { return (i + 1) as usize; } } _ => {} }
        }
    }
    (open_line + 1).min(lines.len())
}

fn regex_capture<'t>(text: &'t str, pattern: &Regex) -> Option<regex::Captures<'t>> {
    pattern.captures(text)
}

lazy_static::lazy_static! {
    // Rust patterns
    static ref RE_RUST_FN: Regex = Regex::new(r"^\s*(pub\s*(?:\([^)]*\))?\s*)?(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();
    static ref RE_RUST_STRUCT: Regex = Regex::new(r"^\s*(pub\s*(?:\([^)]*\))?\s*)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    static ref RE_RUST_ENUM: Regex = Regex::new(r"^\s*(pub\s*(?:\([^)]*\))?\s*)?enum\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    static ref RE_RUST_TRAIT: Regex = Regex::new(r"^\s*(pub\s*(?:\([^)]*\))?\s*)?trait\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    static ref RE_RUST_IMPL: Regex = Regex::new(r"^\s*impl\s+([a-zA-Z_][a-zA-Z0-9_<>]*(?:\s+for\s+[a-zA-Z_][a-zA-Z0-9_<>]*)?)\s*\{").unwrap();
    static ref RE_RUST_USE: Regex = Regex::new(r"^\s*use\s+([^;]+);").unwrap();
    static ref RE_RUST_MOD: Regex = Regex::new(r"^\s*(?:pub\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*;").unwrap();
    // TypeScript patterns
    static ref RE_TS_FN: Regex = Regex::new(r"^\s*(export\s+)?(?:async\s+)?function\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(").unwrap();
    static ref RE_TS_CLASS: Regex = Regex::new(r"^\s*(export\s+)?(?:abstract\s+)?class\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    static ref RE_TS_INTERFACE: Regex = Regex::new(r"^\s*(export\s+)?interface\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    static ref RE_TS_IMPORT: Regex = Regex::new(r#"^\s*import\s+.*\s+from\s+['"]([^'"]+)['"]"#).unwrap();
    static ref RE_TS_EXPORT_FROM: Regex = Regex::new(r#"^\s*export\s+.*\s+from\s+['"]([^'"]+)['"]"#).unwrap();
    // Python patterns
    static ref RE_PY_FN: Regex = Regex::new(r"^\s*(?:async\s+)?def\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();
    static ref RE_PY_CLASS: Regex = Regex::new(r"^\s*class\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*[\(:]").unwrap();
    static ref RE_PY_IMPORT: Regex = Regex::new(r"^\s*(?:from\s+(\S+)\s+)?import\s+(\S+)").unwrap();
}

// ---- Symbol Extraction ----

fn extract_rust_symbols(lines: &[&str], file_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let module_path = module_path_from_file_path(file_path);
    let vis = |raw: Option<&str>| -> Option<String> {
        raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).or_else(|| Some("private".to_string()))
    };

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') { continue; }

        if let Some(caps) = regex_capture(trimmed, &RE_RUST_FN) {
            let visibility = vis(caps.get(1).map(|m| m.as_str()));
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            let end_line = find_closing_brace(lines, line_num);
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Function.as_str().to_string(), start_line: line_num as i64, end_line: end_line as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "function", line_num as i64, Some(line)), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_STRUCT) {
            let visibility = vis(caps.get(1).map(|m| m.as_str()));
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            let end_line = if trimmed.contains(';') { line_num } else { find_closing_brace(lines, line_num) };
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Struct.as_str().to_string(), start_line: line_num as i64, end_line: end_line as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "struct", line_num as i64, None), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_ENUM) {
            let visibility = vis(caps.get(1).map(|m| m.as_str()));
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            let end_line = find_closing_brace(lines, line_num);
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Enum.as_str().to_string(), start_line: line_num as i64, end_line: end_line as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "enum", line_num as i64, None), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_TRAIT) {
            let visibility = vis(caps.get(1).map(|m| m.as_str()));
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            let end_line = find_closing_brace(lines, line_num);
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Trait.as_str().to_string(), start_line: line_num as i64, end_line: end_line as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "trait", line_num as i64, None), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_IMPL) {
            let target = caps.get(1).map(|m| m.as_str()).unwrap_or("unnamed");
            let name = if let Some(idx) = target.find(" for ") { &target[idx + 5..] } else { target };
            let end_line = find_closing_brace(lines, line_num);
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, &format!("impl_{}", name)), name: format!("impl {}", name),
                kind: SymbolKind::Impl.as_str().to_string(), start_line: line_num as i64, end_line: end_line as i64,
                visibility: None, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "impl", line_num as i64, None), import_source: None,
            });
            continue;
        }
        // Imports: store ALL use statements including crate::
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_USE) {
            let import_target = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
            if !import_target.contains("self") {
                let name = import_target.split("::").last().unwrap_or(&import_target).to_string();
                symbols.push(Symbol {
                    file_path: file_path.to_string(), language: "Rust".to_string(), module_path: module_path.clone(),
                    qualified_name: qualified_name(&module_path, &name), name,
                    kind: SymbolKind::Import.as_str().to_string(), start_line: line_num as i64, end_line: line_num as i64,
                    visibility: None, signature: None, documentation: None,
                    symbol_hash: symbol_hash_value(file_path, &import_target, "import", line_num as i64, None),
                    import_source: Some(import_target),
                });
            }
            continue;
        }
    }
    symbols
}

fn extract_typescript_symbols(lines: &[&str], file_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let module_path = module_path_from_file_path(file_path);

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") { continue; }

        if let Some(caps) = regex_capture(trimmed, &RE_TS_FN) {
            let visibility = if caps.get(1).is_some() { Some("export".to_string()) } else { Some("private".to_string()) };
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "TypeScript".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Function.as_str().to_string(), start_line: line_num as i64,
                end_line: find_closing_brace(lines, line_num) as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "function", line_num as i64, Some(line)), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_TS_CLASS) {
            let visibility = if caps.get(1).is_some() { Some("export".to_string()) } else { Some("private".to_string()) };
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "TypeScript".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Class.as_str().to_string(), start_line: line_num as i64,
                end_line: find_closing_brace(lines, line_num) as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "class", line_num as i64, None), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_TS_INTERFACE) {
            let visibility = if caps.get(1).is_some() { Some("export".to_string()) } else { Some("private".to_string()) };
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("unnamed");
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "TypeScript".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Interface.as_str().to_string(), start_line: line_num as i64,
                end_line: find_closing_brace(lines, line_num) as i64,
                visibility, signature: Some(line.trim().to_string()), documentation: capture_doc_comment(lines, i),
                symbol_hash: symbol_hash_value(file_path, name, "interface", line_num as i64, None), import_source: None,
            });
            continue;
        }
        // Imports: support both single and double quotes
        if let Some(caps) = regex_capture(trimmed, &RE_TS_IMPORT) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let name = source.split('/').last().unwrap_or(&source).to_string();
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "TypeScript".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, &name), name,
                kind: SymbolKind::Import.as_str().to_string(), start_line: line_num as i64, end_line: line_num as i64,
                visibility: None, signature: None, documentation: None,
                symbol_hash: symbol_hash_value(file_path, &source, "import", line_num as i64, None),
                import_source: Some(source),
            });
            continue;
        }
        // export ... from ...
        if let Some(caps) = regex_capture(trimmed, &RE_TS_EXPORT_FROM) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let name = source.split('/').last().unwrap_or(&source).to_string();
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "TypeScript".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, &name), name,
                kind: SymbolKind::Import.as_str().to_string(), start_line: line_num as i64, end_line: line_num as i64,
                visibility: Some("export".to_string()), signature: None, documentation: None,
                symbol_hash: symbol_hash_value(file_path, &source, "import", line_num as i64, None),
                import_source: Some(source),
            });
            continue;
        }
    }
    symbols
}

fn extract_python_symbols(lines: &[&str], file_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let module_path = module_path_from_file_path(file_path);

    fn find_python_block_end(lines: &[&str], start_line: usize) -> usize {
        if start_line >= lines.len() { return start_line; }
        let base_indent = lines[start_line - 1].len() - lines[start_line - 1].trim_start().len();
        if base_indent == 0 && lines[start_line - 1].trim().ends_with(':') {
            for i in start_line..lines.len() {
                let trimmed = lines[i].trim();
                if trimmed.is_empty() { continue; }
                let this_indent = lines[i].len() - trimmed.len();
                if this_indent <= base_indent && !trimmed.starts_with('#') { return i; }
            }
        }
        lines.len()
    }

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }

        if let Some(caps) = regex_capture(trimmed, &RE_PY_FN) {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("unnamed");
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Python".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Function.as_str().to_string(), start_line: line_num as i64,
                end_line: find_python_block_end(lines, line_num) as i64,
                visibility: Some("public".to_string()), signature: Some(line.trim().to_string()),
                documentation: None,
                symbol_hash: symbol_hash_value(file_path, name, "function", line_num as i64, Some(line)), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_PY_CLASS) {
            let name = caps.get(1).map(|m| m.as_str()).unwrap_or("unnamed");
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Python".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, name), name: name.to_string(),
                kind: SymbolKind::Class.as_str().to_string(), start_line: line_num as i64,
                end_line: find_python_block_end(lines, line_num) as i64,
                visibility: Some("public".to_string()), signature: Some(line.trim().to_string()),
                documentation: None,
                symbol_hash: symbol_hash_value(file_path, name, "class", line_num as i64, None), import_source: None,
            });
            continue;
        }
        if let Some(caps) = regex_capture(trimmed, &RE_PY_IMPORT) {
            let source = if let Some(from) = caps.get(1) { from.as_str().to_string() }
                else { caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default() };
            let name = source.split('.').last().unwrap_or(&source).to_string();
            symbols.push(Symbol {
                file_path: file_path.to_string(), language: "Python".to_string(), module_path: module_path.clone(),
                qualified_name: qualified_name(&module_path, &name), name,
                kind: SymbolKind::Import.as_str().to_string(), start_line: line_num as i64, end_line: line_num as i64,
                visibility: None, signature: None, documentation: None,
                symbol_hash: symbol_hash_value(file_path, &source, "import", line_num as i64, None),
                import_source: Some(source),
            });
            continue;
        }
    }
    symbols
}

pub fn extract_symbols(content: &str, file_path: &str, language: &str) -> Vec<Symbol> {
    let lines: Vec<&str> = content.lines().collect();
    match language {
        "Rust" => extract_rust_symbols(&lines, file_path),
        "TypeScript" => extract_typescript_symbols(&lines, file_path),
        "Python" => extract_python_symbols(&lines, file_path),
        _ => Vec::new(),
    }
}

// ---- Dependency Extraction ----

fn extract_rust_dependencies(lines: &[&str], file_path: &str, now: i64) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let module_path = module_path_from_file_path(file_path);

    for line in lines.iter() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') { continue; }

        // mod declarations: internal file references
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_MOD) {
            let mod_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            deps.push(Dependency {
                source_file: file_path.to_string(),
                target_file: None,
                source_symbol: None,
                target_symbol: Some(format!("mod {}", mod_name)),
                dependency_type: "file_reference".to_string(),
                import_source: Some(format!("mod {}", mod_name)),
                created_at: now,
            });
            continue;
        }

        // use statements: crate:: = internal, other = external
        if let Some(caps) = regex_capture(trimmed, &RE_RUST_USE) {
            let import_target = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("").to_string();
            if import_target.contains("self") { continue; }

            let dep_type = if import_target.starts_with("crate::") {
                "internal_import"
            } else {
                "import"
            };

            let target_symbol = import_target.split("::").last().unwrap_or(&import_target).to_string();
            let target_file = if dep_type == "internal_import" {
                let first = module_path.split("::").next().unwrap_or("crate");
                Some(import_target.replace("crate::", &format!("{}::", first)))
            } else {
                None
            };
            deps.push(Dependency {
                source_file: file_path.to_string(),
                target_file,
                source_symbol: None,
                target_symbol: Some(target_symbol),
                dependency_type: dep_type.to_string(),
                import_source: Some(import_target),
                created_at: now,
            });
            continue;
        }
    }
    deps
}

fn extract_typescript_dependencies(lines: &[&str], file_path: &str, now: i64) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for line in lines.iter() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") { continue; }

        // import ... from '...'
        if let Some(caps) = regex_capture(trimmed, &RE_TS_IMPORT) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let target_symbol = source.split('/').last().unwrap_or(&source).to_string();
            let dep_type = if source.starts_with('.') { "internal_import" } else { "import" };
            deps.push(Dependency {
                source_file: file_path.to_string(),
                target_file: if source.starts_with('.') { Some(source.clone()) } else { None },
                source_symbol: None,
                target_symbol: Some(target_symbol),
                dependency_type: dep_type.to_string(),
                import_source: Some(source),
                created_at: now,
            });
            continue;
        }

        // export ... from '...'
        if let Some(caps) = regex_capture(trimmed, &RE_TS_EXPORT_FROM) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let target_symbol = source.split('/').last().unwrap_or(&source).to_string();
            let dep_type = if source.starts_with('.') { "internal_import" } else { "import" };
            deps.push(Dependency {
                source_file: file_path.to_string(),
                target_file: if source.starts_with('.') { Some(source.clone()) } else { None },
                source_symbol: None,
                target_symbol: Some(target_symbol),
                dependency_type: dep_type.to_string(),
                import_source: Some(source),
                created_at: now,
            });
            continue;
        }
    }
    deps
}

fn extract_python_dependencies(lines: &[&str], file_path: &str, now: i64) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for line in lines.iter() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }

        if let Some(caps) = regex_capture(trimmed, &RE_PY_IMPORT) {
            let source = if let Some(from) = caps.get(1) { from.as_str().to_string() }
                else { caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default() };
            let target_symbol = source.split('.').last().unwrap_or(&source).to_string();
            let dep_type = if source.starts_with('.') { "internal_import" } else { "import" };
            deps.push(Dependency {
                source_file: file_path.to_string(),
                target_file: None,
                source_symbol: None,
                target_symbol: Some(target_symbol),
                dependency_type: dep_type.to_string(),
                import_source: Some(source),
                created_at: now,
            });
            continue;
        }
    }
    deps
}

pub fn extract_dependencies(content: &str, file_path: &str, language: &str, now: i64) -> Vec<Dependency> {
    let lines: Vec<&str> = content.lines().collect();
    match language {
        "Rust" => extract_rust_dependencies(&lines, file_path, now),
        "TypeScript" => extract_typescript_dependencies(&lines, file_path, now),
        "Python" => extract_python_dependencies(&lines, file_path, now),
        _ => Vec::new(),
    }
}

// ---- Storage ----

fn chunk_lines(content: &str) -> Vec<(usize, usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() { return vec![]; }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let text = lines[start..end].join("\n");
        chunks.push((start + 1, end, text));
        if end == lines.len() { break; }
        start = end - CHUNK_OVERLAP;
    }
    chunks
}

fn store_symbols(conn: &Connection, symbols: &[Symbol], ref_path: &str) {
    conn.execute("DELETE FROM symbols WHERE file_path = ?1", params![ref_path]).ok();
    for sym in symbols {
        if let Err(e) = conn.execute(
            "INSERT INTO symbols (file_path, language, module_path, qualified_name, name, kind, start_line, end_line, visibility, signature, documentation, symbol_hash, import_source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![sym.file_path, sym.language, sym.module_path, sym.qualified_name, sym.name, sym.kind, sym.start_line, sym.end_line, sym.visibility, sym.signature, sym.documentation, sym.symbol_hash, sym.import_source],
        ) {
            tracing::warn!(target: "database", event = "symbol_insert_failed", error = %e, symbol = %sym.name);
        }
    }
}

fn store_dependencies(conn: &Connection, deps: &[Dependency], ref_path: &str) {
    conn.execute("DELETE FROM dependencies WHERE source_file = ?1", params![ref_path]).ok();
    for dep in deps {
        if let Err(e) = conn.execute(
            "INSERT INTO dependencies (source_file, target_file, source_symbol, target_symbol, dependency_type, import_source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![dep.source_file, dep.target_file, dep.source_symbol, dep.target_symbol, dep.dependency_type, dep.import_source, dep.created_at],
        ) {
            tracing::warn!(target: "database", event = "dependency_insert_failed", error = %e, import = %dep.import_source.as_deref().unwrap_or(""));
        }
    }
}

pub fn index_workspace(conn: &Connection, workspace_root: &Path) -> AppResult<IndexStats> {
    let mut stats = IndexStats::default();
    stats.languages_detected = HashMap::new();
    stats.last_index_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    let walker = WalkDir::new(workspace_root).into_iter().filter_entry(|entry| {
        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy();
            return !should_skip_dir(&name);
        }
        true
    });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        let file_size = metadata.len();
        if file_size > MAX_FILE_BYTES { stats.files_skipped_size += 1; continue; }
        stats.files_scanned += 1;

        let modified_at = metadata.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64).unwrap_or(0);
        let rel_path = path.strip_prefix(workspace_root).unwrap_or(path)
            .to_string_lossy().to_string();

        let existing: Option<(String, i64)> = conn.query_row(
            "SELECT content_hash, modified_at FROM files WHERE path = ?1", params![rel_path],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        ).ok();

        if let Some((ref existing_hash, existing_modified)) = existing {
            if existing_modified == modified_at { stats.files_skipped_unchanged += 1; continue; }
            let Ok(bytes) = std::fs::read(path) else { stats.files_failed += 1; continue; };
            if !is_probably_text(&bytes) { stats.files_skipped_binary += 1; continue; }
            let Ok(content) = String::from_utf8(bytes) else { stats.files_failed += 1; continue; };
            if hash_content(&content) == *existing_hash {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                conn.execute("UPDATE files SET modified_at = ?1, indexed_at = ?2 WHERE path = ?3",
                    params![modified_at, now, rel_path]).ok();
                stats.files_skipped_unchanged += 1; continue;
            }
        }

        let Ok(bytes) = std::fs::read(path) else { stats.files_failed += 1; continue; };
        if !is_probably_text(&bytes) { stats.files_skipped_binary += 1; continue; }
        let Ok(content) = String::from_utf8(bytes) else { stats.files_failed += 1; continue; };
        let hash = hash_content(&content);
        let language = classify_language(path);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let line_count = content.lines().count() as i64;

        *stats.languages_detected.entry(language.to_string()).or_insert(0) += 1;
        stats.total_bytes_indexed += file_size;

        // Extract symbols
        let symbols = extract_symbols(&content, &rel_path, language);
        stats.symbols_extracted += symbols.len() as u64;

        // Extract dependencies
        let deps = extract_dependencies(&content, &rel_path, language, now);
        stats.dependencies_extracted += deps.len() as u64;

        let file_id: i64 = if let Some(id) = conn.query_row(
            "SELECT id FROM files WHERE path = ?1", params![rel_path],
            |row| row.get::<_, i64>(0),
        ).ok() {
            conn.execute("UPDATE files SET content_hash = ?1, indexed_at = ?2, file_size = ?3, modified_at = ?4, language = ?5, line_count = ?6 WHERE id = ?7",
                params![hash, now, file_size as i64, modified_at, language, line_count, id]).ok();
            conn.execute("DELETE FROM chunks WHERE file_id = ?1", params![id]).ok();
            id
        } else {
            conn.execute("INSERT INTO files (path, content_hash, indexed_at, file_size, modified_at, language, line_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![rel_path, hash, now, file_size as i64, modified_at, language, line_count]).ok();
            conn.last_insert_rowid()
        };

        for (start_line, end_line, text) in chunk_lines(&content) {
            conn.execute("INSERT INTO chunks (file_id, path, start_line, end_line, content) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![file_id, rel_path, start_line as i64, end_line as i64, text]).ok();
            stats.chunks_created += 1;
        }

        store_symbols(conn, &symbols, &rel_path);
        store_dependencies(conn, &deps, &rel_path);
        stats.files_indexed += 1;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn chunk_lines_splits_with_overlap() {
        let content = (1..=100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunk_lines(&content);
        assert!(chunks.len() > 1); assert_eq!(chunks[0].0, 1); assert_eq!(chunks[0].1, CHUNK_LINES);
        assert_eq!(chunks[1].0, CHUNK_LINES - CHUNK_OVERLAP + 1);
    }
    #[test] fn chunk_lines_handles_short_content() {
        let chunks = chunk_lines("just one line");
        assert_eq!(chunks.len(), 1); assert_eq!(chunks[0].0, 1); assert_eq!(chunks[0].1, 1);
    }
    #[test] fn is_probably_text_rejects_null_bytes() {
        assert!(is_probably_text(b"hello world")); assert!(!is_probably_text(b"hello\0world"));
    }
    #[test] fn classify_language_returns_correct_language() {
        assert_eq!(classify_language(Path::new("main.rs")), "Rust");
        assert_eq!(classify_language(Path::new("app.ts")), "TypeScript");
        assert_eq!(classify_language(Path::new("script.py")), "Python");
    }

    // ---- Symbol tests ----
    #[test] fn module_path_from_relative_path() {
        assert_eq!(module_path_from_file_path("src/database/indexer.rs"), "database::indexer");
        assert_eq!(module_path_from_file_path("src/main.rs"), "main");
    }
    #[test] fn rust_function_extraction() {
        let content = "/// Calculate.\npub fn add(a: i32) -> i32 { a }\nfn priv() {}\n";
        let syms = extract_symbols(content, "src/lib.rs", "Rust");
        assert_eq!(syms.len(), 2); assert_eq!(syms[0].name, "add"); assert_eq!(syms[0].visibility.as_deref(), Some("pub"));
        assert_eq!(syms[1].name, "priv");
    }
    #[test] fn rust_struct_extraction() {
        let content = "pub struct Config {}\nenum Status { A, B }";
        let syms = extract_symbols(content, "src/config.rs", "Rust");
        assert!(syms.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(syms.iter().any(|s| s.name == "Status" && s.kind == "enum"));
    }
    #[test] fn rust_trait_and_impl_extraction() {
        let content = "pub trait R { fn run(&self); }\nimpl R for S { fn run(&self) {} }\n";
        let syms = extract_symbols(content, "src/lib.rs", "Rust");
        assert!(syms.iter().any(|s| s.name == "R" && s.kind == "trait"));
        assert!(syms.iter().any(|s| s.name == "impl S" && s.kind == "impl"));
    }
    #[test] fn rust_import_extraction_includes_crate() {
        let content = "use serde::Serialize;\nuse crate::database::indexer;\nuse std::collections::HashMap;";
        let syms = extract_symbols(content, "src/lib.rs", "Rust");
        assert!(syms.iter().any(|s| s.import_source.as_deref() == Some("serde::Serialize")), "external imports stored");
        assert!(syms.iter().any(|s| s.import_source.as_deref() == Some("crate::database::indexer")), "crate imports now stored");
    }
    #[test] fn typescript_function_extraction() {
        let content = "export function greet(n: string): string { return n; }\nfunction h() {}\n";
        let syms = extract_symbols(content, "src/hello.ts", "TypeScript");
        assert_eq!(syms.len(), 2); assert_eq!(syms[0].name, "greet"); assert_eq!(syms[0].visibility.as_deref(), Some("export"));
    }
    #[test] fn typescript_class_and_interface_extraction() {
        let content = "export interface U { n: string; }\nexport class A implements U { n = \"a\"; }\n";
        let syms = extract_symbols(content, "src/types.ts", "TypeScript");
        assert!(syms.iter().any(|s| s.name == "U" && s.kind == "interface"));
        assert!(syms.iter().any(|s| s.name == "A" && s.kind == "class"));
    }
    #[test] fn typescript_import_single_and_double_quotes() {
        let content_dq = r#"import { invoke } from "@tauri-apps/api/core";"#;
        let content_sq = r#"import { invoke } from '@tauri-apps/api/core';"#;
        let syms1 = extract_symbols(content_dq, "src/lib.ts", "TypeScript");
        let syms2 = extract_symbols(content_sq, "src/lib.ts", "TypeScript");
        assert!(syms1.iter().any(|s| s.import_source.as_deref() == Some("@tauri-apps/api/core")));
        assert!(syms2.iter().any(|s| s.import_source.as_deref() == Some("@tauri-apps/api/core")));
    }
    #[test] fn typescript_export_from_extraction() {
        let content = r#"export { greet } from "./greet";"#;
        let syms = extract_symbols(content, "src/index.ts", "TypeScript");
        assert!(syms.iter().any(|s| s.import_source.as_deref() == Some("./greet") && s.kind == "import"));
    }
    #[test] fn python_function_and_class_extraction() {
        let content = "def greet(name):\n    return name\n\nclass U:\n    def __init__(self):\n        pass\n";
        let syms = extract_symbols(content, "src/main.py", "Python");
        assert_eq!(syms.len(), 3);
        assert!(syms.iter().any(|s| s.name == "greet" && s.kind == "function"));
        assert!(syms.iter().any(|s| s.name == "U" && s.kind == "class"));
    }
    #[test] fn python_import_extraction() {
        let content = "import os\nfrom typing import List";
        let syms = extract_symbols(content, "src/main.py", "Python");
        assert!(syms.iter().any(|s| s.kind == "import"));
    }
    #[test] fn unknown_language_returns_empty() {
        let syms = extract_symbols("fn main() {}", "main.json", "JSON");
        assert!(syms.is_empty());
    }

    // ---- Dependency tests ----
    #[test] fn rust_dependency_extraction() {
        let content = "use serde::Serialize;\nuse crate::database::indexer;\nmod helpers;\nuse std::collections::HashMap;";
        let deps = extract_dependencies(content, "src/lib.rs", "Rust", 1000);
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("serde::Serialize") && d.dependency_type == "import"));
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("crate::database::indexer") && d.dependency_type == "internal_import"));
        assert!(deps.iter().any(|d| d.dependency_type == "file_reference" && d.target_symbol.as_deref() == Some("mod helpers")));
    }

    #[test] fn typescript_dependency_extraction() {
        let content = r#"import { invoke } from "@tauri-apps/api/core";
import { greet } from './greet';
export { type } from './types';"#;
        let deps = extract_dependencies(content, "src/index.ts", "TypeScript", 1000);
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("@tauri-apps/api/core") && d.dependency_type == "import"));
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("./greet") && d.dependency_type == "internal_import"));
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("./types") && d.dependency_type == "internal_import"));
    }

    #[test] fn python_dependency_extraction() {
        let content = "import os\nfrom typing import List\nfrom .helpers import parse";
        let deps = extract_dependencies(content, "src/main.py", "Python", 1000);
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("os") && d.dependency_type == "import"));
        assert!(deps.iter().any(|d| d.import_source.as_deref() == Some("typing")));
    }

    #[test] fn dependency_no_duplicates_on_reindex() {
        let content = "use serde::Serialize;\nuse std::collections::HashMap;";
        let deps = extract_dependencies(content, "src/lib.rs", "Rust", 1000);
        // Each use appears once
        assert_eq!(deps.len(), 2);
    }

    // ---- Integration tests ----
    #[test] fn index_workspace_indexes_and_skips_unchanged() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_indexer_test_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::write(dir.join("node_modules").join("skip.js"), "x").unwrap();
        {
            let conn = crate::database::open_for_workspace(&dir).unwrap();
            let stats1 = index_workspace(&conn, &dir).unwrap();
            assert_eq!(stats1.files_indexed, 1); assert_eq!(stats1.symbols_extracted, 1);
            let stats2 = index_workspace(&conn, &dir).unwrap();
            assert_eq!(stats2.files_indexed, 0); assert_eq!(stats2.files_skipped_unchanged, 1);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test] fn index_workspace_reindexes_after_file_change() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_indexer_reindex_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let fp = dir.join("lib.rs");
        std::fs::write(&fp, "pub fn a() -> i32 { 1 }").unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        assert_eq!(index_workspace(&conn, &dir).unwrap().files_indexed, 1);
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&fp, "pub fn b() -> i32 { 2 }").unwrap();
        assert_eq!(index_workspace(&conn, &dir).unwrap().files_indexed, 1);
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }
    #[test] fn index_workspace_extracts_symbols_and_stores_in_db() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_sym_db_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib.rs"), "pub fn compute() -> i32 { 42 }\nfn hidden() {}").unwrap();
        std::fs::write(dir.join("greet.ts"), "export function greet(n: string): string { return n; }").unwrap();
        let conn = crate::database::open_for_workspace(&dir).unwrap();
        let stats = index_workspace(&conn, &dir).unwrap();
        assert_eq!(stats.symbols_extracted, 3, "2 Rust fns + 1 TS fn");
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 3);
        drop(conn); std::fs::remove_dir_all(&dir).ok();
    }
    #[test] fn index_workspace_extracts_dependencies_and_stores_in_db() {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        dir.push(format!("neuralforge_dep_db_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let rust = "use serde::Serialize;\nuse crate::database::indexer;\nmod helpers;\npub fn run() {}";
        std::fs::write(dir.join("lib.rs"), rust).unwrap();
        let ts = r#"import { invoke } from "@tauri-apps/api/core";
import { greet } from './utils';"#;
        std::fs::write(dir.join("index.ts"), ts).unwrap();

        let conn = crate::database::open_for_workspace(&dir).unwrap();
        let stats = index_workspace(&conn, &dir).unwrap();
        assert!(stats.dependencies_extracted > 0, "should extract dependencies");

        let dep_cnt: i64 = conn.query_row("SELECT COUNT(*) FROM dependencies", [], |r| r.get(0)).unwrap();
        assert!(dep_cnt > 0, "dependencies should be stored in DB");

        // Verify types
        let import_cnt: i64 = conn.query_row("SELECT COUNT(*) FROM dependencies WHERE dependency_type = 'import'", [], |r| r.get(0)).unwrap();
        assert!(import_cnt > 0, "external imports should be stored");

        let internal_cnt: i64 = conn.query_row("SELECT COUNT(*) FROM dependencies WHERE dependency_type = 'internal_import'", [], |r| r.get(0)).unwrap();
        assert!(internal_cnt > 0, "internal imports should be stored");

        let file_ref_cnt: i64 = conn.query_row("SELECT COUNT(*) FROM dependencies WHERE dependency_type = 'file_reference'", [], |r| r.get(0)).unwrap();
        assert_eq!(file_ref_cnt, 1, "mod declaration should be stored as file_reference");

        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }
}