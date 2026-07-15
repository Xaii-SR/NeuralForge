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
}

/// A single ordered step in the plan with dependencies and expectations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: usize,
    pub description: String,
    pub dependencies: Vec<usize>,
    pub required_files: Vec<String>,
    pub expected_outcome: String,
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

        // Add heuristic-based system names
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

        // DFS-based cycle detection with proper visited tracking
        for start in 0..n {
            let mut color: Vec<u8> = vec![0; n]; // 0=white, 1=gray, 2=black
            if Self::dfs_cycle(start, &adj, &mut color) {
                return true;
            }
        }

        false
    }

    fn dfs_cycle(node: usize, adj: &[Vec<usize>], color: &mut [u8]) -> bool {
        if color[node] == 1 {
            return true; // back edge — cycle
        }
        if color[node] == 2 {
            return false; // already fully processed
        }
        color[node] = 1; // mark as in-progress
        for &dep in &adj[node] {
            if Self::dfs_cycle(dep, adj, color) {
                return true;
            }
        }
        color[node] = 2; // mark as done
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
        subtasks,
        risks: analysis.potential_risks.clone(),
        verification: vec!["cargo check".to_string(), "cargo test".to_string()],
        unknown_information: analysis.unknown_information.clone(),
    };

    let validation = PlanningEngine::validate_plan(&plan);
    if !validation.is_valid {
        return Err(format!(
            "Plan validation failed: {}",
            validation.warnings.join("; ")
        ));
    }

    // Surface the plan into AgentContext for downstream phases
    let steps: Vec<String> = plan.subtasks.iter()
        .map(|s| s.description.clone())
        .collect();
    crate::agent_controller::AgentController::plan(ctx, steps);

    Ok(plan)
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

    #[test]
    fn simple_task_planning() {
        let mut ctx = test_context("Fix authentication bug", vec!["src/auth.rs"]);
        let plan = plan_task(&mut ctx).unwrap();
        assert!(!plan.subtasks.is_empty(), "should have at least one subtask");
        assert_eq!(plan.affected_files.len(), 1);
        assert!(plan.objective.contains("authentication"));
    }

    #[test]
    fn multi_step_decomposition() {
        let mut ctx = test_context(
            "Refactor the database layer",
            vec!["src/db/mod.rs", "src/db/connection.rs", "src/db/query.rs"],
        );
        let plan = plan_task(&mut ctx).unwrap();
        assert!(plan.subtasks.len() >= 3, "should decompose into multiple subtasks");
        assert!(plan.subtasks.iter().any(|s| s.description.contains("Verify")));
    }

    #[test]
    fn missing_context_handling() {
        let mut ctx = test_context("Implement caching layer", vec![]);
        let result = plan_task(&mut ctx);
        assert!(result.is_err(), "should fail when no files are known");
    }

    #[test]
    fn impact_analysis_detects_risks() {
        let files: Vec<String> = vec!["src/main.rs".into(), "src/types.rs".into(), "src/handler.rs".into()];
        let plan = TaskPlan {
            task_description: "Add new route".into(),
            objective: "Add new route".into(),
            affected_files: files.clone(),
            subtasks: vec![
                Subtask { id: 0, description: "Modify main.rs".into(), dependencies: vec![], required_files: vec!["src/main.rs".into()], expected_outcome: "ok".into() },
                Subtask { id: 1, description: "Modify types.rs".into(), dependencies: vec![0], required_files: vec!["src/types.rs".into()], expected_outcome: "ok".into() },
            ],
            risks: vec![],
            verification: vec![],
            unknown_information: vec![],
        };
        let report = PlanningEngine::impact_analysis(&files, &plan);
        assert!(!report.regression_risks.is_empty());
        assert!(report.affected_components.iter().any(|c| c.contains("types")));
    }

    #[test]
    fn validation_detects_circular_deps() {
        let plan = TaskPlan {
            task_description: "test".into(),
            objective: "test".into(),
            affected_files: vec!["a.rs".into()],
            subtasks: vec![
                Subtask { id: 0, description: "A".into(), dependencies: vec![1], required_files: vec!["a.rs".into()], expected_outcome: "ok".into() },
                Subtask { id: 1, description: "B".into(), dependencies: vec![0], required_files: vec!["a.rs".into()], expected_outcome: "ok".into() },
            ],
            risks: vec![],
            verification: vec![],
            unknown_information: vec![],
        };
        let validation = PlanningEngine::validate_plan(&plan);
        assert!(!validation.is_valid, "should detect circular dependency");
        assert!(validation.warnings.iter().any(|w| w.to_lowercase().contains("circular")));
    }
}