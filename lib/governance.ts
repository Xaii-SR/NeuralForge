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
