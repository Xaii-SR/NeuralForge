"use client";

import { useEffect, useState } from "react";
import * as governance from "@/lib/governance";
import Spinner from "@/components/ui/Spinner";
import ErrorBanner from "@/components/ui/ErrorBanner";

/** Sprint 9: renders the Sprint 8 structured task report - confidence,
 * failure class, retry lineage, evidence, promotion verdicts - for one
 * task. Read-only except the retry action, which only PREPARES a new
 * task awaiting human approval (it never executes anything). */
export interface TaskReportViewProps {
  taskId: string;
  /** Called after a retry task is created so the parent can refresh. */
  onRetryCreated?: (retryTaskId: string) => void;
}

const FAILURE_LABEL: Record<string, string> = {
  compile_error: "Compile error",
  test_failure: "Test failure",
  execution_error: "Execution error",
  blocked_dependency: "Blocked by dependency",
  user_rejected: "Rejected by user",
  unknown: "Unknown failure",
  not_failed: "Not failed",
};

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">{children}</div>;
}

function confidenceColor(score: number): string {
  if (score >= 0.7) return "text-green-600 dark:text-green-400";
  if (score >= 0.4) return "text-yellow-600 dark:text-yellow-400";
  return "text-red-600 dark:text-red-400";
}

export default function TaskReportView({ taskId, onRetryCreated }: TaskReportViewProps) {
  const [report, setReport] = useState<governance.TaskReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [retryConfirm, setRetryConfirm] = useState(false);
  const [retryResult, setRetryResult] = useState<governance.RetryDecision | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setRetryResult(null);
    setRetryConfirm(false);
    governance
      .getTaskReport(taskId)
      .then((r) => !cancelled && setReport(r))
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [taskId]);

  async function handleRetry() {
    setRetrying(true);
    setError(null);
    try {
      const decision = await governance.retryFailedTask(taskId);
      setRetryResult(decision);
      setRetryConfirm(false);
      if (decision.allowed && decision.retry_task_id) {
        onRetryCreated?.(decision.retry_task_id);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRetrying(false);
    }
  }

  if (loading) {
    return (
      <div className="flex items-center gap-2 py-2 text-neutral-400">
        <Spinner size={12} /> Loading report...
      </div>
    );
  }
  if (error && !report) {
    return <ErrorBanner message={error} onDismiss={() => setError(null)} />;
  }
  if (!report) return null;

  const isFailure = report.failure_class !== "not_failed" && report.failure_class !== "blocked_dependency" && report.failure_class !== "user_rejected";
  const showRetry = report.task.status === "failed" || report.task.status === "rolled_back";

  return (
    <div className="space-y-3 border-t border-neutral-200 pt-3 dark:border-neutral-800">
      <div className="flex items-center gap-4">
        {report.task.status === "completed" && (
          <div>
            <SectionLabel>Confidence</SectionLabel>
            <span className={`text-sm font-semibold ${confidenceColor(report.confidence.score)}`}>
              {(report.confidence.score * 100).toFixed(0)}%
            </span>
          </div>
        )}
        {report.failure_class !== "not_failed" && (
          <div>
            <SectionLabel>Failure class</SectionLabel>
            <span className="inline-block rounded bg-red-100 px-1.5 py-0.5 text-[10px] font-medium text-red-700 dark:bg-red-900/40 dark:text-red-400">
              {FAILURE_LABEL[report.failure_class] ?? report.failure_class}
            </span>
          </div>
        )}
        <div>
          <SectionLabel>Attempts</SectionLabel>
          <span className="text-neutral-700 dark:text-neutral-300">{report.attempts}</span>
        </div>
        <div>
          <SectionLabel>Record</SectionLabel>
          {report.completeness.complete ? (
            <span className="text-green-600 dark:text-green-400">complete</span>
          ) : (
            <span className="text-red-600 dark:text-red-400" title={report.completeness.missing.join("; ")}>
              incomplete
            </span>
          )}
        </div>
      </div>

      {report.task.status === "completed" && report.confidence.factors.length > 0 && (
        <div>
          <SectionLabel>Confidence factors</SectionLabel>
          <ul className="space-y-0.5 text-[11px] text-neutral-500 dark:text-neutral-400">
            {report.confidence.factors.map((f, i) => (
              <li key={i}>· {f}</li>
            ))}
          </ul>
        </div>
      )}

      {!report.completeness.complete && (
        <div className="rounded border border-red-200 bg-red-50 px-2.5 py-2 text-[11px] text-red-600 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300">
          <div className="mb-0.5 font-medium">Evidence record incomplete:</div>
          {report.completeness.missing.map((m, i) => (
            <div key={i}>· {m}</div>
          ))}
        </div>
      )}

      {report.lineage.length > 1 && (
        <div>
          <SectionLabel>Retry lineage ({report.lineage.length} attempts)</SectionLabel>
          <div className="space-y-0.5 font-mono text-[10px] text-neutral-500 dark:text-neutral-400">
            {report.lineage.map((id) => (
              <div key={id}>{id === taskId ? `→ ${id} (this task)` : `· ${id}`}</div>
            ))}
          </div>
        </div>
      )}

      {report.evidence.length > 0 && (
        <div>
          <SectionLabel>Evidence ({report.evidence.length})</SectionLabel>
          <div className="space-y-1.5">
            {report.evidence.map((ev) => (
              <div key={ev.id} className="rounded border border-neutral-200 dark:border-neutral-800">
                <div className="flex items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-2 py-1 dark:border-neutral-800 dark:bg-neutral-800/60">
                  <span className="text-[10px] font-medium text-neutral-600 dark:text-neutral-300">{ev.kind}</span>
                  <span className={`text-[10px] font-medium ${ev.success ? "text-green-600 dark:text-green-400" : "text-red-600 dark:text-red-400"}`}>
                    {ev.success ? "✓ pass" : "✗ fail"}
                  </span>
                </div>
                <pre className="max-h-32 overflow-auto whitespace-pre-wrap px-2 py-1.5 font-mono text-[10px] text-neutral-600 dark:text-neutral-400">
                  {ev.content}
                </pre>
              </div>
            ))}
          </div>
        </div>
      )}

      {report.promotions.length > 0 && (
        <div>
          <SectionLabel>Promotion verdicts</SectionLabel>
          <div className="space-y-0.5">
            {report.promotions.map((p) => (
              <div key={p.id} className="flex items-center gap-2 text-[11px]">
                <span
                  className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${
                    p.status === "promoted"
                      ? "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400"
                      : "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400"
                  }`}
                >
                  {p.status}
                </span>
                <span className="text-neutral-400 dark:text-neutral-500">{new Date(p.requested_at * 1000).toLocaleString()}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {showRetry && isFailure && (
        <div className="space-y-1.5">
          {!retryConfirm && !retryResult?.allowed && (
            <button
              onClick={() => setRetryConfirm(true)}
              className="rounded bg-yellow-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-yellow-500"
            >
              Retry task...
            </button>
          )}
          {retryConfirm && (
            <div className="space-y-1.5 rounded border border-yellow-200 bg-yellow-50 p-2 dark:border-yellow-900/50 dark:bg-yellow-950/30">
              <div className="text-[11px] text-yellow-700 dark:text-yellow-400">
                This prepares attempt {report.attempts + 1} as a new task that will still require your approval before anything
                executes. Nothing runs automatically.
              </div>
              <div className="flex gap-2">
                <button
                  onClick={handleRetry}
                  disabled={retrying}
                  className="flex items-center gap-1.5 rounded bg-yellow-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-yellow-500 disabled:opacity-50"
                >
                  {retrying && <Spinner size={10} />}
                  Prepare retry
                </button>
                <button
                  onClick={() => setRetryConfirm(false)}
                  className="rounded bg-neutral-100 px-3 py-1 text-xs text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
          {retryResult && (
            <div
              className={`rounded px-2.5 py-2 text-[11px] ${
                retryResult.allowed
                  ? "border border-green-200 bg-green-50 text-green-700 dark:border-green-900/50 dark:bg-green-950/40 dark:text-green-300"
                  : "border border-red-200 bg-red-50 text-red-600 dark:border-red-900/50 dark:bg-red-950/40 dark:text-red-300"
              }`}
            >
              {retryResult.reason}
            </div>
          )}
        </div>
      )}
      {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
    </div>
  );
}
