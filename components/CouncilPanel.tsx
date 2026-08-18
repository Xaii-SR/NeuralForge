"use client";

import { useMemo } from "react";
import AITeamPanel from "@/components/AITeamPanel";

/**
 * Council is intentionally a single, bounded review pass. It shares the
 * typed sequential runner with AI TEAM but exposes only reviewer, critic,
 * and final-editor roles rather than a persistent autonomous workflow.
 */
export default function CouncilPanel({ workspaceGeneration }: { workspaceGeneration: number }) {
  // A workspace switch invalidates all prior Council evidence. Remounting the
  // bounded runner clears partial output rather than showing a stale verdict.
  const generationKey = useMemo(() => workspaceGeneration, [workspaceGeneration]);
  return <AITeamPanel key={generationKey} variant="council" workspaceGeneration={workspaceGeneration} />;
}
