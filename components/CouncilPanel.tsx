"use client";

import { useState } from "react";
import Spinner from "@/components/ui/Spinner";
import ErrorBanner from "@/components/ui/ErrorBanner";
import CopyButton from "@/components/ui/CopyButton";
import { runCouncilPass, type CouncilPassResult } from "@/lib/council";

const VERDICT_BADGE: Record<string, string> = {
  Accept: "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400",
  Reject: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
  Revise: "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/40 dark:text-yellow-400",
  Unclear: "bg-neutral-200 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-500",
};

function StageOutput({ label, output }: { label: string; output: string }) {
  return (
    <div className="group relative rounded border border-neutral-200 p-2 dark:border-neutral-800">
      <div className="mb-1 text-[10px] font-medium uppercase text-neutral-500 dark:text-neutral-500">{label}</div>
      <div className="whitespace-pre-wrap text-xs text-neutral-800 dark:text-neutral-200">{output}</div>
      <CopyButton text={output} className="absolute right-1 top-1 opacity-0 group-hover:opacity-100" />
    </div>
  );
}

export default function CouncilPanel() {
  const [taskId, setTaskId] = useState("");
  const [objective, setObjective] = useState("");
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CouncilPassResult | null>(null);

  async function handleRun() {
    if (!objective.trim() || running) return;
    setRunning(true);
    setError(null);
    setResult(null);
    try {
      const id = taskId.trim() || `council-${Date.now()}`;
      const pass = await runCouncilPass(id, objective.trim());
      setResult(pass);
    } catch (e: any) {
      setError(String(e));
    }
    setRunning(false);
  }

  return (
    <div className="flex h-full flex-col gap-2 overflow-y-auto p-2">
      <div className="flex shrink-0 flex-col gap-1.5">
        <input
          value={taskId}
          onChange={(e) => setTaskId(e.target.value)}
          placeholder="Task id (optional - auto-generated if empty)"
          className="rounded border border-neutral-200 bg-transparent px-2 py-1 text-xs dark:border-neutral-800"
        />
        <textarea
          value={objective}
          onChange={(e) => setObjective(e.target.value)}
          placeholder="Objective for the Council pass (Architect proposes, Critic reviews, Judge decides)"
          rows={3}
          className="resize-none rounded border border-neutral-200 bg-transparent px-2 py-1 text-xs dark:border-neutral-800"
        />
        <button
          onClick={handleRun}
          disabled={running || !objective.trim()}
          className="flex items-center justify-center gap-1.5 self-start rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {running && <Spinner size={12} />}
          {running ? "Running Council..." : "Run Council"}
        </button>
      </div>

      {error && <ErrorBanner message={error} onDismiss={() => setError(null)} onRetry={handleRun} />}

      {result && (
        <div className="flex min-h-0 flex-1 flex-col gap-2">
          <div className="flex shrink-0 items-center gap-2">
            <span className="text-[10px] font-medium uppercase text-neutral-500 dark:text-neutral-500">Verdict</span>
            <span className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${VERDICT_BADGE[result.judge_verdict] ?? VERDICT_BADGE.Unclear}`}>
              {result.judge_verdict}
            </span>
          </div>
          <StageOutput label="Architect" output={result.architect_output} />
          <StageOutput label="Critic" output={result.critic_output} />
          <StageOutput label="Judge" output={result.judge_output} />
        </div>
      )}
    </div>
  );
}
