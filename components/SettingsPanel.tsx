"use client";

import { useEffect, useState } from "react";
import * as ai from "@/lib/ai";

export interface SettingsPanelProps {
  onClose: () => void;
}

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [prefs, setPrefs] = useState<ai.Preferences>({ goal: "speed", cost_preference: "free" });
  const [models, setModels] = useState<ai.OllamaModel[]>([]);
  const [benchmarks, setBenchmarks] = useState<Record<string, ai.BenchmarkResult>>({});
  const [benchmarking, setBenchmarking] = useState<string | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);

  useEffect(() => {
    ai.getPreferences().then(setPrefs);
    ai.listModels().then(setModels).catch(() => setModels([]));
    ai.getBenchmarks().then((list) => {
      const map: Record<string, ai.BenchmarkResult> = {};
      for (const b of list) map[b.model] = b;
      setBenchmarks(map);
    });
  }, []);

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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-[480px] max-h-[80vh] overflow-y-auto rounded bg-neutral-900 p-4 text-sm text-neutral-200 shadow-xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold">Settings</h2>
          <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
            ✕
          </button>
        </div>

        <div className="mb-4">
          <div className="mb-1 text-xs uppercase text-neutral-500">Goal</div>
          <div className="flex gap-2">
            {(["speed", "quality"] as const).map((g) => (
              <button
                key={g}
                onClick={() => setPrefs((p) => ({ ...p, goal: g }))}
                className={`rounded px-3 py-1 text-xs ${
                  prefs.goal === g ? "bg-blue-600 text-white" : "bg-neutral-800 text-neutral-300"
                }`}
              >
                {g === "speed" ? "Fast" : "Best Quality"}
              </button>
            ))}
          </div>
        </div>

        <div className="mb-4">
          <div className="mb-1 text-xs uppercase text-neutral-500">Cost preference</div>
          <div className="flex gap-2">
            {(["free", "cheap", "quality_first"] as const).map((c) => (
              <button
                key={c}
                onClick={() => setPrefs((p) => ({ ...p, cost_preference: c }))}
                className={`rounded px-3 py-1 text-xs ${
                  prefs.cost_preference === c ? "bg-blue-600 text-white" : "bg-neutral-800 text-neutral-300"
                }`}
              >
                {c === "free" ? "Free only" : c === "cheap" ? "Cheap OK" : "Quality first"}
              </button>
            ))}
          </div>
        </div>

        <div className="mb-4">
          <div className="mb-1 flex items-center justify-between text-xs uppercase text-neutral-500">
            <span>Model Benchmarks</span>
          </div>
          <div className="space-y-1">
            {models.map((m) => {
              const b = benchmarks[m.name];
              return (
                <div key={m.name} className="flex items-center justify-between rounded bg-neutral-800 px-2 py-1">
                  <div className="min-w-0 flex-1 truncate">
                    <div className="truncate text-xs">{m.name}</div>
                    {b && (
                      <div className="text-[10px] text-neutral-500">
                        {b.tokens_per_second ? `${b.tokens_per_second.toFixed(1)} tok/s` : "n/a"} ·{" "}
                        {b.latency_ms.toFixed(0)}ms latency · {b.reliable ? "reliable" : "unreliable"}
                      </div>
                    )}
                  </div>
                  <button
                    onClick={() => handleBenchmark(m.name)}
                    disabled={benchmarking === m.name}
                    className="ml-2 shrink-0 rounded bg-neutral-700 px-2 py-0.5 text-[10px] hover:bg-neutral-600 disabled:opacity-50"
                  >
                    {benchmarking === m.name ? "Running..." : b ? "Re-run" : "Benchmark"}
                  </button>
                </div>
              );
            })}
            {models.length === 0 && <div className="text-xs text-neutral-500">No models available</div>}
          </div>
        </div>

        <div className="mb-4">
          <div className="mb-1 text-xs uppercase text-neutral-500">Response Cache</div>
          <button
            onClick={handleClearCache}
            className="rounded bg-neutral-800 px-3 py-1 text-xs hover:bg-neutral-700"
          >
            Clear Cache
          </button>
          {cacheStatus && <span className="ml-2 text-xs text-neutral-500">{cacheStatus}</span>}
        </div>

        <div className="flex justify-end gap-2">
          <button onClick={onClose} className="rounded px-3 py-1 text-xs text-neutral-400 hover:bg-neutral-800">
            Cancel
          </button>
          <button onClick={handleSave} className="rounded bg-blue-600 px-3 py-1 text-xs text-white hover:bg-blue-500">
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
