import { invoke } from "@tauri-apps/api/core";

export interface AgentTask {
  id: string;
  objective: string;
  agent: string;
  task_type: string;
  files: string[];
  status: string;
  verification: string | null;
  rollback: string | null;
  proposed_content: string | null;
  risk_summary: string | null;
  error: string | null;
  requirement_id: string | null;
  correlation_id: string | null;
}

/** Sprint 1: edit_file tasks are gated behind a validated requirement -
 * pass a requirement ID from lib/governance.ts, not a raw prompt. */
export function createAndPlanTask(requirementId: string, filePath: string): Promise<AgentTask> {
  return invoke("create_and_plan_task", { requirementId, filePath });
}

export function createAndPlanCodeTask(objective: string): Promise<AgentTask> {
  return invoke("create_and_plan_code_task", { objective });
}

export function approveTask(taskId: string): Promise<AgentTask> {
  return invoke("approve_task", { taskId });
}

export function rejectTask(taskId: string): Promise<void> {
  return invoke("reject_task", { taskId });
}

export function listAgentTasks(): Promise<AgentTask[]> {
  return invoke("list_agent_tasks");
}
