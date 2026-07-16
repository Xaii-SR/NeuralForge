"use client";

import { useCallback, useState } from "react";
import Spinner from "@/components/ui/Spinner";

type AgentPhase =
  | "idle"
  | "analyzing"
  | "retrieving_context"
  | "planning"
  | "awaiting_approval"
  | "applying_patch"
  | "executing"
  | "observing"
  | "recovering"
  | "verifying"
  | "updating_knowledge"
  | "completed"
  | "failed"
  | "cancelled";

interface TimelineEntry {
  id: string;
  timestamp: number;
  phase: AgentPhase;
  summary: string;
  detail: string;
  durationMs: number;
}

const PHASE_LABELS: Record<AgentPhase, string> = {
  idle: "Idle",
  analyzing: "Analyzing",
  retrieving_context: "Retrieving Context",
  planning: "Planning",
  awaiting_approval: "Awaiting Approval",
  applying_patch: "Applying Patch",
  executing: "Executing",
  observing: "Observing",
  recovering: "Recovering",
  verifying: "Verifying",
  updating_knowledge: "Knowledge Update",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

export default function AgentWorkbench() {
  const [phase, setPhase] = useState<AgentPhase>("idle");
  const [userGoal, setUserGoal] = useState("");
  const [timeline, setTimeline] = useState<TimelineEntry[]>([]);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [recoveryCount, setRecoveryCount] = useState(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [planSteps, setPlanSteps] = useState<string[]>([]);
  const [affectedFiles, setAffectedFiles] = useState<string[]>([]);
  const [streamOutput, setStreamOutput] = useState("");
  const [knowledgeQuery, setKnowledgeQuery] = useState("");
  const [knowledgeResults, setKnowledgeResults] = useState<string[]>([]);

  const appendTimeline = useCallback(
    (p: AgentPhase, summary: string, detail = "", durationMs = 0) => {
      setTimeline((prev) => [
        ...prev.slice(-50),
        { id: crypto.randomUUID(), timestamp: Date.now(), phase: p, summary, detail, durationMs },
      ]);
    },
    []
  );

  const resetAgent = () => {
    setPhase("idle");
    setUserGoal("");
    setTimeline([]);
    setStartedAt(null);
    setRecoveryCount(0);
    setErrorMessage(null);
    setIsRunning(false);
    setPlanSteps([]);
    setAffectedFiles([]);
    setStreamOutput("");
  };

  const startAgent = async () => {
    const goal = userGoal.trim();
    if (!goal || isRunning) return;
    setErrorMessage(null);
    setIsRunning(true);
    setStartedAt(Date.now());
    setPhase("analyzing");
    appendTimeline("analyzing", "Analyzing workspace", "Scanning project files...");
    await new Promise((r) => setTimeout(r, 600));
    setPhase("retrieving_context");
    appendTimeline("retrieving_context", "Retrieving relevant context", "Matching goal to project structure");
    await new Promise((r) => setTimeout(r, 500));
    setPhase("planning");
    const steps = [`Identify files to modify for: "${goal}"`, "Generate code changes", "Run build verification"];
    setPlanSteps(steps);
    setAffectedFiles(["src/main.rs", "src/lib.rs"]);
    appendTimeline("planning", "Generated implementation plan", `${steps.length} steps planned`, 1200);
    await new Promise((r) => setTimeout(r, 600));
    setPhase("awaiting_approval");
    appendTimeline("awaiting_approval", "Waiting for human approval", "Plan requires review before execution");
    setIsRunning(false);
  };

  const approveAndExecute = async () => {
    setIsRunning(true);
    setPhase("applying_patch");
    appendTimeline("applying_patch", "Applying approved changes", "Writing modifications to workspace...");
    await new Promise((r) => setTimeout(r, 800));
    setPhase("executing");
    appendTimeline("executing", "Running verification commands", "Executing cargo check...");
    const lines = ["Running cargo check...", " Checking src/main.rs", " Checking src/lib.rs", " Compilation successful!", "Running cargo test...", " All tests passed!", ""];
    for (const line of lines) {
      await new Promise((r) => setTimeout(r, 200));
      setStreamOutput((prev) => prev + line + "\n");
    }
    setPhase("observing");
    appendTimeline("observing", "Build passed, all tests green", "Exit code 0", 2500);
    setPhase("verifying");
    appendTimeline("verifying", "Verification complete", "Task successfully executed");
    setPhase("updating_knowledge");
    appendTimeline("updating_knowledge", "Updating project knowledge", "Caching successful strategy");
    setPhase("completed");
    appendTimeline("completed", "Task completed successfully");
    setIsRunning(false);
  };

  const rejectPlan = () => {
    appendTimeline("failed", "Plan rejected by user", "Approval was declined");
    setPhase("cancelled");
    setIsRunning(false);
  };

  const paused = phase === "awaiting_approval";

  return (
    <div className="flex h-full bg-white dark:bg-neutral-900">
      {/* Left: Controls + Plan */}
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-200 dark:border-neutral-800 p-3 gap-2.5 text-xs overflow-y-auto">
        <div>
          <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-neutral-400">Goal</label>
          <textarea value={userGoal} onChange={(e) => setUserGoal(e.target.value)} rows={3} placeholder="e.g., Fix auth bug, add logging..." disabled={isRunning}
            className="w-full resize-none rounded border border-neutral-200 bg-white px-2.5 py-1.5 text-xs text-neutral-800 outline-none focus:border-blue-500 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
        </div>

        <div className="flex gap-2">
          {!isRunning && !paused && (
            <button onClick={startAgent} disabled={!userGoal.trim()}
              className="flex-1 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-40">▶ Start</button>
          )}
          {paused && (<>
            <button onClick={approveAndExecute} className="flex-1 rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-green-500">✓ Approve</button>
            <button onClick={rejectPlan} className="flex-1 rounded bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-500">✗ Reject</button>
          </>)}
          {(isRunning || paused) && (
            <button onClick={resetAgent} className="flex-1 rounded bg-red-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-600">⏹ Cancel</button>
          )}
        </div>

        <div className="rounded border border-neutral-200 dark:border-neutral-800 p-2">
          <div className="text-[10px] uppercase tracking-wider text-neutral-400 mb-1">Phase</div>
          <div className="flex items-center gap-1.5">
            {isRunning && <Spinner size={10} />}
            <span className="font-semibold text-neutral-800 dark:text-neutral-100">{PHASE_LABELS[phase]}</span>
          </div>
          {startedAt && <div className="mt-0.5 text-[10px] text-neutral-400">Running: {((Date.now() - startedAt) / 1000).toFixed(0)}s</div>}
          {recoveryCount > 0 && <div className="mt-0.5 text-[10px] text-amber-500">Recovery attempts: {recoveryCount}</div>}
        </div>

        {planSteps.length > 0 && (
          <div className="rounded border border-neutral-200 dark:border-neutral-800 p-2">
            <div className="text-[10px] uppercase tracking-wider text-neutral-400 mb-1">Plan ({planSteps.length} steps)</div>
            <ol className="list-decimal pl-4 space-y-0.5 text-neutral-700 dark:text-neutral-300">{planSteps.map((s, i) => (<li key={i} className="text-[11px]">{s}</li>))}</ol>
          </div>
        )}
        {affectedFiles.length > 0 && (
          <div className="rounded border border-neutral-200 dark:border-neutral-800 p-2">
            <div className="text-[10px] uppercase tracking-wider text-neutral-400 mb-1">Affected Files</div>
            <div className="space-y-0.5">{affectedFiles.map((f) => (<div key={f} className="text-[11px] text-neutral-600 dark:text-neutral-400 font-mono">{f}</div>))}</div>
          </div>
        )}
        {errorMessage && (<div className="rounded border border-red-200 bg-red-50 p-2 text-[11px] text-red-700 dark:border-red-800 dark:bg-red-900/30 dark:text-red-400">{errorMessage}</div>)}
      </div>

      {/* Center: Timeline + Terminal */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-8 shrink-0 items-center border-b border-neutral-200 dark:border-neutral-800 px-3">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-neutral-400">Execution Timeline</span>
        </div>
        <div className="flex-1 overflow-y-auto px-3 py-2">
          {timeline.length === 0 && <div className="flex h-full items-center justify-center text-xs text-neutral-400">No actions yet — enter a goal and start the agent</div>}
          <div className="space-y-1">
            {timeline.map((entry) => (
              <div key={entry.id} className="flex items-start gap-2 rounded px-2 py-1 text-[11px] hover:bg-neutral-50 dark:hover:bg-neutral-800/50">
                <span className="mt-0.5 shrink-0 text-neutral-400">{entry.phase === "completed" ? "✓" : entry.phase === "failed" ? "✗" : entry.phase === "awaiting_approval" ? "⏸" : "▸"}</span>
                <div className="min-w-0 flex-1">
                  <div className="font-medium text-neutral-700 dark:text-neutral-200">{entry.summary}</div>
                  {entry.detail && <div className="text-[10px] text-neutral-400 dark:text-neutral-500">{entry.detail}</div>}
                </div>
                <span className="shrink-0 text-[10px] text-neutral-400 tabular-nums">{new Date(entry.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}{entry.durationMs > 0 && ` · ${(entry.durationMs / 1000).toFixed(1)}s`}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="flex h-8 shrink-0 items-center border-t border-neutral-200 dark:border-neutral-800 px-3">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-neutral-400">Terminal Output</span>
        </div>
        <div className="h-28 shrink-0 overflow-y-auto border-t border-neutral-200 bg-black/95 dark:bg-black/90 font-mono text-xs text-green-400 p-2 whitespace-pre-wrap">{streamOutput || "Awaiting execution..."}</div>
      </div>

      {/* Right: Knowledge */}
      <div className="flex w-48 shrink-0 flex-col border-l border-neutral-200 dark:border-neutral-800 p-3 gap-2.5 text-xs">
        <div>
          <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-neutral-400">Knowledge</label>
          <input type="text" value={knowledgeQuery} onChange={(e) => setKnowledgeQuery(e.target.value)} placeholder="Search..." className="w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
        </div>
        {knowledgeResults.length > 0 && (<div className="space-y-1">{knowledgeResults.map((r, i) => (<div key={i} className="rounded bg-neutral-50 dark:bg-neutral-800 p-1.5 text-[10px] text-neutral-600 dark:text-neutral-400">{r}</div>))}</div>)}
        <div className="flex-1" />
        <button onClick={resetAgent} className="rounded border border-neutral-200 dark:border-neutral-700 px-3 py-1.5 text-[10px] font-medium text-neutral-500 hover:bg-neutral-50 dark:text-neutral-400 dark:hover:bg-neutral-800">Clear Session</button>
      </div>
    </div>
  );
}