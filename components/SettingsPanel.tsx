"use client";

import { useEffect, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";

export interface SettingsPanelProps { onClose: () => void; }

const OPTION_BASE = "rounded px-3 py-1.5 text-xs font-medium transition-colors";
const OPTION_ACTIVE = "bg-blue-600 text-white";
const OPTION_INACTIVE = "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700";

const PROVIDER_MODELS: Record<string, string[]> = {
  OpenAI: ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.5-pro", "gpt-5.5-base", "gpt-5.4", "gpt-5.1-medium", "o3", "gpt-4o", "gpt-oss-120b", "gpt-oss-20b"],
  Anthropic: ["claude-mythos-5", "claude-fable-5", "claude-opus-4.8", "claude-opus-4.7", "claude-opus-4.6", "claude-sonnet-5", "claude-sonnet-4.6"],
  Google: ["gemini-3.1-pro", "gemini-3.5-flash", "gemini-3-ultra", "gemini-2.5-pro", "gemini-2.0-flash", "gemma-4", "gemma-3"],
  DeepSeek: ["deepseek-v4-pro", "deepseek-v4-flash", "deepseek-v3.2-exp", "deepseek-r1-0528", "deepseek-chat", "deepseek-reasoner", "deepseek-coder:latest"],
  Qwen: ["qwen3.7-max", "qwen3.7-plus", "qwen3-vl-235b", "qwen3-235b", "qwen2.5-coder:7b"],
  xAI: ["grok-4.5", "grok-4", "grok-1"],
  Amazon: ["nova-micro", "alexatm", "titan"],
  Cohere: ["command-a-plus-218b", "command-r-plus", "command-zero"],
  ZhipuAI: ["glm-5.2", "glm-5.1", "glm-4.7-thinking", "glm-4.6"],
  Moonshot: ["kimi-k2.6", "kimi-k2.5", "kimi-k2-thinking", "kimi-k2-0905"],
  Mistral: ["mistral-large-2", "mistral-small-2506", "mixtral-8x7b", "mistral-7b"],
  Microsoft: ["phi-4", "phi-3", "phi-2", "phi-1"],
  Regional_And_Niche: ["apriel-v1.5-15b-thinker", "minimax-m2.5", "falcon-180b", "yandexgpt-2", "granite-3.0", "pangu-sigma-1085b", "mimo-on-device"],
  Custom: [],
};

const REMOTE_PROVIDERS = ["DeepSeek", "Anthropic", "OpenAI", "Custom"];

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [prefs, setPrefs] = useState<ai.Preferences>({ goal: "speed", cost_preference: "free" });
  const [models, setModels] = useState<ai.OllamaModel[]>([]);
  const [benchmarks, setBenchmarks] = useState<Record<string, ai.BenchmarkResult>>({});
  const [benchmarking, setBenchmarking] = useState<string | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const [provider, setProvider] = useState("Ollama");
  const [selectedModel, setSelectedModel] = useState("");
  const [customModelInput, setCustomModelInput] = useState("");
  const [customApiKey, setCustomApiKey] = useState("");

  const isRemote = REMOTE_PROVIDERS.includes(provider);
  const availableModels = PROVIDER_MODELS[provider] || [];

  useEffect(() => {
    const savedProvider = localStorage.getItem("nf_provider") || "Ollama";
    const savedModel = localStorage.getItem("nf_custom_model") || "";
    const savedKey = localStorage.getItem("nf_custom_api_key") || "";
    setProvider(savedProvider);
    setSelectedModel(savedModel);
    setCustomModelInput(savedModel);
    setCustomApiKey(savedKey);
  }, []);

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
    function onKeyDown(e: KeyboardEvent) { if (e.key === "Escape") onClose(); }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  function handleSave() {
    const finalModel = provider === "Custom" ? customModelInput : selectedModel;
    localStorage.setItem("nf_provider", provider);
    localStorage.setItem("nf_custom_model", finalModel);
    localStorage.setItem("nf_custom_api_key", customApiKey);
    window.dispatchEvent(new Event("nf_settings_updated"));
    ai.savePreferences(prefs).then(() => onClose());
  }

  async function handleBenchmark(model: string) {
    setBenchmarking(model);
    try { const r = await ai.runModelBenchmark(model); setBenchmarks((p) => ({ ...p, [model]: r })); }
    finally { setBenchmarking(null); }
  }

  async function handleClearCache() {
    const count = await ai.clearResponseCache();
    setCacheStatus(`Cleared ${count} cached response${count === 1 ? "" : "s"}`);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()} className="max-h-[85vh] w-[520px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">Settings</h2>
          <button onClick={onClose} className="rounded px-1.5 py-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
        </div>
        {loading ? <div className="flex items-center justify-center gap-2 py-12 text-xs text-neutral-500"><Spinner /> Loading settings...</div> : (<>
          {/* ── Provider / Model Selector ── */}
          <div className="mb-5 space-y-3">
            <div>
              <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Provider</div>
              <select value={provider} onChange={(e) => { setProvider(e.target.value); setSelectedModel(""); }}
                className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                {Object.keys(PROVIDER_MODELS).map((p) => (<option key={p} value={p}>{p}</option>))}
              </select>
            </div>
            {provider !== "Custom" ? (
              <div>
                <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Model</div>
                <select value={selectedModel} onChange={(e) => setSelectedModel(e.target.value)}
                  className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                  <option value="">Select a model...</option>
                  {availableModels.map((m) => (<option key={m} value={m}>{m}</option>))}
                </select>
              </div>
            ) : (
              <div>
                <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Custom Model</div>
                <input type="text" value={customModelInput} onChange={(e) => setCustomModelInput(e.target.value)} placeholder="e.g., gpt-4o or claude-3"
                  className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
              </div>
            )}
          </div>

          {/* ── API Key (conditional) ── */}
          {isRemote && (
            <div className="mb-5">
              <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">{provider} API Key</div>
              <input type="password" value={customApiKey} onChange={(e) => setCustomApiKey(e.target.value)} placeholder="sk-..."
                className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
            </div>
          )}

          <div className="mb-5">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Goal</div>
            <div className="flex gap-2">
              {(["speed","quality"] as const).map((g) => (<button key={g} onClick={() => setPrefs((p) => ({ ...p, goal: g }))} className={`${OPTION_BASE} ${prefs.goal === g ? OPTION_ACTIVE : OPTION_INACTIVE}`}>{g === "speed" ? "Fast" : "Best Quality"}</button>))}
            </div>
          </div>
          <div className="mb-5">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Cost preference</div>
            <div className="flex gap-2">
              {(["free","cheap","quality_first"] as const).map((c) => (<button key={c} onClick={() => setPrefs((p) => ({ ...p, cost_preference: c }))} className={`${OPTION_BASE} ${prefs.cost_preference === c ? OPTION_ACTIVE : OPTION_INACTIVE}`}>{c === "free" ? "Free only" : c === "cheap" ? "Cheap OK" : "Quality first"}</button>))}
            </div>
          </div>
          <div className="mb-5">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Model Benchmarks</div>
            <div className="space-y-1.5">
              {models.map((m) => { const b = benchmarks[m.name]; return (<div key={m.name} className="flex items-center justify-between rounded border border-neutral-200 bg-neutral-50 px-2.5 py-1.5 dark:border-neutral-800 dark:bg-neutral-800/60"><div className="min-w-0 flex-1 truncate"><div className="truncate text-xs font-medium">{m.name}</div>{b && <div className="text-[10px] text-neutral-500">{b.tokens_per_second ? `${b.tokens_per_second.toFixed(1)} tok/s` : "n/a"} · {b.latency_ms.toFixed(0)}ms · <span className={b.reliable ? "text-green-600" : "text-red-500"}>{b.reliable ? "reliable" : "unreliable"}</span></div>}</div><button onClick={() => handleBenchmark(m.name)} disabled={benchmarking === m.name} className="ml-2 flex shrink-0 items-center gap-1 rounded bg-neutral-200 px-2 py-1 text-[10px] font-medium text-neutral-700 hover:bg-neutral-300 disabled:opacity-50 dark:bg-neutral-700 dark:text-neutral-200 dark:hover:bg-neutral-600">{benchmarking === m.name && <Spinner size={9} />}{benchmarking === m.name ? "Running" : b ? "Re-run" : "Benchmark"}</button></div>); })}
            </div>
          </div>
          <div className="mb-5">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Response Cache</div>
            <div className="flex items-center gap-2"><button onClick={handleClearCache} className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700">Clear Cache</button>{cacheStatus && <span className="text-xs text-neutral-500">{cacheStatus}</span>}</div>
          </div>
        </>)}
        <div className="flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800">
          <button onClick={onClose} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Cancel</button>
          <button onClick={handleSave} className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 shadow-lg">Save</button>
        </div>
      </div>
    </div>
  );
}