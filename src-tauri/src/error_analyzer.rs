use crate::planning_engine::{TaskPlan, Subtask};
use crate::terminal_executor::ExecutionResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Structured failure analysis produced from command output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReport {
    pub failures: Vec<DiagnosticFailure>,
    pub analysis_summary: String,
    pub retry_suggested: bool,
    pub max_retries_reached: bool,
}

/// A single classified failure with extracted diagnostics.
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

/// Structured failure categories for recovery routing.
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
        match self {
            FailureCategory::NetworkError | FailureCategory::PermissionError | FailureCategory::FileSystemError => false,
            _ => true,
        }
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
pub enum FailureSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

// ═══════════════════════════════════════════════════════════════
// Error Analyzer — Parses command output into structured failures
// ═══════════════════════════════════════════════════════════════

pub struct ErrorAnalyzer;

impl ErrorAnalyzer {
    /// Parse combined stdout and stderr from an ExecutionResult into a structured FailureReport.
    pub fn analyze(exec_result: &ExecutionResult) -> FailureReport {
        let mut failures = Vec::new();

        let combined = format!("{}\n{}", exec_result.stdout, exec_result.stderr);

        // Parse cargo compiler errors (Rust)
        failures.extend(Self::parse_cargo_diagnostics(&combined));

        // Parse TypeScript/npm diagnostics
        failures.extend(Self::parse_typescript_diagnostics(&combined));

        // Check for timeouts
        if exec_result.was_cancelled {
            failures.push(DiagnosticFailure {
                category: FailureCategory::Timeout,
                severity: FailureSeverity::High,
                file_name: None,
                line_number: None,
                column_number: None,
                error_code: None,
                raw_message: format!("Command timed out after {}ms", exec_result.duration_ms),
                suggested_fix: "Increase timeout or optimize the build pipeline".to_string(),
            });
        }

        // Check for non-zero exit without diagnostics
        if exec_result.exit_code != 0 && failures.is_empty() {
            failures.push(DiagnosticFailure {
                category: FailureCategory::Unknown,
                severity: FailureSeverity::High,
                file_name: None,
                line_number: None,
                column_number: None,
                error_code: Some(format!("EXIT_CODE_{}", exec_result.exit_code)),
                raw_message: format!("Process exited with code {} but no diagnostics were parsed from output", exec_result.exit_code),
                suggested_fix: "Review stdout/stderr manually to identify the failure cause".to_string(),
            });
        }

        let retry_suggested = !failures.is_empty() && failures.iter().any(|f| f.category.is_recoverable());
        let analysis_summary = if failures.is_empty() {
            "No failures detected — verification passed".to_string()
        } else {
            format!(
                "Found {} failure(s): {}",
                failures.len(),
                failures.iter().map(|f| f.category.as_str()).collect::<Vec<_>>().join(", ")
            )
        };

        FailureReport {
            failures,
            analysis_summary,
            retry_suggested,
            max_retries_reached: false,
        }
    }

