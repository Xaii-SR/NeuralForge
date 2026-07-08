"use client";

import { useEffect, useState } from "react";
import * as extensions from "@/lib/extensions";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";

export default function ExtensionsPanel() {
  const [list, setList] = useState<extensions.InstalledExtension[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyName, setBusyName] = useState<string | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [testInput, setTestInput] = useState("print('hello from an isolated process')");
  const [testRunning, setTestRunning] = useState(false);
  const [testResult, setTestResult] = useState<extensions.ExtensionResult | null>(null);

  async function refresh() {
    try {
      const found = await extensions.listExtensions();
      setList(found);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function toggle(name: string, enabled: boolean) {
    setBusyName(name);
    try {
      await extensions.setExtensionEnabled(name, enabled);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyName(null);
    }
  }

  async function uninstall(name: string) {
    setBusyName(name);
    try {
      await extensions.uninstallExtension(name);
      if (selectedName === name) setSelectedName(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyName(null);
    }
  }

  async function runTest(ext: extensions.InstalledExtension) {
    setTestRunning(true);
    setTestResult(null);
    try {
      const request = ext.manifest.name === "file-search" ? { query: testInput, files: ["src/auth.rs", "src/lib.rs", "README.md"] } : { code: testInput };
      const result = await extensions.runExtension(ext.manifest.name, request);
      setTestResult(result);
    } catch (e) {
      setTestResult({ success: false, output: null, error: String(e) });
    } finally {
      setTestRunning(false);
    }
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner />
      </div>
    );
  }

  const selected = list.find((e) => e.manifest.name === selectedName) ?? list[0] ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-neutral-200 dark:border-neutral-800">
        <div className="border-b border-neutral-200 px-3 py-2 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:border-neutral-800 dark:text-neutral-500">
          Installed extensions
        </div>
        {error && (
          <div className="p-2">
            <ErrorBanner message={error} onDismiss={() => setError(null)} />
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {list.length === 0 && !error && (
            <div className="p-4 text-center text-xs text-neutral-400 dark:text-neutral-600">No extensions found</div>
          )}
          {list.map((ext) => (
            <div
              key={ext.manifest.name}
              onClick={() => {
                setSelectedName(ext.manifest.name);
                setTestResult(null);
              }}
              className={`cursor-pointer space-y-1 border-b border-neutral-200 px-3 py-2 text-xs transition-colors hover:bg-neutral-100 dark:border-neutral-800 dark:hover:bg-neutral-800 ${
                selected?.manifest.name === ext.manifest.name ? "bg-neutral-100 dark:bg-neutral-800" : ""
              }`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="truncate font-medium text-neutral-700 dark:text-neutral-300">{ext.manifest.name}</span>
                <span
                  className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${
                    ext.enabled
                      ? "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400"
                      : "bg-neutral-200 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-500"
                  }`}
                >
                  {ext.enabled ? "enabled" : "disabled"}
                </span>
              </div>
              <div className="truncate text-[10px] text-neutral-400 dark:text-neutral-500">
                v{ext.manifest.version} · {ext.manifest.runtime}
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-xs">
        {!selected && <EmptyState icon="🧩" title="No extension selected" hint="Extensions are loaded from ~/.neuralforge/extensions" />}
        {selected && (
          <div className="space-y-3">
            <div>
              <div className="mb-1 text-sm font-semibold text-neutral-800 dark:text-neutral-200">{selected.manifest.name}</div>
              <div className="text-neutral-500 dark:text-neutral-400">{selected.manifest.description}</div>
            </div>
            <div className="grid grid-cols-2 gap-2 text-[11px]">
              <div>
                <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Version</div>
                <div className="text-neutral-700 dark:text-neutral-300">{selected.manifest.version}</div>
              </div>
              <div>
                <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Author</div>
                <div className="text-neutral-700 dark:text-neutral-300">{selected.manifest.author}</div>
              </div>
              <div>
                <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Runtime</div>
                <div className="text-neutral-700 dark:text-neutral-300">{selected.manifest.runtime} (isolated child process)</div>
              </div>
              <div>
                <div className="mb-0.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Permissions</div>
                <div className="text-neutral-700 dark:text-neutral-300">{selected.manifest.permissions.join(", ") || "none declared"}</div>
              </div>
            </div>
            <div className="truncate text-[10px] text-neutral-400 dark:text-neutral-500" title={selected.dir}>
              {selected.dir}
            </div>
            <div className="flex gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-800">
              <button
                onClick={() => toggle(selected.manifest.name, !selected.enabled)}
                disabled={busyName === selected.manifest.name}
                className="flex items-center gap-1.5 rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 disabled:opacity-50 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
              >
                {busyName === selected.manifest.name && <Spinner size={10} />}
                {selected.enabled ? "Disable" : "Enable"}
              </button>
              <button
                onClick={() => uninstall(selected.manifest.name)}
                disabled={busyName === selected.manifest.name}
                className="rounded bg-red-50 px-3 py-1.5 text-xs font-medium text-red-600 transition-colors hover:bg-red-100 disabled:opacity-50 dark:bg-red-950/40 dark:text-red-400 dark:hover:bg-red-950/60"
              >
                Uninstall
              </button>
            </div>

            <div className="border-t border-neutral-200 pt-3 dark:border-neutral-800">
              <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                Test this extension directly
              </div>
              <textarea
                value={testInput}
                onChange={(e) => setTestInput(e.target.value)}
                rows={3}
                placeholder={selected.manifest.name === "file-search" ? "search query" : "python code"}
                className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1.5 font-mono text-[11px] text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
              <button
                onClick={() => runTest(selected)}
                disabled={testRunning || !selected.enabled}
                className="mt-1.5 flex items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
              >
                {testRunning && <Spinner size={10} />}
                {testRunning ? "Running..." : "Run"}
              </button>
              {!selected.enabled && (
                <div className="mt-1 text-[10px] text-neutral-400 dark:text-neutral-500">Enable the extension to run it.</div>
              )}
              {testResult && (
                <div className="mt-2">
                  <div
                    className={`mb-1 text-[10px] font-medium uppercase tracking-wide ${
                      testResult.success ? "text-green-600 dark:text-green-400" : "text-red-600 dark:text-red-400"
                    }`}
                  >
                    {testResult.success ? "Success" : "Error"}
                  </div>
                  <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded border border-neutral-200 bg-neutral-50 p-2 text-[11px] text-neutral-700 dark:border-neutral-800 dark:bg-neutral-800/60 dark:text-neutral-300">
                    {testResult.error ?? JSON.stringify(testResult.output, null, 2)}
                  </pre>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
