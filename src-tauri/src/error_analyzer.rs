use crate::planning_engine::{TaskPlan, Subtask};
use crate::terminal_executor::ExecutionResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReport {
    pub failures: Vec<DiagnosticFailure>,
    pub analysis_summary: String,
    pub retry_suggested: bool,
    pub max_retries_reached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticFailure {
    pub category: FailureCategory,
    pub severity: FailureSeverity,
    pub file_name: Option<String>,
    pub line_number: Option<usize>,
    pub column_number: Option<usize>,
    pub error_code: Option<String>,
    pub raw_message: String,
    pub suggested_fix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailureCategory {
    CompilationError,
    TypeCheckError,
    MissingImport,
    MissingDependency,
    BuildConfiguration,
    RuntimePanic,
    TestFailure,
    PermissionError,
    FileSystemError,
    NetworkError,
    Timeout,
    Unknown,
}

impl FailureCategory {
    pub fn is_recoverable(&self) -> bool {
        !matches!(self, FailureCategory::NetworkError | FailureCategory::PermissionError | FailureCategory::FileSystemError)
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            FailureCategory::CompilationError => "compilation_error",
            FailureCategory::TypeCheckError => "type_check_error",
            FailureCategory::MissingImport => "missing_import",
            FailureCategory::MissingDependency => "missing_dependency",
            FailureCategory::BuildConfiguration => "build_configuration",
            FailureCategory::RuntimePanic => "runtime_panic",
            FailureCategory::TestFailure => "test_failure",
            FailureCategory::PermissionError => "permission_error",
            FailureCategory::FileSystemError => "filesystem_error",
            FailureCategory::NetworkError => "network_error",
            FailureCategory::Timeout => "timeout",
            FailureCategory::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailureSeverity { Critical, High, Medium, Low, Info }

pub struct ErrorAnalyzer;

impl ErrorAnalyzer {
    pub fn analyze(exec_result: &ExecutionResult) -> FailureReport {
        let mut failures = Vec::new();
        let combined = format!("{}\n{}", exec_result.stdout, exec_result.stderr);
        failures.extend(Self::parse_cargo_diagnostics(&combined));
        failures.extend(Self::parse_typescript_diagnostics(&combined));
        if exec_result.was_cancelled {
            failures.push(DiagnosticFailure {
                category: FailureCategory::Timeout, severity: FailureSeverity::High,
                file_name: None, line_number: None, column_number: None, error_code: None,
                raw_message: format!("Command timed out after {}ms", exec_result.duration_ms),
                suggested_fix: "Increase timeout or optimize the build pipeline".to_string(),
            });
        }
        if exec_result.exit_code != 0 && failures.is_empty() {
            failures.push(DiagnosticFailure {
                category: FailureCategory::Unknown, severity: FailureSeverity::High,
                file_name: None, line_number: None, column_number: None,
                error_code: Some(format!("EXIT_CODE_{}", exec_result.exit_code)),
                raw_message: format!("Process exited with code {} but no diagnostics were parsed from output", exec_result.exit_code),
                suggested_fix: "Review stdout/stderr manually to identify the failure cause".to_string(),
            });
        }
        let retry_suggested = !failures.is_empty() && failures.iter().any(|f| f.category.is_recoverable());
        let analysis_summary = if failures.is_empty() {
            "No failures detected — verification passed".to_string()
        } else {
            format!("Found {} failure(s): {}", failures.len(), failures.iter().map(|f| f.category.as_str()).collect::<Vec<_>>().join(", "))
        };
        FailureReport { failures, analysis_summary, retry_suggested, max_retries_reached: false }
    }

    fn parse_cargo_diagnostics(output: &str) -> Vec<DiagnosticFailure> {
        let mut failures = Vec::new();
        let lines: Vec<&str> = output.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("permission denied") || trimmed.contains("Permission denied") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::PermissionError, severity: FailureSeverity::Critical,
                    file_name: Self::extract_location(trimmed).0, line_number: None, column_number: None, error_code: None,
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Check file permissions and ensure the process has write access".to_string(),
                });
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("error[") {
                if let Some(code_end) = rest.find(']') {
                    let ec = rest[..code_end].to_string();
                    let msg = rest[code_end+1..].trim().trim_start_matches(':').trim().to_string();
                    let (file_name, line_num, col_num) = if lines.get(i+1).map_or(false, |n| n.contains("-->")) {
                        Self::extract_location(lines[i+1].trim())
                    } else { Self::extract_location(trimmed) };
                    let category = if ec.starts_with("E06")||ec.starts_with("E05")||ec.starts_with("E03") { FailureCategory::TypeCheckError }
                        else if ec.starts_with("E04") { FailureCategory::MissingImport } else { FailureCategory::CompilationError };
                    let fix = Self::suggest_fix(&ec, &msg);
                    failures.push(DiagnosticFailure {
                        category, severity: FailureSeverity::High,
                        file_name, line_number: line_num, column_number: col_num, error_code: Some(ec), raw_message: msg, suggested_fix: fix,
                    });
                    continue;
                }
            }
            if trimmed.starts_with("error:") || trimmed.starts_with("error :") {
                let msg = trimmed.trim_start_matches("error:").trim_start_matches("error :").trim().to_string();
                let (file_name, line_num, col_num) = Self::extract_location(trimmed);
                failures.push(DiagnosticFailure {
                    category: FailureCategory::CompilationError, severity: FailureSeverity::Medium,
                    file_name, line_number: line_num, column_number: col_num, error_code: None,
                    raw_message: msg, suggested_fix: "Review the error message and fix the compilation issue".to_string(),
                });
                continue;
            }
            if trimmed.starts_with("test ") && trimmed.contains("FAILED") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::TestFailure, severity: FailureSeverity::Medium,
                    file_name: None, line_number: None, column_number: None, error_code: Some("TEST_FAILED".into()),
                    raw_message: trimmed.to_string(), suggested_fix: "Fix the failing test assertion or logic".to_string(),
                });
            }
            if trimmed.contains("panic!") || (trimmed.contains("thread '") && trimmed.contains("panicked")) {
                let (file_name, line_num, col_num) = Self::extract_location(trimmed);
                failures.push(DiagnosticFailure {
                    category: FailureCategory::RuntimePanic, severity: FailureSeverity::Critical,
                    file_name, line_number: line_num, column_number: col_num, error_code: None,
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Investigate the panic location. Add error handling or fix the invariant violation.".to_string(),
                });
            }
        }
        failures
    }

    fn parse_typescript_diagnostics(output: &str) -> Vec<DiagnosticFailure> {
        let mut failures = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("error TS") {
                let rest = rest.trim();
                if let Some(ce) = rest.find(':') {
                    let ec = format!("TS{}", &rest[..ce]);
                    let msg = rest[ce+1..].trim().to_string();
                    let file = rest.find(':').and_then(|ci| {
                        let fp = &rest[..ci];
                        if fp.contains('(') { Some(fp.split('(').next().unwrap_or("").to_string()) } else { None }
                    });
                    failures.push(DiagnosticFailure {
                        category: FailureCategory::TypeCheckError, severity: FailureSeverity::Medium,
                        file_name: file, line_number: None, column_number: None, error_code: Some(ec),
                        raw_message: msg, suggested_fix: "Fix the TypeScript type error as indicated".to_string(),
                    });
                }
            }
            if trimmed.contains("Cannot find module") || trimmed.contains("could not resolve") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::MissingDependency, severity: FailureSeverity::High,
                    file_name: None, line_number: None, column_number: None, error_code: None,
                    raw_message: trimmed.to_string(), suggested_fix: "Install the missing dependency or fix the import path".to_string(),
                });
            }
        }
        failures
    }

    fn extract_location(line: &str) -> (Option<String>, Option<usize>, Option<usize>) {
        for chunk in line.split(|c: char| c==' '||c==':') {
            if chunk.contains(".rs")||chunk.contains(".ts")||chunk.contains(".tsx")||chunk.contains(".js")||chunk.contains(".py") {
                let f = if chunk.contains('(') { chunk.split('(').next().unwrap_or(chunk).to_string() } else { chunk.to_string() };
                return (Some(f), None, None);
            }
        }
        (None, None, None)
    }

    fn suggest_fix(code: &str, msg: &str) -> String {
        if code == "E0432" || code == "E0433" { "Add the missing `use` import statement".into() }
        else if code == "E0609" { "The field or method does not exist — check the type definition".into() }
        else if msg.contains("cannot find") { "Ensure the identifier is in scope or imported correctly".into() }
        else { "Review the error and fix the compilation issue".into() }
    }
}