    /// Parse Rust compiler (cargo) error diagnostics.
    fn parse_cargo_diagnostics(output: &str) -> Vec<DiagnosticFailure> {
        let mut failures = Vec::new();
        let lines: Vec<&str> = output.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Permission / filesystem errors (detect before generic "error:")
            if trimmed.contains("permission denied") || trimmed.contains("Permission denied") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::PermissionError,
                    severity: FailureSeverity::Critical,
                    file_name: Self::extract_location(trimmed).0,
                    line_number: None,
                    column_number: None,
                    error_code: None,
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Check file permissions and ensure the process has write access".to_string(),
                });
                continue;
            }

            // Pattern: error[E####]: message
            if let Some(rest) = trimmed.strip_prefix("error[") {
                if let Some(code_end) = rest.find(']') {
                    let error_code = rest[..code_end].to_string();
                    let message = rest[code_end + 1..].trim().trim_start_matches(':').trim().to_string();

                    // Look at the current line AND the next line for --> file:line:col
                    let (file_name, line_num, col_num) = if lines.get(i + 1).map_or(false, |next| next.contains("-->")) {
                        Self::extract_location(lines[i + 1].trim())
                    } else {
                        Self::extract_location(trimmed)
                    };

                    let category = if error_code.starts_with("E06") || error_code.starts_with("E05") || error_code.starts_with("E03") {
                        FailureCategory::TypeCheckError
                    } else if error_code.starts_with("E04") {
                        FailureCategory::MissingImport
                    } else {
                        FailureCategory::CompilationError
                    };

                    let suggested_fix = Self::suggest_fix_for_rust_error(&error_code, &message);

                    failures.push(DiagnosticFailure {
                        category,
                        severity: FailureSeverity::High,
                        file_name,
                        line_number: line_num,
                        column_number: col_num,
                        error_code: Some(error_code),
                        raw_message: message,
                        suggested_fix,
                    });
                    continue;
                }
            }

            // Pattern: error: message (without error code)
            if trimmed.starts_with("error:") || trimmed.starts_with("error :") {
                let message = trimmed.trim_start_matches("error:").trim_start_matches("error :").trim().to_string();
                let (file_name, line_num, col_num) = Self::extract_location(trimmed);

                failures.push(DiagnosticFailure {
                    category: FailureCategory::CompilationError,
                    severity: FailureSeverity::Medium,
                    file_name,
                    line_number: line_num,
                    column_number: col_num,
                    error_code: None,
                    raw_message: message,
                    suggested_fix: "Review the error message and fix the compilation issue".to_string(),
                });
                continue;
            }

            // Test failures
            if trimmed.starts_with("test ") && trimmed.contains("FAILED") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::TestFailure,
                    severity: FailureSeverity::Medium,
                    file_name: None,
                    line_number: None,
                    column_number: None,
                    error_code: Some("TEST_FAILED".to_string()),
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Fix the failing test assertion or logic".to_string(),
                });
            }

            // Runtime panics
            if trimmed.contains("panic!") || trimmed.contains("thread '") && trimmed.contains("panicked") {
                let (file_name, line_num, col_num) = Self::extract_location(trimmed);
                failures.push(DiagnosticFailure {
                    category: FailureCategory::RuntimePanic,
                    severity: FailureSeverity::Critical,
                    file_name,
                    line_number: line_num,
                    column_number: col_num,
                    error_code: None,
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Investigate the panic location. Add error handling or fix the invariant violation.".to_string(),
                });
            }
        }

        failures
    }

    /// Parse TypeScript/npm diagnostic output.
    fn parse_typescript_diagnostics(output: &str) -> Vec<DiagnosticFailure> {
        let mut failures = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();

            // TS error: file.ts(line,col): error TS####: message
            if let Some(rest) = trimmed.strip_prefix("error TS") {
                let rest = rest.trim();
                if let Some(code_end) = rest.find(':') {
                    let error_code = format!("TS{}", &rest[..code_end]);
                    let message = rest[code_end + 1..].trim().to_string();

                    // Extract file location
                    let file = if let Some(colon_idx) = rest.find(':') {
                        let file_part = &rest[..colon_idx];
                        if file_part.contains('(') {
                            Some(file_part.split('(').next().unwrap_or("").to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    failures.push(DiagnosticFailure {
                        category: FailureCategory::TypeCheckError,
                        severity: FailureSeverity::Medium,
                        file_name: file,
                        line_number: None,
                        column_number: None,
                        error_code: Some(error_code),
                        raw_message: message,
                        suggested_fix: "Fix the TypeScript type error as indicated".to_string(),
                    });
                }
            }

            // Missing dependency detection
            if trimmed.contains("Cannot find module") || trimmed.contains("could not resolve") {
                failures.push(DiagnosticFailure {
                    category: FailureCategory::MissingDependency,
                    severity: FailureSeverity::High,
                    file_name: None,
                    line_number: None,
                    column_number: None,
                    error_code: None,
                    raw_message: trimmed.to_string(),
                    suggested_fix: "Install the missing dependency or fix the import path".to_string(),
                });
            }
        }

        failures
    }

    /// Extract file:line:col from compiler diagnostics.
    fn extract_location(line: &str) -> (Option<String>, Option<usize>, Option<usize>) {
        // Look for patterns like "src/main.rs:10:5" or "src/main.rs(10,5)"
        let parts: Vec<&str> = line.split(|c: char| c == ' ' || c == ':').collect();
        for chunk in &parts {
            if chunk.contains(".rs") || chunk.contains(".ts") || chunk.contains(".tsx") || chunk.contains(".js") || chunk.contains(".py") {
                let file = if chunk.contains('(') {
                    chunk.split('(').next().unwrap_or(chunk).to_string()
                } else {
                    chunk.to_string()
                };
                return (Some(file), None, None);
            }
        }
        (None, None, None)
    }

    /// Suggest a fix for known Rust error codes.
    fn suggest_fix_for_rust_error(code: &str, message: &str) -> String {
        if code == "E0432" || code == "E0433" {
            "Add the missing `use` import statement".to_string()
        } else if code == "E0609" {
            "The field or method does not exist — check the type definition".to_string()
        } else if message.contains("cannot find") {
            "Ensure the identifier is in scope or imported correctly".to_string()
        } else {
            "Review the error and fix the compilation issue".to_string()
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Retry Coordinator — bounded exponential backoff
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub require_approval: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            require_approval: true,
        }
    }
}

/// Tracks retry state for a given task.
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
        Self {
            task_id: task_id.to_string(),
            attempt_count: 0,
            max_retries,
            previous_failures: Vec::new(),
            last_delay_ms: 0,
            aborted: false,
        }
    }

    /// Returns whether another retry is allowed.
    pub fn can_retry(&self) -> bool {
        !self.aborted && self.attempt_count < self.max_retries
    }

    /// Calculate the next back-off delay using exponential backoff.
    pub fn next_delay_ms(&self) -> u64 {
        let delay = self.base_delay_ms() * 2u64.pow(self.attempt_count);
        delay.min(self.max_delay_ms())
    }

    fn base_delay_ms(&self) -> u64 { 1000 }
    fn max_delay_ms(&self) -> u64 { 30000 }

    /// Records a failure and increments the attempt counter.
    pub fn record_failure(&mut self, report: &FailureReport) {
        self.attempt_count += 1;
        self.last_delay_ms = self.next_delay_ms();
        self.previous_failures.push(report.clone());
    }

    /// Abort retries — unrecoverable failure detected.
    pub fn abort(&mut self, reason: &str) {
        self.aborted = true;
    }
}

