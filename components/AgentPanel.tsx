"use client";

import { useEffect, useState } from "react";
import * as agent from "@/lib/agent";

export interface AgentPanelProps {
  workspaceOpen: boolean;
}

const STATUS_COLOR: Record<string, string> = {
  planning: "text-neutral-400",
  awaiting_approval: "text-yellow-400",
  applying: "text-blue-400",
  completed: "text-green-400",
  rolled_back: "text-red-400",
  failed: "text-red-400",
  rejected: "text-neutral-500",
};

export default function AgentPanel({ workspaceOpen }: AgentPanelProps) {
  const [objective, setObjective] = useState("");
  const [filePath, setFilePath] = useState("");
  const [tasks, setTasks] = useState<agent.AgentTask[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [planning, setPlanning] = useState(false);
  const [approving, setApproving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const list = await agent.listAgentTasks();
      setTasks(list);
    } catch {
      setTasks([]);
    }
  }

  useEffect(() => {
    if (workspaceOpen) refresh();
  }, [workspaceOpen]);

  async function handlePlan() {
    if (!objective.trim() || !filePath.trim() || planning) return;
    setError(null);
    setPlanning(true);
    try {
      const task = await agent.createAndPlanTask(objective, filePath);
      setSelectedId(task.id);
      setObjective("");
      setFilePath("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPlanning(false);
    }
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
    return <div className="flex h-full items-center justify-center text-xs text-neutral-500">Open a folder to use the agent</div>;
  }

  const selected = tasks.find((t) => t.id === selectedId) ?? tasks[0] ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-800">
        <div className="space-y-1 border-b border-neutral-800 p-2">
          <input
            value={objective}
            onChange={(e) => setObjective(e.target.value)}
            placeholder="Objective (e.g. add a doc comment)"
            className="w-full rounded bg-neutral-800 px-2 py-1 text-xs text-neutral-200 outline-none"
          />
          <input
            value={filePath}
            onChange={(e) => setFilePath(e.target.value)}
            placeholder="File path (relative to workspace)"
            className="w-full rounded bg-neutral-800 px-2 py-1 text-xs text-neutral-200 outline-none"
          />
          <button
            onClick={handlePlan}
            disabled={planning || !objective.trim() || !filePath.trim()}
            className="w-full rounded bg-blue-600 px-2 py-1 text-xs text-white hover:bg-blue-500 disabled:opacity-50"
          >
            {planning ? "Planning..." : "Plan Task"}
          </button>
          {error && <div className="text-[10px] text-red-400">{error}</div>}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {tasks.map((t) => (
            <div
              key={t.id}
              onClick={() => setSelectedId(t.id)}
              className={`cursor-pointer border-b border-neutral-800 px-2 py-1.5 text-xs hover:bg-neutral-800 ${
                selected?.id === t.id ? "bg-neutral-800" : ""
              }`}
            >
              <div className="truncate text-neutral-300">{t.objective}</div>
              <div className="truncate text-[10px] text-neutral-500">{t.files[0]}</div>
              <div className={`text-[10px] ${STATUS_COLOR[t.status] ?? "text-neutral-500"}`}>{t.status}</div>
            </div>
          ))}
          {tasks.length === 0 && <div className="p-2 text-xs text-neutral-500">No tasks yet</div>}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-xs">
        {!selected && <div className="text-neutral-500">Select or create a task</div>}
        {selected && (
          <div className="space-y-2">
            <div>
              <div className="text-[10px] uppercase text-neutral-500">Objective</div>
              <div className="text-neutral-200">{selected.objective}</div>
            </div>
            <div>
              <div className="text-[10px] uppercase text-neutral-500">File</div>
              <div className="text-neutral-200">{selected.files[0]}</div>
            </div>
            <div>
              <div className="text-[10px] uppercase text-neutral-500">Status</div>
              <div className={STATUS_COLOR[selected.status] ?? "text-neutral-200"}>{selected.status}</div>
            </div>
            {selected.risk_summary && (
              <div>
                <div className="text-[10px] uppercase text-neutral-500">Risk (Simulation Mode)</div>
                <div className="text-neutral-300">{selected.risk_summary}</div>
              </div>
            )}
            {selected.verification && (
              <div>
                <div className="text-[10px] uppercase text-neutral-500">Verification</div>
                <div className="whitespace-pre-wrap text-neutral-300">{selected.verification}</div>
              </div>
            )}
            {selected.rollback && (
              <div>
                <div className="text-[10px] uppercase text-neutral-500">Rollback</div>
                <div className="text-red-400">{selected.rollback}</div>
              </div>
            )}
            {selected.proposed_content && (
              <div>
                <div className="text-[10px] uppercase text-neutral-500">Proposed content</div>
                <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded bg-neutral-800 p-2 text-[11px] text-neutral-300">
                  {selected.proposed_content}
                </pre>
              </div>
            )}
            {selected.status === "awaiting_approval" && (
              <div className="flex gap-2 pt-2">
                <button
                  onClick={() => handleApprove(selected.id)}
                  disabled={approving}
                  className="rounded bg-green-600 px-3 py-1 text-xs text-white hover:bg-green-500 disabled:opacity-50"
                >
                  {approving ? "Applying..." : "Approve"}
                </button>
                <button
                  onClick={() => handleReject(selected.id)}
                  className="rounded bg-neutral-700 px-3 py-1 text-xs text-neutral-200 hover:bg-neutral-600"
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
