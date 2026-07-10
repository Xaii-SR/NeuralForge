import { invoke } from "@tauri-apps/api/core";

export interface RequirementContract {
  id: string;
  version: number;
  title: string;
  intent: string;
  acceptance_criteria: string[];
  status: string;
  correlation_id: string;
  created_at: number;
  updated_at: number;
  created_by: string;
}

export interface RequirementHistoryEntry {
  requirement_id: string;
  version: number;
  status: string;
  title: string;
  intent: string;
  acceptance_criteria: string[];
  changed_at: number;
}

export function createRequirement(title: string, intent: string, acceptanceCriteria: string[]): Promise<RequirementContract> {
  return invoke("create_requirement", { title, intent, acceptanceCriteria });
}

export function updateRequirement(id: string, title: string, intent: string, acceptanceCriteria: string[]): Promise<RequirementContract> {
  return invoke("update_requirement", { id, title, intent, acceptanceCriteria });
}

export function setRequirementStatus(id: string, status: string): Promise<RequirementContract> {
  return invoke("set_requirement_status", { id, status });
}

export function getRequirement(id: string): Promise<RequirementContract> {
  return invoke("get_requirement", { id });
}

export function listRequirements(): Promise<RequirementContract[]> {
  return invoke("list_requirements");
}

export function getRequirementHistory(id: string): Promise<RequirementHistoryEntry[]> {
  return invoke("get_requirement_history", { id });
}

// Ledger types and functions

export interface LedgerEntry {
  seq: number;
  event_type: string;
  correlation_id: string | null;
  requirement_id: string | null;
  task_id: string | null;
  payload: string;
  created_at: number;
  prev_hash: string;
  entry_hash: string;
}

export interface ChainVerification {
  valid: boolean;
  entries: number;
  problem: string | null;
}

export interface EvidenceRecord {
  id: string;
  task_id: string;
  correlation_id: string | null;
  kind: string;
  content: string;
  success: boolean;
  created_at: number;
}

export function getLedger(limit: number = 50): Promise<LedgerEntry[]> {
  return invoke("get_ledger", { limit });
}

export function getLedgerForCorrelation(correlationId: string): Promise<LedgerEntry[]> {
  return invoke("get_ledger_for_correlation", { correlationId });
}

export function verifyLedger(): Promise<ChainVerification> {
  return invoke("verify_ledger");
}

export function getEvidenceForTask(taskId: string): Promise<EvidenceRecord[]> {
  return invoke("get_evidence_for_task", { taskId });
}

// Evidence kind constants
export const EvidenceKind = {
  VERIFICATION: "verification",
  ROLLBACK: "rollback",
  EXECUTION_OUTPUT: "execution_output"
};

// ---------------------------------------------------------------------
// Sprint 9 bindings for already-registered backend commands. Interfaces
// mirror the Rust Serialize structs field-for-field; argument keys match
// each command's real signature in src-tauri (checked, not guessed).
// ---------------------------------------------------------------------

// Sprint 4: promotion verdicts

export interface PromotionRequest {
  id: string;
  /** Empty string when promotion was judged with no evidence at all. */
  evidence_id: string;
  task_id: string;
  status: "requested" | "promoted" | "blocked" | string;
  requested_at: number;
  promoted_at: number | null;
}

export function getPromotionsForTask(taskId: string): Promise<PromotionRequest[]> {
  return invoke("get_promotions_for_task", { taskId });
}

// Sprint 3: task DAGs

export interface DagTaskSpec {
  file_path: string;
  note: string | null;
  /** Indices into the spec list of tasks this one depends on. */
  depends_on: number[];
}

export interface TaskDagRecord {
  id: string;
  requirement_id: string;
  version: number;
  created_at: number;
  correlation_id: string;
  task_ids: string[];
  execution_order: string[];
}

export function planRequirementDag(requirementId: string, specs: DagTaskSpec[]): Promise<TaskDagRecord> {
  // Rust param is snake_case requirement_id; Tauri v2 exposes it camelCased.
  return invoke("plan_requirement_dag", { requirementId, specs });
}

export function getDag(dagId: string): Promise<TaskDagRecord> {
  return invoke("get_dag", { dagId });
}

export function getDagRunnableTasks(dagId: string): Promise<import("./agent").AgentTask[]> {
  return invoke("get_dag_runnable_tasks", { dagId });
}

// Sprint 5: worker profiles + capability matching

export interface WorkerProfile {
  id: string;
  name: string;
  capabilities: string[];
  reliability_score: number;
  tasks_completed: number;
  tasks_failed: number;
}

export interface WorkerMatch {
  profile: WorkerProfile;
  score: number;
  matched: number;
  missing: string[];
}

export function listWorkerProfiles(): Promise<WorkerProfile[]> {
  return invoke("list_worker_profiles");
}

export function upsertWorkerProfile(profile: WorkerProfile): Promise<WorkerProfile> {
  return invoke("upsert_worker_profile", { profile });
}

export function deleteWorkerProfile(workerId: string): Promise<void> {
  return invoke("delete_worker_profile", { workerId });
}

export function refreshWorkerReliability(workerId: string): Promise<WorkerProfile> {
  return invoke("refresh_worker_reliability", { workerId });
}

export function matchWorkers(requiredCapabilities: string[]): Promise<WorkerMatch[]> {
  return invoke("match_workers", { requiredCapabilities });
}

// Sprint 8: reliability layer (retry / confidence / completeness / report)

export type FailureClass =
  | "compile_error"
  | "test_failure"
  | "execution_error"
  | "blocked_dependency"
  | "user_rejected"
  | "unknown"
  | "not_failed";

export interface RetryDecision {
  allowed: boolean;
  reason: string;
  failure_class: FailureClass;
  attempts_so_far: number;
  retry_task_id: string | null;
}

export interface ConfidenceReport {
  score: number;
  factors: string[];
}

export interface CompletenessReport {
  complete: boolean;
  missing: string[];
}

export interface TaskReport {
  task: import("./agent").AgentTask;
  failure_class: FailureClass;
  attempts: number;
  lineage: string[];
  evidence: EvidenceRecord[];
  promotions: PromotionRequest[];
  ledger_events: LedgerEntry[];
  confidence: ConfidenceReport;
  completeness: CompletenessReport;
}

/** Bounded, human-gated: an allowed retry only CREATES a task awaiting
 * approval in the Agent panel - nothing executes from this call. */
export function retryFailedTask(taskId: string, maxRetries?: number): Promise<RetryDecision> {
  return invoke("retry_failed_task", { taskId, maxRetries: maxRetries ?? null });
}

export function getTaskConfidence(taskId: string): Promise<ConfidenceReport> {
  return invoke("get_task_confidence", { taskId });
}

export function getTaskReport(taskId: string): Promise<TaskReport> {
  return invoke("get_task_report", { taskId });
}
