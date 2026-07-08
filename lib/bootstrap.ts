import { invoke } from "@tauri-apps/api/core";

export interface SelfImprovementProposal {
  title: string;
  slug: string;
  file_path: string;
  rationale: string;
  original_content: string;
  proposed_content: string;
  risk_summary: string;
  diff: string;
}

export interface SelfImprovementResult {
  branch_name: string;
  diff: string;
  tests_passed: boolean;
  test_output: string;
  pr_summary: string;
}

export function proposeSelfImprovement(): Promise<SelfImprovementProposal> {
  return invoke("propose_self_improvement");
}

export function applySelfImprovement(proposal: SelfImprovementProposal): Promise<SelfImprovementResult> {
  return invoke("apply_self_improvement", { proposal });
}