// ═══════════════════════════════════════════════════════════════
// Repair Context Builder
// ═══════════════════════════════════════════════════════════════

/// Feeds structured failure data back into the Planning Engine for repair.
pub struct RepairContextBuilder;

impl RepairContextBuilder {
    /// Builds a new TaskPlan based on the previous plan and current failures.
    pub fn build_repair_plan(
        original_plan: &TaskPlan,
        exec_result: &ExecutionResult,
        retry_state: &RetryState,
    ) -> TaskPlan {
        let report = ErrorAnalyzer::analyze(exec_result);

        let affected_files: Vec<String> = report
            .failures
            .iter()
            .filter_map(|f| f.file_name.clone())
            .collect();

        let combined_files = if affected_files.is_empty() {
            original_plan.affected_files.clone()
        } else {
            affected_files
        };

        let subtasks: Vec<Subtask> = report
            .failures
            .iter()
            .enumerate()
            .map(|(i, failure)| Subtask {
                id: i,
                description: format!(
                    "[Retry {}/{}] {} in {:?}: {}",
                    retry_state.attempt_count + 1,
                    retry_state.max_retries,
                    failure.category.as_str(),
                    failure.file_name.as_deref().unwrap_or("workspace"),
                    failure.suggested_fix
                ),
                dependencies: vec![],
                required_files: if let Some(ref file) = failure.file_name {
                    vec![file.clone()]
                } else {
                    combined_files.clone()
                },
                expected_outcome: format!("Fix {} and re-verify", failure.category.as_str()),
            })
            .collect();

        TaskPlan {
            task_description: format!(
                "[Retry {}/{}] Repair plan for original task: {}",
                retry_state.attempt_count + 1,
                retry_state.max_retries,
                original_plan.task_description
            ),
            objective: format!("Repair failures detected during execution: {}", report.analysis_summary),
            affected_files: combined_files,
            subtasks: if subtasks.is_empty() {
                vec![Subtask {
                    id: 0,
                    description: "Review execution output and fix errors manually".to_string(),
                    dependencies: vec![],
                    required_files: original_plan.affected_files.clone(),
                    expected_outcome: "All errors resolved".to_string(),
                }]
            } else {
                subtasks
            },
            risks: report
                .failures
                .iter()
                .map(|f| format!("{}: {}", f.category.as_str(), f.raw_message))
                .collect(),
            verification: original_plan.verification.clone(),
            unknown_information: Vec::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning_engine::{TaskPlan, Subtask};
    use crate::terminal_executor::{ExecutionRequest, ExecutionResult};

    fn make_result(stdout: &str, stderr: &str, exit_code: i32) -> ExecutionResult {
        ExecutionResult {
            request: ExecutionRequest {
                command: "cargo".into(),
                arguments: vec!["check".into()],
                working_directory: ".".into(),
                timeout_seconds: 30,
            },
            exit_code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            started_at: 0,
            finished_at: 1000,
            duration_ms: 1000,
            was_cancelled: false,
        }
    }

    #[test]
    fn parses_cargo_compilation_error() {
        let result = make_result(
            "",
            "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n   |\n10 |     let x: i32 = \"hello\";\n   |                ^^^^^^^ expected i32, found &str",
            101,
        );
        let report = ErrorAnalyzer::analyze(&result);
        assert!(!report.failures.is_empty());
        assert!(report.failures.iter().any(|f| f.category == FailureCategory::TypeCheckError));
        assert!(report.retry_suggested);
    }

    #[test]
    fn parses_test_failure() {
        let result = make_result(
            "test auth::tests::login ... FAILED",
            "failures:\n    auth::tests::login",
            101,
        );
        let report = ErrorAnalyzer::analyze(&result);
        assert!(report.failures.iter().any(|f| f.category == FailureCategory::TestFailure));
    }

    #[test]
    fn parses_timeout() {
        let mut result = make_result("", "", -1);
        result.was_cancelled = true;
        let report = ErrorAnalyzer::analyze(&result);
        assert!(report.failures.iter().any(|f| f.category == FailureCategory::Timeout));
    }

    #[test]
    fn retry_coordinator_respects_limits() {
        let mut state = RetryState::new("task-1", 3);
        assert!(state.can_retry());
        state.record_failure(&FailureReport {
            failures: vec![],
            analysis_summary: "test".into(),
            retry_suggested: true,
            max_retries_reached: false,
        });
        state.record_failure(&FailureReport { failures: vec![], analysis_summary: "test".into(), retry_suggested: true, max_retries_reached: false });
        state.record_failure(&FailureReport { failures: vec![], analysis_summary: "test".into(), retry_suggested: true, max_retries_reached: false });
        assert!(!state.can_retry(), "should exhaust retries at 3");
    }

    #[test]
    fn retry_coordinator_abort_stops() {
        let mut state = RetryState::new("task-2", 5);
        state.abort("unrecoverable disk error");
        assert!(!state.can_retry());
    }

    #[test]
    fn exponential_backoff_grows() {
        let state = RetryState { task_id: "t".into(), attempt_count: 0, max_retries: 5, previous_failures: vec![], last_delay_ms: 0, aborted: false };
        let d1 = state.next_delay_ms();
        let mut s2 = state.clone();
        s2.attempt_count = 2;
        let d2 = s2.next_delay_ms();
        assert!(d2 > d1, "delay should grow with retries: {} vs {}", d2, d1);
    }

    #[test]
    fn repair_plan_includes_failure_files() {
        let result = make_result(
            "",
            "error[E0609]: no field `foo` on type `Config`\n  --> src/config.rs:42:10",
            101,
        );
        let original = TaskPlan {
            task_description: "Add config field".into(),
            objective: "Add field".into(),
            affected_files: vec!["src/config.rs".into()],
            subtasks: vec![],
            risks: vec![],
            verification: vec![],
            unknown_information: vec![],
        };
        let state = RetryState::new("task-1", 3);
        let repair = RepairContextBuilder::build_repair_plan(&original, &result, &state);
        assert!(repair.affected_files.contains(&"src/config.rs".to_string()));
        assert!(repair.subtasks.iter().any(|s| s.description.contains("config.rs")));
    }

    #[test]
    fn non_recoverable_failures_not_suggested() {
        let result = make_result("", "error: could not write to disk — permission denied", 1);
        let report = ErrorAnalyzer::analyze(&result);
        assert!(!report.failures.iter().any(|f| f.category.is_recoverable()));
    }

    #[test]
    fn missing_dependency_detection() {
        let result = make_result("", "error TS2307: Cannot find module './missing' or its corresponding type declarations.", 2);
        let report = ErrorAnalyzer::analyze(&result);
        assert!(report.failures.iter().any(|f| f.category == FailureCategory::MissingDependency));
    }

    #[test]
    fn empty_output_no_failures() {
        let result = make_result("Compiled successfully", "", 0);
        let report = ErrorAnalyzer::analyze(&result);
        assert!(report.failures.is_empty());
    }
}