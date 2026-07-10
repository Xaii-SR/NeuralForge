"use client";

import { Fragment, useEffect, useState } from "react";
import * as governance from "@/lib/governance";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";

/** Sprint 9: read-only window onto the Sprint 2 governance ledger -
 * browse entries, filter by correlation chain, verify the hash chain.
 * Nothing here writes anything. */
export interface GovernancePanelProps {
  workspaceOpen: boolean;
}

const EVENT_COLOR: Record<string, string> = {
  requirement_created: "text-blue-600 dark:text-blue-400",
  requirement_rejected: "text-red-600 dark:text-red-400",
  task_completed: "text-green-600 dark:text-green-400",
  task_failed: "text-red-600 dark:text-red-400",
  task_rolled_back: "text-red-600 dark:text-red-400",
  task_retried: "text-yellow-600 dark:text-yellow-400",
  promotion_approved: "text-green-600 dark:text-green-400",
  promotion_blocked: "text-red-600 dark:text-red-400",
};

export default function GovernancePanel({ workspaceOpen }: GovernancePanelProps) {
  const [entries, setEntries] = useState<governance.LedgerEntry[]>([]);
  const [correlationFilter, setCorrelationFilter] = useState("");
  const [verification, setVerification] = useState<governance.ChainVerification | null>(null);
  const [loading, setLoading] = useState(true);
  const [verifying, setVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedSeq, setExpandedSeq] = useState<number | null>(null);

  async function refresh(correlation?: string) {
    setLoading(true);
    setError(null);
    try {
      const list = correlation?.trim()
        ? await governance.getLedgerForCorrelation(correlation.trim())
        : await governance.getLedger(200);
      setEntries(list);
    } catch (e) {
      setError(String(e));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (workspaceOpen) refresh();
    else setLoading(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceOpen]);

  async function handleVerify() {
    setVerifying(true);
    setError(null);
    try {
      setVerification(await governance.verifyLedger());
    } catch (e) {
      setError(String(e));
    } finally {
      setVerifying(false);
    }
  }

  if (!workspaceOpen) {
    return <EmptyState icon="📜" title="Open a folder to view its governance ledger" hint="Every requirement, task, verdict, and retry is recorded in a hash-chained audit trail" />;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-neutral-200 p-2 dark:border-neutral-800">
        <input
          value={correlationFilter}
          onChange={(e) => setCorrelationFilter(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && refresh(correlationFilter)}
          placeholder="Filter by correlation ID (Enter to apply, empty for latest 200)"
          className="min-w-0 flex-1 rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <button
          onClick={() => refresh(correlationFilter)}
          className="rounded bg-neutral-100 px-2.5 py-1 text-xs font-medium text-neutral-600 transition-colors hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
        >
          Refresh
        </button>
        <button
          onClick={handleVerify}
          disabled={verifying}
          className="flex items-center gap-1.5 rounded bg-blue-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        >
          {verifying && <Spinner size={10} />}
          Verify chain
        </button>
      </div>
      {verification && (
        <div
          className={`shrink-0 border-b px-3 py-1.5 text-xs ${
            verification.valid
              ? "border-green-200 bg-green-50 text-green-700 dark:border-green-900/50 dark:bg-green-950/40 dark:text-green-300"
              : "border-red-200 bg-red-50 text-red-600 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300"
          }`}
        >
          {verification.valid
            ? `✓ Hash chain verified: ${verification.entries} entries intact`
            : `✗ TAMPERING DETECTED: ${verification.problem}`}
        </div>
      )}
      {error && (
        <div className="shrink-0 p-2">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Spinner />
          </div>
        ) : entries.length === 0 ? (
          <div className="p-4 text-center text-xs text-neutral-400 dark:text-neutral-600">
            {correlationFilter.trim() ? "No ledger entries for that correlation ID" : "No governance events recorded yet"}
          </div>
        ) : (
          <table className="w-full text-left text-[11px]">
            <thead className="sticky top-0 bg-neutral-50 text-[10px] uppercase tracking-wide text-neutral-400 dark:bg-neutral-900 dark:text-neutral-500">
              <tr>
                <th className="px-2 py-1.5 font-medium">Seq</th>
                <th className="px-2 py-1.5 font-medium">Event</th>
                <th className="px-2 py-1.5 font-medium">Task</th>
                <th className="px-2 py-1.5 font-medium">Correlation</th>
                <th className="px-2 py-1.5 font-medium">When</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((e) => (
                <Fragment key={e.seq}>
                  <tr
                    onClick={() => setExpandedSeq(expandedSeq === e.seq ? null : e.seq)}
                    className="cursor-pointer border-b border-neutral-100 transition-colors hover:bg-neutral-50 dark:border-neutral-800/60 dark:hover:bg-neutral-800/60"
                  >
                    <td className="px-2 py-1 font-mono text-neutral-400 dark:text-neutral-500">{e.seq}</td>
                    <td className={`px-2 py-1 font-medium ${EVENT_COLOR[e.event_type] ?? "text-neutral-600 dark:text-neutral-300"}`}>
                      {e.event_type}
                    </td>
                    <td className="max-w-32 truncate px-2 py-1 font-mono text-neutral-500 dark:text-neutral-400">{e.task_id ?? "—"}</td>
                    <td
                      className="max-w-32 cursor-pointer truncate px-2 py-1 font-mono text-neutral-500 underline-offset-2 hover:underline dark:text-neutral-400"
                      onClick={(ev) => {
                        ev.stopPropagation();
                        if (e.correlation_id) {
                          setCorrelationFilter(e.correlation_id);
                          refresh(e.correlation_id);
                        }
                      }}
                      title={e.correlation_id ? "Click to filter by this correlation chain" : undefined}
                    >
                      {e.correlation_id ?? "—"}
                    </td>
                    <td className="whitespace-nowrap px-2 py-1 text-neutral-400 dark:text-neutral-500">
                      {new Date(e.created_at * 1000).toLocaleString()}
                    </td>
                  </tr>
                  {expandedSeq === e.seq && (
                    <tr className="border-b border-neutral-100 dark:border-neutral-800/60">
                      <td colSpan={5} className="bg-neutral-50 px-3 py-2 dark:bg-neutral-800/40">
                        <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Payload</div>
                        <pre className="max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px] text-neutral-600 dark:text-neutral-400">
                          {e.payload}
                        </pre>
                        <div className="mt-1.5 space-y-0.5 font-mono text-[9px] text-neutral-400 dark:text-neutral-600">
                          <div>hash: {e.entry_hash}</div>
                          <div>prev: {e.prev_hash}</div>
                        </div>
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