// ── Retry Coordinator ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub require_approval: bool,
}
impl Default for RetryConfig {
    fn default() -> Self { Self { max_retries: 3, base_delay_ms: 1000, max_delay_ms: 30000, require_approval: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryState {
    pub task_id: String,
    pub attempt_count: u32,
    pub max_retries: u32,
    pub previous_failures: Vec<FailureReport>,
    pub last_delay_ms: u64,
    pub aborted: bool,
}
impl RetryState {
    pub fn new(task_id: &str, max_retries: u32) -> Self {
        Self { task_id: task_id.to_string(), attempt_count: 0, max_retries, previous_failures: vec![], last_delay_ms: 0, aborted: false }
    }
    pub fn can_retry(&self) -> bool { !self.aborted && self.attempt_count < self.max_retries }
    pub fn next_delay_ms(&self) -> u64 { (1000u64 * 2u64.pow(self.attempt_count)).min(30000) }
    pub fn record_failure(&mut self, report: &FailureReport) { self.attempt_count += 1; self.last_delay_ms = self.next_delay_ms(); self.previous_failures.push(report.clone()); }
    pub fn abort(&mut self, reason: &str) { self.aborted = true; let _ = reason; }
}

// ── Repair Context Builder ────────────────────────────────────

pub struct RepairContextBuilder;

impl RepairContextBuilder {
    pub fn build_repair_plan(original_plan: &TaskPlan, exec_result: &ExecutionResult, retry_state: &RetryState) -> TaskPlan {
        let report = ErrorAnalyzer::analyze(exec_result);
        let affected_files: Vec<String> = report.failures.iter().filter_map(|f| f.file_name.clone()).collect();
        let combined = if affected_files.is_empty() { original_plan.affected_files.clone() } else { affected_files };
        let subtasks: Vec<Subtask> = report.failures.iter().enumerate().map(|(i, failure)| Subtask {
            id: i,
            description: format!("[Retry {}/{}] {} in {:?}: {}", retry_state.attempt_count+1, retry_state.max_retries, failure.category.as_str(), failure.file_name.as_deref().unwrap_or("workspace"), failure.suggested_fix),
            dependencies: vec![],
            required_files: failure.file_name.as_ref().map(|n| vec![n.clone()]).unwrap_or_else(|| combined.clone()),
            expected_outcome: format!("Fix {} and re-verify", failure.category.as_str()),
            confidence_score: 0.85,
        }).collect();
        TaskPlan {
            task_description: format!("[Retry {}/{}] Repair plan for: {}", retry_state.attempt_count+1, retry_state.max_retries, original_plan.task_description),
            objective: format!("Repair failures: {}", report.analysis_summary),
            affected_files: combined,
            subtasks: if subtasks.is_empty() { vec![Subtask { id:0, description:"Review execution output manually".into(), dependencies:vec![], required_files:original_plan.affected_files.clone(), expected_outcome:"All errors resolved".into(), confidence_score:0.5 }] } else { subtasks },
            risks: report.failures.iter().map(|f| format!("{}: {}", f.category.as_str(), f.raw_message)).collect(),
            verification: original_plan.verification.clone(),
            unknown_information: vec![],
            confidence: 0.0,
            estimated_runtime_commands: 2,
            rollback_plan: String::new(),
            reasoning: "Auto-generated repair plan from error diagnostics".to_string(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning_engine::{TaskPlan, Subtask};
    use crate::terminal_executor::{ExecutionRequest, ExecutionResult};
    fn mr(stdout: &str, stderr: &str, exit_code: i32) -> ExecutionResult {
        ExecutionResult { request: ExecutionRequest{command:"cargo".into(),arguments:vec!["check".into()],working_directory:".".into(),timeout_seconds:30}, exit_code, stdout:stdout.into(), stderr:stderr.into(), started_at:0, finished_at:1000, duration_ms:1000, was_cancelled:false }
    }
    #[test] fn parses_cargo_error() { let r = mr("","error[E0308]: mismatched types\n  --> src/main.rs:10:5\n10 |     let x: i32 = \"hello\";\n   |                ^^^^^^^ expected i32",101); let rep = ErrorAnalyzer::analyze(&r); assert!(!rep.failures.is_empty()); assert!(rep.failures.iter().any(|f| f.category==FailureCategory::TypeCheckError)); }
    #[test] fn parses_test_failure() { let r = mr("test auth::tests::login ... FAILED","failures:\n    auth::tests::login",101); assert!(ErrorAnalyzer::analyze(&r).failures.iter().any(|f| f.category==FailureCategory::TestFailure)); }
    #[test] fn parses_timeout() { let mut r = mr("","",-1); r.was_cancelled=true; assert!(ErrorAnalyzer::analyze(&r).failures.iter().any(|f| f.category==FailureCategory::Timeout)); }
    #[test] fn retry_respects_limits() { let mut s = RetryState::new("t1",3); s.record_failure(&FailureReport{failures:vec![],analysis_summary:"x".into(),retry_suggested:true,max_retries_reached:false}); s.record_failure(&FailureReport{failures:vec![],analysis_summary:"x".into(),retry_suggested:true,max_retries_reached:false}); s.record_failure(&FailureReport{failures:vec![],analysis_summary:"x".into(),retry_suggested:true,max_retries_reached:false}); assert!(!s.can_retry()); }
    #[test] fn abort_stops() { let mut s = RetryState::new("t2",5); s.abort("disk"); assert!(!s.can_retry()); }
    #[test] fn backoff_grows() { let s = RetryState{task_id:"t".into(),attempt_count:0,max_retries:5,previous_failures:vec![],last_delay_ms:0,aborted:false}; let mut s2 = s.clone(); s2.attempt_count=2; assert!(s2.next_delay_ms() > s.next_delay_ms()); }
    #[test] fn repair_plan_files() { let r = mr("","error[E0609]: no field `foo`\n  --> src/config.rs:42:10",101); let orig = TaskPlan{task_description:"Add config field".into(),objective:"Add".into(),affected_files:vec!["src/config.rs".into()],subtasks:vec![],risks:vec![],verification:vec![],unknown_information:vec![],confidence:0.0,estimated_runtime_commands:0,rollback_plan:String::new(),reasoning:String::new()}; let state = RetryState::new("t1",3); let plan = RepairContextBuilder::build_repair_plan(&orig,&r,&state); assert!(plan.subtasks.iter().any(|s| s.description.contains("config.rs"))); }
    #[test] fn non_recoverable() { let r = mr("","error: could not write to disk — permission denied",1); let rep = ErrorAnalyzer::analyze(&r); assert!(!rep.failures.iter().any(|f| f.category.is_recoverable())); }
    #[test] fn missing_dep() { let r = mr("","error TS2307: Cannot find module './missing'",2); assert!(ErrorAnalyzer::analyze(&r).failures.iter().any(|f| f.category==FailureCategory::MissingDependency)); }
    #[test] fn empty_ok() { assert!(ErrorAnalyzer::analyze(&mr("Compiled","",0)).failures.is_empty()); }
}