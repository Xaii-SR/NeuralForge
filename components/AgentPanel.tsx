"use client";

import { useEffect, useState } from "react";
import * as agent from "@/lib/agent";
import * as governance from "@/lib/governance";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";
import AutoResizeTextarea from "@/components/ui/AutoResizeTextarea";

export interface AgentPanelProps {
  workspaceOpen: boolean;
}

const STATUS_BADGE: Record<string, string> = {
  planning: "bg-neutral-200 text-neutral-600 dark:bg-neutral-800 dark:text-neutral-400",
  awaiting_approval: "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/40 dark:text-yellow-400",
  applying: "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400",
  completed: "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400",
  rolled_back: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
  failed: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
  rejected: "bg-neutral-200 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-500",
};

function riskLevel(summary: string): "low" | "medium" | "high" | null {
  if (summary.startsWith("low")) return "low";
  if (summary.startsWith("medium")) return "medium";
  if (summary.startsWith("high")) return "high";
  return null;
}

const RISK_BADGE: Record<string, string> = {
  low: "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400",
  medium: "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/40 dark:text-yellow-400",
  high: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400",
};

function StatusBadge({ status }: { status: string }) {
  return (
    <span className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${STATUS_BADGE[status] ?? "bg-neutral-200 text-neutral-500"}`}>
      {status.replace("_", " ")}
    </span>
  );
}

export default function AgentPanel({ workspaceOpen }: AgentPanelProps) {
  const [mode, setMode] = useState<"edit_file" | "run_code">("edit_file");
  const [objective, setObjective] = useState("");
  // Sprint 1: edit_file tasks are gated behind a validated requirement -
  // the form collects a contract (title/intent/criteria), not a raw prompt.
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

  async function refresh() {
    try {
      const list = await agent.listAgentTasks();
      setTasks(list);
    } catch {
      setTasks([]);
    } finally {
      setLoadingTasks(false);
    }
  }

  useEffect(() => {
    if (workspaceOpen) refresh();
    else setLoadingTasks(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceOpen]);

  async function planEditFile(resolvedPath: string) {
    setPlanning(true);
    setError(null);
    try {
      // Requirement first: a weak request is rejected by the validator
      // here, before any task row exists or any LLM call happens.
      const criteria = reqCriteria
        .split("\n")
        .map((c) => c.trim())
        .filter((c) => c.length > 0);
      const requirement = await governance.createRequirement(reqTitle, reqIntent, criteria);
      const task = await agent.createAndPlanTask(requirement.id, resolvedPath);
      setSelectedId(task.id);
      setReqTitle("");
      setReqIntent("");
      setReqCriteria("");
      setFilePath("");
      setCandidates(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPlanning(false);
    }
  }

  const editFileReady = reqTitle.trim().length > 0 && reqIntent.trim().length > 0 && reqCriteria.trim().length > 0;

  async function handlePlan() {
    if (planning || resolving) return;
    if (mode === "edit_file" && !editFileReady) return;
    if (mode === "run_code" && !objective.trim()) return;

    if (mode === "run_code") {
      setError(null);
      setPlanning(true);
      try {
        const task = await agent.createAndPlanCodeTask(objective);
        setSelectedId(task.id);
        setObjective("");
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setPlanning(false);
      }
      return;
    }

    if (!filePath.trim()) return;

    // Cursor-style resolution: the file field accepts a description, not
    // just an exact path. A clear winner proceeds automatically (with the
    // resolved path shown, not silently substituted); multiple close
    // candidates surface a disambiguation list instead of guessing.
    setError(null);
    setCandidates(null);
    setResolving(true);
    try {
      const result = await ai.resolveFileReference(filePath);
      if (result.resolved) {
        await planEditFile(result.resolved);
      } else if (result.candidates.length > 0) {
        setCandidates(result.candidates);
      } else {
        setError(`No file found matching "${filePath}"`);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setResolving(false);
    }
  }

  function handlePickCandidate(path: string) {
    setCandidates(null);
    planEditFile(path);
  }

  async function handleApprove(taskId: string) {
    setApproving(true);
    setError(null);
    try {
      await agent.approveTask(taskId);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setApproving(false);
    }
  }

  async function handleReject(taskId: string) {
    await agent.rejectTask(taskId);
    await refresh();
  }

  if (!workspaceOpen) {
    return <EmptyState icon="🤖" title="Open a folder to use the agent" hint="The Coder agent proposes file changes for your review before applying anything" />;
  }

  const selected = tasks.find((t) => t.id === selectedId) ?? tasks[0] ?? null;
  const risk = selected?.risk_summary ? riskLevel(selected.risk_summary) : null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-200 dark:border-neutral-800">
        <div className="space-y-1.5 border-b border-neutral-200 p-2 dark:border-neutral-800">
          <div className="flex rounded border border-neutral-200 p-0.5 text-[10px] font-medium dark:border-neutral-700">
            <button
              onClick={() => setMode("edit_file")}
              className={`flex-1 rounded px-2 py-1 transition-colors ${
                mode === "edit_file" ? "bg-blue-600 text-white" : "text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"
              }`}
            >
              Edit File
            </button>
            <button
              onClick={() => setMode("run_code")}
              className={`flex-1 rounded px-2 py-1 transition-colors ${
                mode === "run_code" ? "bg-blue-600 text-white" : "text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"
              }`}
            >
              Run Code
            </button>
          </div>
          {mode === "run_code" && (
            <AutoResizeTextarea
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              onSubmit={handlePlan}
              maxRows={5}
              placeholder="Objective (e.g. print the first 10 primes) - Shift+Enter for a new line"
              className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
            />
          )}
          {mode === "edit_file" && (
            <>
              <input
                value={reqTitle}
                onChange={(e) => setReqTitle(e.target.value)}
                placeholder="Requirement title (e.g. Add doc comment to add())"
                className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
              <AutoResizeTextarea
                value={reqIntent}
                onChange={(e) => setReqIntent(e.target.value)}
                onSubmit={handlePlan}
                maxRows={4}
                placeholder="Intent - what should change and why"
                className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
              <AutoResizeTextarea
                value={reqCriteria}
                onChange={(e) => setReqCriteria(e.target.value)}
                onSubmit={handlePlan}
                maxRows={4}
                placeholder="Acceptance criteria - one checkable outcome per line"
                className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
              <input
                value={filePath}
                onChange={(e) => {
                  setFilePath(e.target.value);
                  setCandidates(null);
                }}
                onKeyDown={(e) => e.key === "Enter" && handlePlan()}
                placeholder="Describe the file (e.g. 'the carina UI json') or give an exact path"
                className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
              {candidates && candidates.length > 0 && (
                <div className="space-y-1 rounded border border-yellow-200 bg-yellow-50 p-1.5 dark:border-yellow-900/50 dark:bg-yellow-950/30">
                  <div className="px-0.5 text-[10px] font-medium text-yellow-700 dark:text-yellow-400">Did you mean:</div>
                  {candidates.map((c) => (
                    <button
                      key={c.path}
                      onClick={() => handlePickCandidate(c.path)}
                      className="block w-full truncate rounded px-1.5 py-1 text-left font-mono text-[11px] text-neutral-700 transition-colors hover:bg-yellow-100 dark:text-neutral-300 dark:hover:bg-yellow-900/40"
                    >
                      {c.path}
                    </button>
                  ))}
                </div>
              )}
            </>
          )}
          {mode === "run_code" && (
            <div className="text-[10px] text-neutral-400 dark:text-neutral-500">
              Generates Python and runs it via the python-repl extension in an isolated process, after your approval.
            </div>
          )}
          <button
            onClick={handlePlan}
            disabled={
              planning ||
              resolving ||
              (mode === "edit_file" && (!editFileReady || !filePath.trim())) ||
              (mode === "run_code" && !objective.trim())
            }
            className="flex w-full items-center justify-center gap-1.5 rounded bg-blue-600 px-2 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
          >
            {(planning || resolving) && <Spinner size={10} />}
            {resolving ? "Finding file..." : planning ? "Planning..." : "Plan Task"}
          </button>
          {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {loadingTasks ? (
            <div className="flex items-center justify-center py-8">
              <Spinner />
            </div>
          ) : (
            <>
              {tasks.map((t) => (
                <div
                  key={t.id}
                  onClick={() => setSelectedId(t.id)}
                  className={`cursor-pointer space-y-1 border-b border-neutral-200 px-2 py-2 text-xs transition-colors hover:bg-neutral-100 dark:border-neutral-800 dark:hover:bg-neutral-800 ${
                    selected?.id === t.id ? "bg-neutral-100 dark:bg-neutral-800" : ""
                  }`}
                >
                  <div className="truncate font-medium text-neutral-700 dark:text-neutral-300">{t.objective}</div>
                  <div className="truncate text-[10px] text-neutral-400 dark:text-neutral-500">
                    {t.task_type === "run_code" ? "🐍 Python code" : t.files[0]}
                  </div>
                  <StatusBadge status={t.status} />
                </div>
              ))}
              {tasks.length === 0 && (
                <div className="p-4 text-center text-xs text-neutral-400 dark:text-neutral-600">No tasks yet</div>
              )}
            </>
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-xs">
        {!selected && !loadingTasks && (
          <EmptyState icon="📋" title="Select or create a task" hint="Describe an objective and a file to have the agent propose a change" />
        )}
        {selected && (
          <div className="space-y-3">
            <div>
              <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Objective</div>
              <div className="whitespace-pre-wrap text-neutral-800 dark:text-neutral-200">{selected.objective}</div>
            </div>
            {selected.requirement_id && (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Requirement</div>
                <div className="font-mono text-[10px] text-neutral-500 dark:text-neutral-400">
                  {selected.requirement_id}
                  {selected.correlation_id && <span> · correlation {selected.correlation_id}</span>}
                </div>
              </div>
            )}
            {selected.task_type === "run_code" ? (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Type</div>
                <div className="text-neutral-800 dark:text-neutral-200">🐍 Run Python via python-repl extension</div>
              </div>
            ) : (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">File</div>
                <div className="font-mono text-neutral-800 dark:text-neutral-200">{selected.files[0]}</div>
              </div>
            )}
            <div className="flex items-center gap-2">
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Status</div>
                <StatusBadge status={selected.status} />
              </div>
              {risk && (
                <div>
                  <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Risk</div>
                  <span className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${RISK_BADGE[risk]}`}>{risk}</span>
                </div>
              )}
            </div>
            {selected.risk_summary && (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                  Simulation Mode analysis
                </div>
                <div className="text-neutral-600 dark:text-neutral-300">{selected.risk_summary}</div>
              </div>
            )}
            {selected.verification && (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Verification</div>
                <div className="whitespace-pre-wrap rounded bg-neutral-50 p-2 font-mono text-[11px] text-neutral-600 dark:bg-neutral-800/60 dark:text-neutral-300">
                  {selected.verification}
                </div>
              </div>
            )}
            {selected.rollback && (
              <div className="flex items-start gap-2 rounded border border-red-200 bg-red-50 px-2.5 py-2 dark:border-red-900/50 dark:bg-red-950/40">
                <span aria-hidden>↩</span>
                <div>
                  <div className="text-[10px] font-medium uppercase tracking-wide text-red-500 dark:text-red-400">Rolled back</div>
                  <div className="text-red-600 dark:text-red-300">{selected.rollback}</div>
                </div>
              </div>
            )}
            {selected.proposed_content && (
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                  Proposed content
                </div>
                <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded border border-neutral-200 bg-neutral-50 p-2 text-[11px] text-neutral-700 dark:border-neutral-800 dark:bg-neutral-800/60 dark:text-neutral-300">
                  {selected.proposed_content}
                </pre>
              </div>
            )}
            {selected.status === "awaiting_approval" && (
              <div className="flex gap-2 pt-1">
                <button
                  onClick={() => handleApprove(selected.id)}
                  disabled={approving}
                  className="flex items-center gap-1.5 rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-green-500 disabled:opacity-50"
                >
                  {approving && <Spinner size={10} />}
                  {approving ? "Applying..." : "Approve"}
                </button>
                <button
                  onClick={() => handleReject(selected.id)}
                  className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-600 transition-colors hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
                >
                  Reject
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
