import { invoke } from "@tauri-apps/api/core";

export type CouncilVerdict = "Accept" | "Reject" | "Revise" | "Unclear";

export interface CouncilPassResult {
  architect_output: string;
  critic_output: string;
  judge_output: string;
  judge_verdict: CouncilVerdict;
}

// Runs one real, sequential Architect -> Critic -> Judge pass against
// `objective`, backed by agent_core::commands::run_council_pass. taskId is
// caller-supplied (no existing task required) - a plain string is enough.
export function runCouncilPass(taskId: string, objective: string): Promise<CouncilPassResult> {
  return invoke("run_council_pass", { taskId, objective });
}
