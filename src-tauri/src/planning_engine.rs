use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::agent_controller::AgentContext;

/// A validated, structured implementation plan produced by the Planning Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    pub task_description: String,
    pub objective: String,
    pub affected_files: Vec<String>,
    pub subtasks: Vec<Subtask>,
    pub risks: Vec<String>,
    pub verification: Vec<String>,
    pub unknown_information: Vec<String>,
    /// Estimated complexity of the plan (1-10).
    pub confidence: f64,
    /// Estimated runtime in commands (number of verification steps).
    pub estimated_runtime_commands: usize,
    /// Plan for rolling back changes on failure.
    pub rollback_plan: String,
    /// Human-readable reasoning for the plan.
    pub reasoning: String,
}

/// A single ordered step in the plan with dependencies and expectations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: usize,
    pub description: String,
    pub dependencies: Vec<usize>,
    pub required_files: Vec<String>,
    pub expected_outcome: String,
    /// Confidence score for this subtask (0.0-1.0).
    pub confidence_score: f64,
}

/// The Planning Engine decomposes user tasks into structured, validated plans.
pub struct PlanningEngine;

impl PlanningEngine {
    /// Phase 1: Task Analysis — extract objectives, affected systems, and unknowns.
    pub fn analyze_task(ctx: &AgentContext, user_request: &str) -> TaskAnalysis {
        let keywords: Vec<&str> = user_request
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 2)
            .collect();

        let objective = Self::derive_objective(user_request);
        let affected_systems = Self::detect_affected_systems(&ctx.relevant_files, &keywords);
        let unknowns = Self::identify_unknowns(&ctx.relevant_files, &keywords);
        let risks = Self::assess_risks(&ctx.relevant_files, &affected_systems);

