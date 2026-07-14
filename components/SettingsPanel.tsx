"use client";

import { useEffect, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";

export interface SettingsPanelProps { onClose: () => void; }

const PROVIDER_MODELS: Record<string, string[]> = {
  "DeepSeek": ["deepseek-v4-pro", "deepseek-v4-flash", "deepseek-r1-0528", "deepseek-chat", "deepseek-reasoner"],
  "Anthropic": ["claude-mythos-5", "claude-fable-5", "claude-opus-4.8", "claude-sonnet-5"],
  "OpenAI": ["gpt-5.6-sol", "gpt-5.5-pro", "o3", "gpt-4o"],
  "Google": ["gemini-3.1-pro", "gemini-2.5-pro", "gemma-4"],
  "Ollama": ["qwen2.5-coder:7b", "deepseek-coder:latest", "llama3.1:latest"],
  "Qwen": ["qwen3.7-max", "qwen3-vl-235b", "qwen2.5-coder:7b"],
  "xAI": ["grok-4.5", "grok-4"],
  "Cohere": ["command-a-plus-218b", "command-r-plus"],
  "Mistral": ["mistral-large-2", "mistral-small-2506"],
  "Custom": [],
};

const REMOTE_PROVIDERS = ["DeepSeek", "Anthropic", "OpenAI", "Google", "Cohere", "Mistral", "Custom"];

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

  function handleProviderChange(newProvider: string) {
    setProvider(newProvider);
    setSelectedModel("");
    setCustomModelInput("");
  }

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
          <button onClick={onClose} className="rounded px-1.5 py-0.5 text-neutral-400 hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
        </div>
        {loading ? <div className="flex items-center justify-center gap-2 py-12 text-xs text-neutral-500"><Spinner /> Loading...</div> : (<>
          <div className="mb-5 space-y-3">
            <div>
              <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Provider</div>
              <select value={provider} onChange={(e) => handleProviderChange(e.target.value)} className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                {Object.keys(PROVIDER_MODELS).map((p) => (<option key={p} value={p}>{p}</option>))}
              </select>
            </div>
            {provider !== "Custom" ? (
              <div>
                <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Model</div>
                <select value={selectedModel} onChange={(e) => setSelectedModel(e.target.value)} className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                  <option value="">Select a model...</option>
                  {availableModels.map((m) => (<option key={m} value={m}>{m}</option>))}
                </select>
              </div>
            ) : (<div><div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Custom Model</div><input type="text" value={customModelInput} onChange={(e) => setCustomModelInput(e.target.value)} placeholder="e.g., gpt-4o" className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" /></div>)}
          </div>
          {isRemote && (<div className="mb-5"><div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">{provider} API Key</div><input type="password" value={customApiKey} onChange={(e) => setCustomApiKey(e.target.value)} placeholder="sk-..." className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" /></div>)}
          <div className="mb-5"><div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Model Benchmarks</div><div className="space-y-1.5">{models.map((m) => { const b = benchmarks[m.name]; return (<div key={m.name} className="flex items-center justify-between rounded border border-neutral-200 bg-neutral-50 px-2.5 py-1.5 dark:border-neutral-800 dark:bg-neutral-800/60"><div className="min-w-0 flex-1 truncate"><div className="truncate text-xs font-medium">{m.name}</div>{b && <div className="text-[10px] text-neutral-500">{b.tokens_per_second ? `${b.tokens_per_second.toFixed(1)} tok/s` : "n/a"} · {b.latency_ms.toFixed(0)}ms</div>}</div><button onClick={() => handleBenchmark(m.name)} disabled={benchmarking === m.name} className="ml-2 flex shrink-0 items-center gap-1 rounded bg-neutral-200 px-2 py-1 text-[10px] font-medium text-neutral-700 hover:bg-neutral-300 disabled:opacity-50 dark:bg-neutral-700 dark:text-neutral-200 dark:hover:bg-neutral-600">{benchmarking === m.name && <Spinner size={9} />}{benchmarking === m.name ? "Running" : b ? "Re-run" : "Benchmark"}</button></div>); })}</div></div>
          <div className="mb-5"><div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Response Cache</div><div className="flex items-center gap-2"><button onClick={handleClearCache} className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700">Clear Cache</button>{cacheStatus && <span className="text-xs text-neutral-500">{cacheStatus}</span>}</div></div>
        </>)}
        <div className="flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800"><button onClick={onClose} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Cancel</button><button onClick={handleSave} className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-blue-500 shadow-lg">Save</button></div>
      </div>
    </div>
  );
}