import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type TaskLifecycle =
  | "Created"
  | "Analyzing"
  | "Planning"
  | "AwaitingApproval"
  | "Executing"
  | "Observing"
  | "Recovering"
  | "Verifying"
  | "Completed"
  | "Failed"
  | "Cancelled";

export interface OrchestratorTask {
  id: string;
  user_goal: string;
  phase: TaskLifecycle;
  created_at: number;
  updated_at: number;
  workspace_root: string;
  child_tasks: string[];
  execution_history: TaskPhaseRecord[];
  failure_reports: any[];
  recovery_attempts: number;
  max_recovery_attempts: number;
  current_plan: TaskPlan | null;
  plan_steps: string[];
  affected_files: string[];
}

export interface TaskPhaseRecord {
  phase: string;
  entered_at: number;
  exited_at: number | null;
  summary: string;
  success: boolean | null;
}

export interface TaskPlan {
  task_description: string;
  objective: string;
  affected_files: string[];
  subtasks: Subtask[];
  risks: string[];
  verification: string[];
  unknown_information: string[];
}

export interface Subtask {
  id: number;
  description: string;
  dependencies: number[];
  required_files: string[];
  expected_outcome: string;
}

export interface OrchestratorStatePayload {
  task_id: string;
  phase: TaskLifecycle;
  phase_name: string;
  progress_percent: number;
  recovery_attempts: number;
  elapsed_ms: number;
}

export function createOrchestratorTask(goal: string): Promise<OrchestratorTask> {
  return invoke("orchestrator_create_task", { goal });
}

export function approveOrchestratorTask(): Promise<OrchestratorTask> {
  return invoke("orchestrator_approve_task");
}

export function rejectOrchestratorTask(): Promise<OrchestratorTask> {
  return invoke("orchestrator_reject_task");
}

export function cancelOrchestratorTask(): Promise<void> {
  return invoke("orchestrator_cancel_task");
}

export function getOrchestratorState(): Promise<OrchestratorTask> {
  return invoke("orchestrator_get_state");
}

export function resetOrchestrator(): Promise<void> {
  return invoke("orchestrator_reset");
}

export function listenOrchestratorState(
  callback: (payload: OrchestratorStatePayload) => void
): Promise<() => void> {
  return listen<OrchestratorStatePayload>("orchestrator-state-changed", (event) => {
    callback(event.payload);
  });
}