        TaskAnalysis {
            objective,
            affected_systems,
            unknown_information: unknowns,
            potential_risks: risks,
        }
    }

    /// Phase 2: Decompose into ordered subtasks with dependencies.
    pub fn decompose_task(analysis: &TaskAnalysis, files: &[String]) -> Vec<Subtask> {
        let mut subtasks = Vec::new();
        let mut id = 0usize;

        // Core logic: each affected file gets a modification subtask
        for file in files {
            let mut deps: Vec<usize> = Vec::new();
            // If the file has imports (heuristic: "import" or "use" keywords), add root dependency
            if file.contains("mod") || file.contains("main") {
                deps.clear(); // root-level file
            } else if id > 0 {
                deps.push(id - 1); // sequential dependency
            }

            subtasks.push(Subtask {
                id,
                description: format!("Modify `{}` to implement the change", file),
                dependencies: deps.clone(),
                required_files: vec![file.clone()],
                expected_outcome: format!("`{}` is updated and compiles successfully", file),
                confidence_score: if id == 0 { 0.9 } else { 0.75 },
            });
            id += 1;
        }

        // Add verification steps last
        if !subtasks.is_empty() {
            let verify_id = id;
            subtasks.push(Subtask {
                id: verify_id,
                description: "Verify the changes compile and pass existing tests".to_string(),
                dependencies: (0..id).collect(),
                required_files: files.to_vec(),
                expected_outcome: "All tests pass, no regressions detected".to_string(),
                confidence_score: 0.85,
            });
        }

        subtasks
    }

    /// Phase 3: Impact Analysis — identify what could break.
    pub fn impact_analysis(files: &[String], plan: &TaskPlan) -> ImpactReport {
        let mut regression_risks = Vec::new();
        let mut affected_components = Vec::new();

        for file in files {
            let name = file.rsplit('/').next().unwrap_or(file);
            if name.contains("mod") || name.contains("lib") || name.contains("main") {
                regression_risks.push(format!("Changes to `{}` may affect downstream modules", file));
            }
            if name.contains("types") || name.contains("schema") || name.contains("interface") {
                affected_components.push(format!("Type/schema file `{}` — may cascade to all consumers", file));
            }
        }

        if plan.subtasks.len() > 3 {
            regression_risks.push(format!(
                "Large plan ({} subtasks) increases integration risk",
                plan.subtasks.len()
            ));
        }

        ImpactReport {
            affected_components,
            regression_risks,
            recommended_verification: vec![
                "cargo check".to_string(),
                "cargo test".to_string(),
                "Manual review of changed files".to_string(),
            ],
        }
    }

    /// Phase 4: Validate plan before execution.
    pub fn validate_plan(plan: &TaskPlan) -> PlanValidation {
        let mut warnings = Vec::new();
        let mut is_valid = true;

        if plan.affected_files.is_empty() {
            warnings.push("No affected files identified — plan may be incomplete".to_string());
            is_valid = false;
        }

        if plan.subtasks.is_empty() {
            warnings.push("No subtasks defined".to_string());
            is_valid = false;
        }

        for (i, subtask) in plan.subtasks.iter().enumerate() {
            if subtask.required_files.is_empty() {
                warnings.push(format!("Subtask {} has no required files", i));
                is_valid = false;
            }
            for dep in &subtask.dependencies {
                if *dep >= plan.subtasks.len() || *dep == i {
                    warnings.push(format!(
                        "Subtask {} has invalid dependency reference to task {}", i, dep
                    ));
                    is_valid = false;
                }
            }
        }

        // Circular dependency detection (simple DFS)
        if Self::has_circular_deps(&plan.subtasks) {
            warnings.push("Circular dependency detected among subtasks".to_string());
            is_valid = false;
        }

        PlanValidation {
            is_valid,
            warnings,
            recommendations: if is_valid {
                vec!["Plan is ready for execution".to_string()]
            } else {
                vec!["Address warnings before proceeding to execution".to_string()]
            },
        }
    }

    /// Compute overall plan confidence from subtask scores.
    pub fn compute_confidence(plan: &TaskPlan) -> f64 {
        if plan.subtasks.is_empty() { return 0.0; }
        let sum: f64 = plan.subtasks.iter().map(|s| s.confidence_score).sum();
        let avg = sum / plan.subtasks.len() as f64;
        // Penalize for large plans
        if plan.subtasks.len() > 5 { avg - 0.1 } else { avg }.max(0.0).min(1.0)
    }

    /// Generate a rollback plan description.
    pub fn generate_rollback_plan(plan: &TaskPlan) -> String {
        if plan.affected_files.is_empty() { return "No files were modified — no rollback needed".to_string(); }
        let file_list = plan.affected_files.join(", ");
        format!("Revert changes to files: {}. Use git checkout or manual undo following the patch backup files.", file_list)
    }

    // ── Private helpers ──

    fn derive_objective(request: &str) -> String {
        let cleaned = request.trim();
        if cleaned.len() > 200 {
            format!("{}...", &cleaned[..200])
        } else {
            cleaned.to_string()
        }
    }

    fn detect_affected_systems(files: &[String], keywords: &[&str]) -> Vec<String> {
        let mut systems: Vec<String> = files
            .iter()
            .filter(|f| keywords.iter().any(|kw| f.to_lowercase().contains(&kw.to_lowercase())))
            .cloned()
            .collect();

        let known_systems = ["auth", "database", "api", "ui", "config", "models", "routes", "handler", "service", "repository"];
        for sys in &known_systems {
            if keywords.contains(sys) || files.iter().any(|f| f.contains(sys)) {
                if !systems.iter().any(|s| s.contains(sys)) {
                    systems.push(format!("system:{}", sys));
                }
            }
        }

        systems
    }

    fn identify_unknowns(files: &[String], keywords: &[&str]) -> Vec<String> {
        let mut unknowns = Vec::new();

        if files.len() < 3 {
            unknowns.push("Limited file context — may need broader workspace analysis".to_string());
        }

        let common_unknowns = ["dependency", "upgrade", "migration", "compatibility", "deploy"];
        for uk in &common_unknowns {
            if keywords.contains(uk) {
                unknowns.push(format!("Task involves `{}` — verify current state before changing", uk));
            }
        }

        unknowns
    }

    fn assess_risks(files: &[String], systems: &[String]) -> Vec<String> {
        let mut risks = Vec::new();

        if systems.len() > 3 {
            risks.push("Multiple systems affected — increased integration risk".to_string());
        }

        if files.iter().any(|f| f.contains("main") || f.contains("index") || f.contains("app")) {
            risks.push("Entry-point files affected — changes may cascade broadly".to_string());
        }

        if files.is_empty() {
            risks.push("No files identified — plan may be operating blind".to_string());
        }

        risks
    }

    fn has_circular_deps(subtasks: &[Subtask]) -> bool {
        let n = subtasks.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for s in subtasks {
            if s.id < n {
                for dep in &s.dependencies {
                    if *dep < n {
                        adj[s.id].push(*dep);
                    }
                }
            }
        }

        for start in 0..n {
            let mut color: Vec<u8> = vec![0; n];
            if Self::dfs_cycle(start, &adj, &mut color) {
                return true;
            }
        }

        false
    }

    fn dfs_cycle(node: usize, adj: &[Vec<usize>], color: &mut [u8]) -> bool {
        if color[node] == 1 { return true; }
        if color[node] == 2 { return false; }
        color[node] = 1;
        for &dep in &adj[node] {
            if Self::dfs_cycle(dep, adj, color) { return true; }
        }
        color[node] = 2;
        false
    }
}

/// Output of task analysis phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    pub objective: String,
    pub affected_systems: Vec<String>,
    pub unknown_information: Vec<String>,
    pub potential_risks: Vec<String>,
}

/// Output of impact analysis phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub affected_components: Vec<String>,
    pub regression_risks: Vec<String>,
    pub recommended_verification: Vec<String>,
}

