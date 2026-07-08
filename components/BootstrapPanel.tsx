"use client";

import { useState } from "react";
import * as bootstrap from "@/lib/bootstrap";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";

export interface BootstrapPanelProps {
  workspaceOpen: boolean;
}

export default function BootstrapPanel({ workspaceOpen }: BootstrapPanelProps) {
  const [proposal, setProposal] = useState<bootstrap.SelfImprovementProposal | null>(null);
  const [result, setResult] = useState<bootstrap.SelfImprovementResult | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleAnalyze() {
    setError(null);
    setResult(null);
    setProposal(null);
    setAnalyzing(true);
    try {
      const p = await bootstrap.proposeSelfImprovement();
      setProposal(p);
    } catch (e) {
      setError(String(e));
    } finally {
      setAnalyzing(false);
    }
  }

  async function handleApprove() {
    if (!proposal) return;
    setError(null);
    setApplying(true);
    try {
      const r = await bootstrap.applySelfImprovement(proposal);
      setResult(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  }

  function handleReject() {
    setProposal(null);
    setResult(null);
  }

  if (!workspaceOpen) {
    return (
      <EmptyState
        icon="🔁"
        title="Open a folder to use self-bootstrap"
        hint="NeuralForge can analyze the open workspace's own code and propose one focused improvement, gated behind your approval"
      />
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto p-3 text-xs">
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="max-w-md text-[10px] text-neutral-400 dark:text-neutral-500">
          Reads this workspace&apos;s own project memory and source files, proposes ONE focused improvement, and shows you a diff. Nothing is
          written until you approve - and even then, NeuralForge only creates a local git branch and runs tests. It never pushes or opens a PR.
        </div>
        <button
          onClick={handleAnalyze}
          disabled={analyzing}
          className="flex shrink-0 items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        >
          {analyzing && <Spinner size={10} />}
          {analyzing ? "Analyzing..." : "Analyze & Propose"}
        </button>
      </div>

      {error && (
        <div className="mb-2">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      )}

      {!proposal && !analyzing && (
        <EmptyState icon="🔁" title="No proposal yet" hint="Click Analyze & Propose to have NeuralForge suggest an improvement to its own code" />
      )}

      {proposal && (
        <div className="space-y-3">
          <div>
            <div className="mb-1 text-sm font-semibold text-neutral-800 dark:text-neutral-200">{proposal.title}</div>
            <div className="text-neutral-500 dark:text-neutral-400">{proposal.rationale}</div>
          </div>
          <div className="grid grid-cols-2 gap-2 text-[11px]">
            <div>
              <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">File</div>
              <div className="font-mono text-neutral-700 dark:text-neutral-300">{proposal.file_path}</div>
            </div>
            <div>
              <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Risk</div>
              <div className="text-neutral-700 dark:text-neutral-300">{proposal.risk_summary}</div>
            </div>
          </div>
          <div>
            <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Diff</div>
            <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded border border-neutral-200 bg-neutral-50 p-2 font-mono text-[11px] text-neutral-700 dark:border-neutral-800 dark:bg-neutral-800/60 dark:text-neutral-300">
              {proposal.diff}
            </pre>
          </div>

          {!result && (
            <div className="flex gap-2 pt-1">
              <button
                onClick={handleApprove}
                disabled={applying}
                className="flex items-center gap-1.5 rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-green-500 disabled:opacity-50"
              >
                {applying && <Spinner size={10} />}
                {applying ? "Creating branch + running tests..." : "Approve"}
              </button>
              <button
                onClick={handleReject}
                disabled={applying}
                className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-600 transition-colors hover:bg-neutral-200 disabled:opacity-50 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
              >
                Reject
              </button>
            </div>
          )}

          {result && (
            <div className="space-y-2 border-t border-neutral-200 pt-3 dark:border-neutral-800">
              <div className="flex items-center gap-2">
                <span
                  className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-medium ${
                    result.tests_passed
                      ? "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400"
                      : "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-400"
                  }`}
                >
                  {result.tests_passed ? "tests passed" : "tests failed"}
                </span>
                <span className="font-mono text-neutral-600 dark:text-neutral-300">{result.branch_name}</span>
              </div>
              <div className="rounded border border-blue-200 bg-blue-50 px-2.5 py-2 text-blue-700 dark:border-blue-900/50 dark:bg-blue-950/40 dark:text-blue-300">
                Created locally on branch <span className="font-mono">{result.branch_name}</span>. Nothing was pushed - review the summary below,
                then push and open a PR yourself if you approve.
              </div>
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Test output</div>
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded border border-neutral-200 bg-neutral-50 p-2 font-mono text-[11px] text-neutral-700 dark:border-neutral-800 dark:bg-neutral-800/60 dark:text-neutral-300">
                  {result.test_output}
                </pre>
              </div>
              <div>
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">PR summary</div>
                <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded border border-neutral-200 bg-neutral-50 p-2 font-mono text-[11px] text-neutral-700 dark:border-neutral-800 dark:bg-neutral-800/60 dark:text-neutral-300">
                  {result.pr_summary}
                </pre>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
