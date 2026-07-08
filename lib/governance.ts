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