/// Result of plan validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidation {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// High-level entry point: run the full planning pipeline and store results in AgentContext.
pub fn plan_task(ctx: &mut AgentContext) -> Result<TaskPlan, String> {
    let analysis = PlanningEngine::analyze_task(ctx, &ctx.user_task);
    let subtasks = PlanningEngine::decompose_task(&analysis, &ctx.relevant_files);

    let plan = TaskPlan {
        task_description: ctx.user_task.clone(),
        objective: analysis.objective.clone(),
        affected_files: ctx.relevant_files.clone(),
        subtasks: subtasks.clone(),
        risks: analysis.potential_risks.clone(),
        verification: vec!["cargo check".to_string(), "cargo test".to_string()],
        unknown_information: analysis.unknown_information.clone(),
        confidence: 0.0, // filled after validation
        estimated_runtime_commands: subtasks.len() + 1,
        rollback_plan: String::new(), // filled below
        reasoning: format!(
            "Generated plan with {} subtask(s) affecting {} file(s) in the {} system(s).",
            subtasks.len(), ctx.relevant_files.len(), analysis.affected_systems.len()
        ),
    };

    let validation = PlanningEngine::validate_plan(&plan);
    if !validation.is_valid {
        return Err(format!("Plan validation failed: {}", validation.warnings.join("; ")));
    }

    let mut finalized = plan;
    finalized.confidence = PlanningEngine::compute_confidence(&finalized);
    finalized.rollback_plan = PlanningEngine::generate_rollback_plan(&finalized);

    let steps: Vec<String> = finalized.subtasks.iter().map(|s| s.description.clone()).collect();
    crate::agent_controller::AgentController::plan(ctx, steps);

    Ok(finalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_context(task: &str, files: Vec<&str>) -> AgentContext {
        let mut ctx = AgentContext::new("test-task", task, PathBuf::from("/tmp"));
        ctx.relevant_files = files.into_iter().map(|s| s.to_string()).collect();
        ctx
    }

    #[test] fn simple_task_planning() { let mut ctx = test_context("Fix authentication bug", vec!["src/auth.rs"]); let plan = plan_task(&mut ctx).unwrap(); assert!(!plan.subtasks.is_empty()); assert!(plan.confidence > 0.0); assert!(!plan.rollback_plan.is_empty()); }
    #[test] fn multi_step_decomposition() { let mut ctx = test_context("Refactor the database layer", vec!["src/db/mod.rs", "src/db/connection.rs", "src/db/query.rs"]); let plan = plan_task(&mut ctx).unwrap(); assert!(plan.subtasks.len() >= 3); }
    #[test] fn missing_context_handling() { let mut ctx = test_context("Implement caching layer", vec![]); assert!(plan_task(&mut ctx).is_err()); }
    #[test] fn impact_analysis_detects_risks() { let files: Vec<String> = vec!["src/main.rs".into(), "src/types.rs".into(), "src/handler.rs".into()]; let plan = TaskPlan { task_description: "t".into(), objective: "o".into(), affected_files: files.clone(), subtasks: vec![Subtask{id:0,description:"d".into(),dependencies:vec![],required_files:vec!["a".into()],expected_outcome:"ok".into(),confidence_score:0.9},Subtask{id:1,description:"d2".into(),dependencies:vec![0],required_files:vec!["b".into()],expected_outcome:"ok".into(),confidence_score:0.9}], risks:vec![],verification:vec![],unknown_information:vec![],confidence:0.9,estimated_runtime_commands:2,rollback_plan:"Revert".into(),reasoning:"t".into()}; let report = PlanningEngine::impact_analysis(&files, &plan); assert!(!report.regression_risks.is_empty()); }
    #[test] fn validation_detects_circular_deps() { let plan = TaskPlan { task_description:"t".into(),objective:"o".into(),affected_files:vec!["a.rs".into()],subtasks:vec![Subtask{id:0,description:"A".into(),dependencies:vec![1],required_files:vec!["a.rs".into()],expected_outcome:"ok".into(),confidence_score:0.9},Subtask{id:1,description:"B".into(),dependencies:vec![0],required_files:vec!["a.rs".into()],expected_outcome:"ok".into(),confidence_score:0.9}],risks:vec![],verification:vec![],unknown_information:vec![],confidence:0.0,estimated_runtime_commands:2,rollback_plan:String::new(),reasoning:String::new()}; let v = PlanningEngine::validate_plan(&plan); assert!(!v.is_valid); }
    #[test] fn confidence_penalizes_large_plans() { let mut plan = TaskPlan { task_description:"t".into(),objective:"o".into(),affected_files:vec!["a.rs".into()],subtasks:Vec::new(),risks:vec![],verification:vec![],unknown_information:vec![],confidence:0.0,estimated_runtime_commands:0,rollback_plan:String::new(),reasoning:String::new()}; for i in 0..10 { plan.subtasks.push(Subtask{id:i,description:"s".into(),dependencies:vec![],required_files:vec!["a.rs".into()],expected_outcome:"ok".into(),confidence_score:1.0}); } assert!(PlanningEngine::compute_confidence(&plan) <= 0.9); }
}