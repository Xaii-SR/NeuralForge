use super::dag::{PlannedTask, TaskDAG};
use crate::core::errors::{AppError, AppResult};
use crate::governance::ledger::{self, LedgerEvent};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sprint 3 DAG decomposition. Deliberately a NEW module: the Phase 5
/// agent::planner (single-file LLM content planning) is untouched and is
/// still what generates each node's proposed content later in the flow.
/// This module only decides the SHAPE of the work - which files, in what
/// dependency order - and persists that shape as first-class rows.
///
/// Decomposition input is explicit (one spec per target file plus
/// dependency indices) rather than a raw LLM guess: the caller - UI or a
/// future LLM decomposition step - proposes the split, and this module
/// enforces that the split is a valid DAG before any row exists.
#[derive(Deserialize, Clone)]
pub struct DagTaskSpec {
    pub file_path: String,
    /// Optional per-node focus appended to the requirement's objective.
    pub note: Option<String>,
    /// Indices into the spec list of tasks this one depends on.
    pub depends_on: Vec<usize>,
}

#[derive(Serialize, Clone, Debug)]
pub struct TaskDagRecord {
    pub id: String,
    pub requirement_id: String,
    pub version: i64,
    pub created_at: i64,
    pub correlation_id: String,
    pub task_ids: Vec<String>,
    /// Topological execution order, computed once at plan time.
    pub execution_order: Vec<String>,
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// requirement -> Vec<PlannedTask> -> validated TaskDAG. Pure (no DB):
/// builds one node per spec, wiring dependency indices into task IDs.
pub fn decompose(requirement: &crate::governance::requirements::RequirementContract, specs: &[DagTaskSpec]) -> AppResult<TaskDAG> {
    let base_objective = crate::agent::objective_from_requirement(requirement);
    let ids: Vec<String> = specs.iter().map(|_| uuid::Uuid::new_v4().to_string()).collect();

    let mut nodes = Vec::with_capacity(specs.len());
    for (i, spec) in specs.iter().enumerate() {
        let mut depends_on = Vec::with_capacity(spec.depends_on.len());
        for &dep in &spec.depends_on {
            let dep_id = ids
                .get(dep)
                .ok_or_else(|| AppError::Provider(format!("task {i} depends on out-of-range spec index {dep}")))?;
            depends_on.push(dep_id.clone());
        }
        let objective = match &spec.note {
            Some(note) => format!("{base_objective}\n\nThis task's focus: {note}"),
            None => base_objective.clone(),
        };
        nodes.push(PlannedTask { id: ids[i].clone(), objective, file_path: spec.file_path.clone(), depends_on });
    }

    let dag = TaskDAG::from_nodes(nodes);
    dag.validate()?;
    Ok(dag)
}

/// Validates the decomposition, then persists the task_dags row and one
/// agent_tasks row per node (status PLANNING, original content read by the
/// caller), ledgering task_planned per node under the requirement's
/// correlation_id - the Sprint 2 chain simply grows more entries.
/// Validation failure means NOTHING is written: no dag row, no task rows.
pub fn plan_dag(
    conn: &Connection,
    requirement: &crate::governance::requirements::RequirementContract,
    specs: &[DagTaskSpec],
    original_contents: &[String],
) -> AppResult<TaskDagRecord> {
    if specs.len() != original_contents.len() {
        return Err(AppError::Provider("one original-content entry per task spec is required".to_string()));
    }
    let dag = decompose(requirement, specs)?;
    let execution_order = dag.topological_order()?;

    let dag_id = uuid::Uuid::new_v4().to_string();
    let created_at = now_secs();
    // Sprint 7: the DAG row, every task row, every membership stamp, and
    // every task_planned ledger event commit as one unit - a kill mid-plan
    // must never leave a partial DAG (which load_dag would then reject).
    crate::database::in_transaction(conn, |conn| {
    conn.execute(
        "INSERT INTO task_dags (id, requirement_id, version, created_at, correlation_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![dag_id, requirement.id, requirement.version, created_at, requirement.correlation_id],
    )
    .map_err(|e| AppError::Provider(format!("failed to create task DAG: {e}")))?;

    for (node, original) in dag.nodes.iter().zip(original_contents) {
        crate::agent::insert_task(
            conn,
            &node.id,
            &node.objective,
            crate::agent::task_type::EDIT_FILE,
            &node.file_path,
            crate::agent::status::PLANNING,
            original,
            "",
            "",
            Some(&requirement.id),
            Some(&requirement.correlation_id),
        )?;
        crate::agent::set_dag_membership(conn, &node.id, &dag_id, &node.depends_on)?;
        let _ = ledger::append(
            conn,
            LedgerEvent::TaskPlanned,
            Some(&requirement.correlation_id),
            Some(&requirement.id),
            Some(&node.id),
            serde_json::json!({
                "dag_id": dag_id,
                "file_path": node.file_path,
                "depends_on": node.depends_on,
            }),
        );
    }
    Ok(())
    })?;

    tracing::info!(target: "planning", event = "dag_planned", dag_id = %dag_id, requirement_id = %requirement.id, tasks = dag.nodes.len());
    Ok(TaskDagRecord {
        id: dag_id,
        requirement_id: requirement.id.clone(),
        version: requirement.version,
        created_at,
        correlation_id: requirement.correlation_id.clone(),
        task_ids: dag.nodes.iter().map(|n| n.id.clone()).collect(),
        execution_order,
    })
}

/// Reloads a persisted DAG from real rows and re-validates it. Orphan
/// gate: every task claiming this dag_id must belong to a task_dags row
/// that actually exists, and the reconstructed graph must still be
/// acyclic - a DB edited out from under us fails here, before execution.
pub fn load_dag(conn: &Connection, dag_id: &str) -> AppResult<(TaskDagRecord, TaskDAG)> {
    let (requirement_id, version, created_at, correlation_id): (String, i64, i64, String) = conn
        .query_row(
            "SELECT requirement_id, version, created_at, correlation_id FROM task_dags WHERE id = ?1",
            params![dag_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("task DAG {dag_id} - tasks referencing it are orphans")))?;

    let tasks = crate::agent::list_dag_tasks(conn, dag_id)?;
    let nodes: Vec<PlannedTask> = tasks
        .iter()
        .map(|t| PlannedTask {
            id: t.id.clone(),
            objective: t.objective.clone(),
            file_path: t.files.first().cloned().unwrap_or_default(),
            depends_on: t.depends_on.clone(),
        })
        .collect();
    let dag = TaskDAG::from_nodes(nodes);
    dag.validate()?;

    let execution_order = dag.topological_order()?;
    Ok((
        TaskDagRecord {
            id: dag_id.to_string(),
            requirement_id,
            version,
            created_at,
            correlation_id,
            task_ids: dag.nodes.iter().map(|n| n.id.clone()).collect(),
            execution_order,
        },
        dag,
    ))
}
