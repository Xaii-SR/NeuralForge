"use client";

import { useEffect, useState } from "react";
import * as governance from "@/lib/governance";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";

/** Sprint 9: worker registry UI over the Sprint 5 backend - profile CRUD
 * (delete behind explicit confirmation), reliability refresh (derived
 * from real governance verdicts, never hand-set here), and a capability
 * match preview showing how the matcher would rank the registry. */
export interface WorkersPanelProps {
  workspaceOpen: boolean;
}

function reliabilityColor(score: number): string {
  if (score >= 0.8) return "text-green-600 dark:text-green-400";
  if (score >= 0.5) return "text-yellow-600 dark:text-yellow-400";
  return "text-red-600 dark:text-red-400";
}

export default function WorkersPanel({ workspaceOpen }: WorkersPanelProps) {
  const [profiles, setProfiles] = useState<governance.WorkerProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [newId, setNewId] = useState("");
  const [newName, setNewName] = useState("");
  const [newCaps, setNewCaps] = useState("");
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState<string | null>(null);
  const [matchQuery, setMatchQuery] = useState("");
  const [matches, setMatches] = useState<governance.WorkerMatch[] | null>(null);

  async function refresh() {
    try {
      setProfiles(await governance.listWorkerProfiles());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (workspaceOpen) refresh();
    else setLoading(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceOpen]);

  async function handleCreate() {
    if (!newId.trim() || !newName.trim() || !newCaps.trim() || saving) return;
    setSaving(true);
    setError(null);
    try {
      await governance.upsertWorkerProfile({
        id: newId.trim(),
        name: newName.trim(),
        capabilities: newCaps.split(",").map((c) => c.trim()).filter(Boolean),
        reliability_score: 1.0,
        tasks_completed: 0,
        tasks_failed: 0,
      });
      setNewId("");
      setNewName("");
      setNewCaps("");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(id: string) {
    setError(null);
    try {
      await governance.deleteWorkerProfile(id);
      setConfirmDelete(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleRefreshReliability(id: string) {
    setRefreshing(id);
    setError(null);
    try {
      await governance.refreshWorkerReliability(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(null);
    }
  }

  async function handleMatch() {
    const caps = matchQuery.split(",").map((c) => c.trim()).filter(Boolean);
    setError(null);
    try {
      setMatches(await governance.matchWorkers(caps));
    } catch (e) {
      setError(String(e));
    }
  }

  if (!workspaceOpen) {
    return <EmptyState icon="👷" title="Open a folder to manage worker profiles" hint="Workers are matched to tasks by capability; reliability is derived from their real verified outcomes" />;
  }

  return (
    <div className="flex h-full">
      <div className="flex w-72 shrink-0 flex-col gap-1.5 border-r border-neutral-200 p-2 dark:border-neutral-800">
        <div className="text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">New worker profile</div>
        <input
          value={newId}
          onChange={(e) => setNewId(e.target.value)}
          placeholder="ID (e.g. coder)"
          className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          placeholder="Display name"
          className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <input
          value={newCaps}
          onChange={(e) => setNewCaps(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleCreate()}
          placeholder="Capabilities, comma-separated (coding, testing)"
          className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <button
          onClick={handleCreate}
          disabled={saving || !newId.trim() || !newName.trim() || !newCaps.trim()}
          className="flex w-full items-center justify-center gap-1.5 rounded bg-blue-600 px-2 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        >
          {saving && <Spinner size={10} />}
          Save profile
        </button>

        <div className="mt-3 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Match preview</div>
        <input
          value={matchQuery}
          onChange={(e) => setMatchQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleMatch()}
          placeholder="Required capabilities (e.g. testing)"
          className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <button
          onClick={handleMatch}
          className="w-full rounded bg-neutral-100 px-2 py-1.5 text-xs font-medium text-neutral-600 transition-colors hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
        >
          Rank workers
        </button>
        {matches && (
          <div className="min-h-0 flex-1 space-y-1 overflow-y-auto">
            {matches.length === 0 && <div className="text-[11px] text-neutral-400">No profiles registered</div>}
            {matches.map((m, i) => (
              <div key={m.profile.id} className="rounded border border-neutral-200 px-2 py-1 text-[11px] dark:border-neutral-700">
                <div className="flex items-center justify-between">
                  <span className="font-medium text-neutral-700 dark:text-neutral-300">
                    {i === 0 ? "→ " : ""}
                    {m.profile.name}
                  </span>
                  <span className="font-mono text-[10px] text-neutral-400">{m.score.toFixed(2)}</span>
                </div>
                {m.missing.length > 0 && (
                  <div className="text-[10px] text-red-500 dark:text-red-400">missing: {m.missing.join(", ")}</div>
                )}
              </div>
            ))}
          </div>
        )}
        {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Spinner />
          </div>
        ) : profiles.length === 0 ? (
          <EmptyState icon="👷" title="No worker profiles yet" hint="Create one on the left; tasks can then be routed by capability" />
        ) : (
          <div className="space-y-2">
            {profiles.map((p) => (
              <div key={p.id} className="rounded border border-neutral-200 p-2.5 dark:border-neutral-800">
                <div className="flex items-center justify-between">
                  <div>
                    <span className="text-xs font-medium text-neutral-800 dark:text-neutral-200">{p.name}</span>
                    <span className="ml-2 font-mono text-[10px] text-neutral-400 dark:text-neutral-500">{p.id}</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => handleRefreshReliability(p.id)}
                      disabled={refreshing === p.id}
                      title="Recompute reliability from this worker's real verified outcomes"
                      className="flex items-center gap-1 rounded bg-neutral-100 px-2 py-0.5 text-[10px] text-neutral-600 transition-colors hover:bg-neutral-200 disabled:opacity-50 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
                    >
                      {refreshing === p.id && <Spinner size={9} />}
                      Refresh score
                    </button>
                    {confirmDelete === p.id ? (
                      <span className="flex items-center gap-1">
                        <button
                          onClick={() => handleDelete(p.id)}
                          className="rounded bg-red-600 px-2 py-0.5 text-[10px] font-medium text-white hover:bg-red-500"
                        >
                          Confirm delete
                        </button>
                        <button
                          onClick={() => setConfirmDelete(null)}
                          className="rounded bg-neutral-100 px-2 py-0.5 text-[10px] text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300"
                        >
                          Cancel
                        </button>
                      </span>
                    ) : (
                      <button
                        onClick={() => setConfirmDelete(p.id)}
                        className="rounded bg-neutral-100 px-2 py-0.5 text-[10px] text-neutral-500 transition-colors hover:bg-red-100 hover:text-red-600 dark:bg-neutral-800 dark:text-neutral-400 dark:hover:bg-red-900/40 dark:hover:text-red-400"
                      >
                        Delete
                      </button>
                    )}
                  </div>
                </div>
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {p.capabilities.map((c) => (
                    <span key={c} className="rounded bg-blue-100 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:bg-blue-900/40 dark:text-blue-400">
                      {c}
                    </span>
                  ))}
                </div>
                <div className="mt-1.5 flex items-center gap-3 text-[11px]">
                  <span>
                    reliability:{" "}
                    <span className={`font-semibold ${reliabilityColor(p.reliability_score)}`}>
                      {(p.reliability_score * 100).toFixed(0)}%
                    </span>
                  </span>
                  <span className="text-neutral-500 dark:text-neutral-400">
                    {p.tasks_completed} completed · {p.tasks_failed} failed
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
