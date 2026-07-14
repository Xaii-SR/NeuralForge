"use client";

import { useEffect, useState } from "react";
import * as agent from "@/lib/agent";
import * as governance from "@/lib/governance";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";
import AutoResizeTextarea from "@/components/ui/AutoResizeTextarea";
import TaskReportView from "@/components/TaskReportView";

export interface AgentPanelProps { workspaceOpen: boolean; }

const STATUS_BADGE: Record<string, string> = {
  planning: "bg-neutral-200 text-neutral-600 dark:bg-neutral-800 dark:text-neutral-400",
  awaiting_approval: "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/40 dark:text-yellow-400",
  applying: "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400",
  completed: "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400",
  rolled_back: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
  failed: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
  rejected: "bg-neutral-200 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-500",
  blocked: "bg-orange-100 text-orange-700 dark:bg-orange-900/40 dark:text-orange-400",
};

function StatusBadge({ status }: { status: string }) {
  return <span className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${STATUS_BADGE[status] ?? "bg-neutral-200 text-neutral-500"}`}>{status.replace("_", " ")}</span>;
}

export default function AgentPanel({ workspaceOpen }: AgentPanelProps) {
  const [mode, setMode] = useState<"edit_file" | "run_code">("edit_file");
  const [objective, setObjective] = useState("");
  const [reqTitle, setReqTitle] = useState("");
  const [reqIntent, setReqIntent] = useState("");
  const [reqCriteria, setReqCriteria] = useState("");
  const [filePath, setFilePath] = useState("");
  const [tasks, setTasks] = useState<agent.AgentTask[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [planning, setPlanning] = useState(false);
  const [resolving, setResolving] = useState(false);
  const [candidates, setCandidates] = useState<ai.FileCandidate[] | null>(null);
  const [approving, setApproving] = useState(false);
  const [loadingTasks, setLoadingTasks] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [v2TaskId, setV2TaskId] = useState<string | null>(null);
  const [v2State, setV2State] = useState<string | null>(null);

  async function refresh() { try { setTasks(await agent.listAgentTasks()); } catch { setTasks([]); } finally { setLoadingTasks(false); } }

  useEffect(() => { if (workspaceOpen) refresh(); else setLoadingTasks(false); }, [workspaceOpen]);

  // Phase D V2 telemetry listener
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<{ id: string; state: any }>("agent-state-changed", (event) => {
        const s = event.payload.state;
        setV2State(typeof s === "object" && s?.Failed ? `Failed: ${s.Failed}` : String(s));
        if (s === "AwaitingApproval") setV2TaskId(event.payload.id);
        if (s === "Completed" || (typeof s === "object" && "Failed" in s)) refresh();
      }).then((fn) => { unlisten = fn; });
    });
    return () => { unlisten?.(); };
  }, []);

  async function planEditFile(resolvedPath: string) {
    setPlanning(true); setError(null);
    try {
      const criteria = reqCriteria.split("\n").map((c) => c.trim()).filter((c) => c.length > 0);
      const requirement = await governance.createRequirement(reqTitle, reqIntent, criteria);
      const task = await agent.createAndPlanTask(requirement.id, resolvedPath);
      setSelectedId(task.id); setReqTitle(""); setReqIntent(""); setReqCriteria(""); setFilePath(""); setCandidates(null);
      await refresh();
    } catch (e) { setError(String(e)); } finally { setPlanning(false); }
  }

  const editFileReady = reqTitle.trim().length > 0 && reqIntent.trim().length > 0 && reqCriteria.trim().length > 0;

  async function handlePlan() {
    if (planning || resolving) return;
    if (mode === "edit_file" && !editFileReady) return;
    if (mode === "run_code" && !objective.trim()) return;

    if (mode === "run_code") {
      setError(null); setPlanning(true);
      try { const task = await agent.createAndPlanCodeTask(objective); setSelectedId(task.id); setObjective(""); await refresh(); }
      catch (e) { setError(String(e)); } finally { setPlanning(false); }
      return;
    }
    if (!filePath.trim()) return;
    setError(null); setCandidates(null); setResolving(true);
    try {
      const result = await ai.resolveFileReference(filePath);
      if (result.resolved) await planEditFile(result.resolved);
      else if (result.candidates.length > 0) setCandidates(result.candidates);
      else setError(`No file found matching "${filePath}"`);
    } catch (e) { setError(String(e)); } finally { setResolving(false); }
  }

  async function handleApprove(taskId: string) {
    setApproving(true); setError(null);
    try { await agent.approveTask(taskId); await refresh(); }
    catch (e) { setError(String(e)); } finally { setApproving(false); }
  }

  async function handleReject(taskId: string) { await agent.rejectTask(taskId); await refresh(); }

  // Phase G HITL: approve/reject V2 agent task
  async function handleV2Approve() {
    if (!v2TaskId) return;
    try { await import("@tauri-apps/api/core").then(({ invoke }) => invoke("approve_agent_task", { id: v2TaskId })); }
    catch (e) { setError(String(e)); }
  }

  async function handleV2Reject() {
    if (!v2TaskId) return;
    try { await import("@tauri-apps/api/core").then(({ invoke }) => invoke("reject_agent_task", { id: v2TaskId })); }
    catch (e) { setError(String(e)); }
  }

  if (!workspaceOpen) return <EmptyState icon="🤖" title="Open a folder to use the agent" hint="The Coder agent proposes file changes for your review before applying anything" />;

  const selected = tasks.find((t) => t.id === selectedId) ?? tasks[0] ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-200 dark:border-neutral-800">
        <div className="space-y-1.5 border-b border-neutral-200 p-2 dark:border-neutral-800">
          <div className="flex rounded border border-neutral-200 p-0.5 text-[10px] font-medium dark:border-neutral-700">
            <button onClick={() => setMode("edit_file")} className={`flex-1 rounded px-2 py-1 transition-colors ${mode === "edit_file" ? "bg-blue-600 text-white" : "text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"}`}>Edit File</button>
            <button onClick={() => setMode("run_code")} className={`flex-1 rounded px-2 py-1 transition-colors ${mode === "run_code" ? "bg-blue-600 text-white" : "text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"}`}>Run Code</button>
          </div>

          <button
            onClick={async () => {
              const desc = mode === "edit_file" ? reqIntent || "Edit file task" : objective || "Code task";
              try { await import("@tauri-apps/api/core").then(({ invoke }) => invoke("start_agent_task", { description: desc })); }
              catch (e) { /* ignore */ }
            }}
            className="flex w-full items-center justify-center gap-1.5 rounded border border-purple-500 text-purple-300 bg-purple-900/20 px-2 py-1.5 text-xs font-medium transition-colors hover:bg-purple-900/40"
          >
            🤖 Run V2 Agent
          </button>

          {/* HITL Approval buttons */}
          {v2State === "AwaitingApproval" && (
            <div className="flex gap-2 pt-1">
              <button onClick={handleV2Approve} className="flex-1 flex items-center justify-center gap-1 rounded bg-green-700 px-2 py-1.5 text-xs font-semibold text-white hover:bg-green-600">🟢 Approve</button>
              <button onClick={handleV2Reject} className="flex-1 flex items-center justify-center gap-1 rounded bg-red-800 px-2 py-1.5 text-xs font-semibold text-white hover:bg-red-700">🔴 Reject</button>
            </div>
          )}

          {mode === "run_code" && <AutoResizeTextarea value={objective} onChange={(e) => setObjective(e.target.value)} onSubmit={handlePlan} maxRows={5} placeholder="Objective (e.g. print the first 10 primes)" className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />}
          {mode === "edit_file" && (<>
            <input value={reqTitle} onChange={(e) => setReqTitle(e.target.value)} placeholder="Requirement title" className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
            <AutoResizeTextarea value={reqIntent} onChange={(e) => setReqIntent(e.target.value)} onSubmit={handlePlan} maxRows={4} placeholder="Intent - what should change and why" className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
            <AutoResizeTextarea value={reqCriteria} onChange={(e) => setReqCriteria(e.target.value)} onSubmit={handlePlan} maxRows={4} placeholder="Acceptance criteria" className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
            <input value={filePath} onChange={(e) => { setFilePath(e.target.value); setCandidates(null); }} onKeyDown={(e) => e.key === "Enter" && handlePlan()} placeholder="Describe the file or give an exact path" className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </>)}
          <button onClick={handlePlan} disabled={planning || resolving || (mode === "edit_file" && (!editFileReady || !filePath.trim())) || (mode === "run_code" && !objective.trim())}
            className="flex w-full items-center justify-center gap-1.5 rounded bg-blue-600 px-2 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50">
            {(planning || resolving) && <Spinner size={10} />}{resolving ? "Finding file..." : planning ? "Planning..." : "Plan Task"}
          </button>
          {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-xs">
        {!selected && !loadingTasks && <EmptyState icon="📋" title="Select or create a task" />}
        {selected && (<div className="space-y-3">
          <div><div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400">Objective</div><div className="whitespace-pre-wrap text-neutral-800 dark:text-neutral-200">{selected.objective}</div></div>
          <div className="flex items-center gap-2"><StatusBadge status={selected.status} /></div>
          {selected.status === "awaiting_approval" && (<div className="flex gap-2 pt-1">
            <button onClick={() => handleApprove(selected.id)} disabled={approving} className="flex items-center gap-1.5 rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-green-500 disabled:opacity-50">{approving && <Spinner size={10} />}{approving ? "Applying..." : "Approve"}</button>
            <button onClick={() => handleReject(selected.id)} className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700">Reject</button>
          </div>)}
          {["completed", "failed", "rolled_back", "blocked"].includes(selected.status) && <TaskReportView taskId={selected.id} onRetryCreated={(retryId) => { setSelectedId(retryId); refresh(); }} />}
        </div>)}
      </div>
    </div>
  );
}