"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Spinner from "@/components/ui/Spinner";
import {
  createOrchestratorTask,
  approveOrchestratorTask,
  rejectOrchestratorTask,
  cancelOrchestratorTask,
  getOrchestratorState,
  listenOrchestratorState,
  type OrchestratorTask,
  type OrchestratorStatePayload,
} from "@/lib/orchestrator";

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

const backendPhaseToUI: Record<string, AgentPhase> = {
  Created: "idle",
  Analyzing: "analyzing",
  Planning: "planning",
  "Awaiting Approval": "awaiting_approval",
  Executing: "executing",
  Observing: "observing",
  Recovering: "recovering",
  Verifying: "verifying",
  Completed: "completed",
  Failed: "failed",
  Cancelled: "cancelled",
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
  const unlistenRef = useRef<(() => void) | null>(null);

  // Listen for real backend state changes
  useEffect(() => {
    listenOrchestratorState((payload: OrchestratorStatePayload) => {
      const uiPhase = backendPhaseToUI[payload.phase_name] || "idle";
      setPhase(uiPhase);
      setRecoveryCount(payload.recovery_attempts);

      appendTimeline(
        uiPhase,
        payload.phase_name,
        `Progress: ${payload.progress_percent.toFixed(0)}% · ${(payload.elapsed_ms / 1000).toFixed(0)}s`,
        0
      );
    }).then((fn) => { unlistenRef.current = fn; });
    return () => { unlistenRef.current?.(); };
  }, []);

  const appendTimeline = useCallback(
    (p: AgentPhase, summary: string, detail = "", durationMs = 0) => {
      setTimeline((prev) => [
        ...prev.slice(-50),
        { id: crypto.randomUUID(), timestamp: Date.now(), phase: p, summary, detail, durationMs },
      ]);
    },
    []
  );

  const resetAgent = async () => {
    try { await cancelOrchestratorTask(); } catch {}
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

    try {
      const task: OrchestratorTask = await createOrchestratorTask(goal);
      const uiPhase = backendPhaseToUI[task.phase] || "idle";
      setPhase(uiPhase);
      appendTimeline(uiPhase, task.phase || "Created", `Goal: ${goal}`);

      if (task.current_plan) {
        setPlanSteps(task.current_plan.subtasks?.map((s: any) => s.description) || []);
        setAffectedFiles(task.current_plan.affected_files || []);
      }
    } catch (err: any) {
      setErrorMessage(err?.message || String(err));
      setIsRunning(false);
    }
  };

  const approveAndExecute = async () => {
    setIsRunning(true);
    try {
      const task = await approveOrchestratorTask();
      appendTimeline("applying_patch", "Applying patch", "Human approved execution");
      // Poll for updated state
      const updated = await getOrchestratorState();
      if (updated.current_plan) {
        setPlanSteps(updated.current_plan.subtasks?.map((s: any) => s.description) || []);
        setAffectedFiles(updated.current_plan.affected_files || []);
      }
    } catch (err: any) {
      setErrorMessage(err?.message || String(err));
      setIsRunning(false);
    }
  };

  const rejectPlan = async () => {
    try {
      await rejectOrchestratorTask();
      appendTimeline("cancelled", "Plan rejected", "Human declined execution");
      setPhase("cancelled");
      setIsRunning(false);
    } catch (err: any) {
      setErrorMessage(err?.message || String(err));
    }
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