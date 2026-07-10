use crate::core::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use specta::Type;

/// One node of a task DAG before it becomes an agent_tasks row: what to do,
/// where, and which sibling tasks must complete first. The existing
/// single-task executor consumes each node unchanged - the DAG layer only
/// decides ordering and gating, never how a node is applied or verified.
#[derive(Type, Serialize, Deserialize, Clone, Debug)]
pub struct PlannedTask {
    pub id: String,
    pub objective: String,
    pub file_path: String,
    pub depends_on: Vec<String>,
}

/// Dependency graph for one requirement's decomposition. `edges` holds
/// (task_id, depends_on_id) pairs - derived from the nodes' depends_on
/// lists so the two can never disagree.
#[derive(Serialize, Type, Clone, Debug)]
pub struct TaskDAG {
    pub nodes: Vec<PlannedTask>,
    pub edges: Vec<(String, String)>,
}

impl TaskDAG {
    pub fn from_nodes(nodes: Vec<PlannedTask>) -> Self {
        let edges = nodes
            .iter()
            .flat_map(|n| n.depends_on.iter().map(move |d| (n.id.clone(), d.clone())))
            .collect();
        TaskDAG { nodes, edges }
    }

    /// Rejects empty graphs, duplicate IDs, dangling dependency references
    /// (a task depending on an ID that isn't in the graph), self-loops, and
    /// cycles - all before any task row is written or any executor runs.
    pub fn validate(&self) -> AppResult<()> {
        if self.nodes.is_empty() {
            return Err(AppError::Provider("DAG has no tasks".to_string()));
        }
        let ids: std::collections::HashSet<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        if ids.len() != self.nodes.len() {
            return Err(AppError::Provider("DAG contains duplicate task IDs".to_string()));
        }
        for (task, dep) in &self.edges {
            if task == dep {
                return Err(AppError::Provider(format!("task {task} depends on itself")));
            }
            if !ids.contains(dep.as_str()) {
                return Err(AppError::Provider(format!("task {task} depends on unknown task {dep}")));
            }
        }
        self.topological_order().map(|_| ())
    }

    /// Kahn's algorithm: repeatedly peel off tasks with no unmet
    /// dependencies. If anything remains unpeeled, those tasks form a
    /// cycle - named in the error so the failure is diagnosable.
    pub fn topological_order(&self) -> AppResult<Vec<String>> {
        use std::collections::HashMap;
        let mut in_degree: HashMap<&str, usize> = self.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for (task, _dep) in &self.edges {
            if let Some(d) = in_degree.get_mut(task.as_str()) {
                *d += 1;
            }
        }

        // Seed with dependency-free tasks in declaration order (stable,
        // deterministic output for a given input).
        let mut ready: Vec<&str> = self
            .nodes
            .iter()
            .filter(|n| in_degree[n.id.as_str()] == 0)
            .map(|n| n.id.as_str())
            .collect();
        let mut order: Vec<String> = Vec::with_capacity(self.nodes.len());

        while let Some(id) = ready.first().copied() {
            ready.remove(0);
            order.push(id.to_string());
            for (task, dep) in &self.edges {
                if dep == id {
                    let d = in_degree.get_mut(task.as_str()).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        ready.push(task.as_str());
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            let stuck: Vec<&str> = self
                .nodes
                .iter()
                .map(|n| n.id.as_str())
                .filter(|id| !order.iter().any(|o| o == id))
                .collect();
            return Err(AppError::Provider(format!("DAG contains a cycle involving: {}", stuck.join(", "))));
        }
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, deps: &[&str]) -> PlannedTask {
        PlannedTask {
            id: id.to_string(),
            objective: format!("objective for {id}"),
            file_path: format!("{id}.rs"),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn cycle_a_b_a_is_rejected() {
        let dag = TaskDAG::from_nodes(vec![node("a", &["b"]), node("b", &["a"])]);
        let err = dag.validate().unwrap_err().to_string();
        assert!(err.contains("cycle"), "got: {err}");
    }

    #[test]
    fn self_loop_is_rejected() {
        let dag = TaskDAG::from_nodes(vec![node("a", &["a"])]);
        assert!(dag.validate().unwrap_err().to_string().contains("depends on itself"));
    }

    #[test]
    fn dangling_dependency_is_rejected() {
        let dag = TaskDAG::from_nodes(vec![node("a", &["ghost"])]);
        assert!(dag.validate().unwrap_err().to_string().contains("unknown task ghost"));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let dag = TaskDAG::from_nodes(vec![node("a", &[]), node("a", &[])]);
        assert!(dag.validate().unwrap_err().to_string().contains("duplicate"));
    }

    #[test]
    fn empty_dag_is_rejected() {
        assert!(TaskDAG::from_nodes(vec![]).validate().is_err());
    }

    #[test]
    fn three_task_chain_orders_topologically() {
        // c depends on b depends on a; declared out of order on purpose.
        let dag = TaskDAG::from_nodes(vec![node("c", &["b"]), node("a", &[]), node("b", &["a"])]);
        dag.validate().unwrap();
        assert_eq!(dag.topological_order().unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn diamond_orders_dependencies_before_dependents() {
        // d depends on b and c, which both depend on a.
        let dag = TaskDAG::from_nodes(vec![node("a", &[]), node("b", &["a"]), node("c", &["a"]), node("d", &["b", "c"])]);
        let order = dag.topological_order().unwrap();
        let pos = |id: &str| order.iter().position(|o| o == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }
}
