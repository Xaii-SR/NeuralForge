"use client";

import { useEffect, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";

export interface SettingsPanelProps {
  onClose: () => void;
}

const OPTION_BASE = "rounded px-3 py-1.5 text-xs font-medium transition-colors";
const OPTION_ACTIVE = "bg-blue-600 text-white";
const OPTION_INACTIVE =
  "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700";

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [prefs, setPrefs] = useState<ai.Preferences>({ goal: "speed", cost_preference: "free" });
  const [models, setModels] = useState<ai.OllamaModel[]>([]);
  const [benchmarks, setBenchmarks] = useState<Record<string, ai.BenchmarkResult>>({});
  const [benchmarking, setBenchmarking] = useState<string | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      ai.getPreferences(),
      ai.listModels().catch(() => []),
      ai.getBenchmarks().catch(() => []),
    ]).then(([loadedPrefs, loadedModels, loadedBenchmarks]) => {
      setPrefs(loadedPrefs);
      setModels(loadedModels);
      const map: Record<string, ai.BenchmarkResult> = {};
      for (const b of loadedBenchmarks) map[b.model] = b;
      setBenchmarks(map);
      setLoading(false);
    });
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  async function handleSave() {
    await ai.savePreferences(prefs);
    onClose();
  }

  async function handleBenchmark(model: string) {
    setBenchmarking(model);
    try {
      const result = await ai.runModelBenchmark(model);
      setBenchmarks((prev) => ({ ...prev, [model]: result }));
    } finally {
      setBenchmarking(null);
    }
  }

  async function handleClearCache() {
    const count = await ai.clearResponseCache();
    setCacheStatus(`Cleared ${count} cached response${count === 1 ? "" : "s"}`);
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]"
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="max-h-[80vh] w-[480px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200"
      >
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">Settings</h2>
          <button
            onClick={onClose}
            aria-label="Close settings"
            className="rounded px-1.5 py-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 hover:text-neutral-700 dark:text-neutral-500 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
          >
            ✕
          </button>
        </div>

        {loading ? (
          <div className="flex items-center justify-center gap-2 py-12 text-xs text-neutral-500">
            <Spinner />
            Loading settings...
          </div>
        ) : (
          <>
            <div className="mb-5">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Goal</div>
              <div className="flex gap-2">
                {(["speed", "quality"] as const).map((g) => (
                  <button
                    key={g}
                    onClick={() => setPrefs((p) => ({ ...p, goal: g }))}
                    className={`${OPTION_BASE} ${prefs.goal === g ? OPTION_ACTIVE : OPTION_INACTIVE}`}
                  >
                    {g === "speed" ? "Fast" : "Best Quality"}
                  </button>
                ))}
              </div>
            </div>

            <div className="mb-5">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                Cost preference
              </div>
              <div className="flex gap-2">
                {(["free", "cheap", "quality_first"] as const).map((c) => (
                  <button
                    key={c}
                    onClick={() => setPrefs((p) => ({ ...p, cost_preference: c }))}
                    className={`${OPTION_BASE} ${prefs.cost_preference === c ? OPTION_ACTIVE : OPTION_INACTIVE}`}
                  >
                    {c === "free" ? "Free only" : c === "cheap" ? "Cheap OK" : "Quality first"}
                  </button>
                ))}
              </div>
            </div>

            <div className="mb-5">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                Model Benchmarks
              </div>
              <div className="space-y-1.5">
                {models.map((m) => {
                  const b = benchmarks[m.name];
                  return (
                    <div
                      key={m.name}
                      className="flex items-center justify-between rounded border border-neutral-200 bg-neutral-50 px-2.5 py-1.5 dark:border-neutral-800 dark:bg-neutral-800/60"
                    >
                      <div className="min-w-0 flex-1 truncate">
                        <div className="truncate text-xs font-medium">{m.name}</div>
                        {b && (
                          <div className="text-[10px] text-neutral-500 dark:text-neutral-500">
                            {b.tokens_per_second ? `${b.tokens_per_second.toFixed(1)} tok/s` : "n/a"} ·{" "}
                            {b.latency_ms.toFixed(0)}ms latency ·{" "}
                            <span className={b.reliable ? "text-green-600 dark:text-green-400" : "text-red-500 dark:text-red-400"}>
                              {b.reliable ? "reliable" : "unreliable"}
                            </span>
                          </div>
                        )}
                      </div>
                      <button
                        onClick={() => handleBenchmark(m.name)}
                        disabled={benchmarking === m.name}
                        className="ml-2 flex shrink-0 items-center gap-1 rounded bg-neutral-200 px-2 py-1 text-[10px] font-medium text-neutral-700 transition-colors hover:bg-neutral-300 disabled:opacity-50 dark:bg-neutral-700 dark:text-neutral-200 dark:hover:bg-neutral-600"
                      >
                        {benchmarking === m.name && <Spinner size={9} />}
                        {benchmarking === m.name ? "Running" : b ? "Re-run" : "Benchmark"}
                      </button>
                    </div>
                  );
                })}
                {models.length === 0 && (
                  <div className="rounded border border-dashed border-neutral-200 px-3 py-4 text-center text-xs text-neutral-400 dark:border-neutral-800 dark:text-neutral-600">
                    No models available
                  </div>
                )}
              </div>
            </div>

            <div className="mb-5">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
                Response Cache
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={handleClearCache}
                  className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700"
                >
                  Clear Cache
                </button>
                {cacheStatus && <span className="text-xs text-neutral-500 dark:text-neutral-500">{cacheStatus}</span>}
              </div>
            </div>
          </>
        )}

        <div className="flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800">
          <button
            onClick={onClose}
            className="rounded px-3 py-1.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
