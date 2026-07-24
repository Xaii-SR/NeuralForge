"use client";

import { useCallback, useEffect, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import ErrorBanner from "@/components/ui/ErrorBanner";
import { getAppConfig, inferEffortForModel, saveAppConfig } from "@/lib/store";
import ProviderManager from "@/components/ProviderManager";
import { getBuildInfo, type BuildInfo } from "@/lib/buildInfo";

export interface SettingsPanelProps { onClose: () => void; }

/**
 * Ollama is always available with zero configuration and is the fallback
 * used whenever a chat model isn't found in any configured cloud/custom
 * provider (see provider_router::resolve_provider_for_model on the backend).
 * These are its known-good defaults for when Ollama itself is unreachable
 * (so the dropdown is never empty) - not a list of "the only providers",
 * which now also includes whatever is configured below via ProviderManager.
 */
const FALLBACK_MODELS = ["qwen2.5-coder:7b", "deepseek-coder:latest", "llama3.1:latest"];

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [prefs, setPrefs] = useState<ai.Preferences>({ goal: "speed", cost_preference: "free" });
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [selectedModel, setSelectedModel] = useState("");
  const [installedModels, setInstalledModels] = useState<string[]>([]);
  const [endpoint, setEndpoint] = useState("http://localhost:11434");
  const [effort, setEffort] = useState<"Light" | "Medium" | "High" | "Extra High">("High");
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);

  useEffect(() => {
    getBuildInfo().then(setBuildInfo).catch(() => setBuildInfo(null));
  }, []);

  useEffect(() => {
    getAppConfig().then((config) => {
      setSelectedModel(config.model);
      setEndpoint(config.endpoint);
      setEffort(config.effort);
    });
  }, []);

  // A genuine listModels() rejection (e.g. Ollama isn't running) is expected
  // and already handled gracefully below via FALLBACK_MODELS - that's not
  // an error worth interrupting the user over. What this guards against is
  // different: a *resolved* value that isn't the array listModels() is
  // typed to return, which the old code fed straight into `.map()` with no
  // check, silently hanging the panel on "Loading..." forever if it ever
  // happened (reproduced by mocking the IPC layer to resolve with `null`).
  // That specific case now surfaces as a real, retryable error instead.
  const loadSettings = useCallback(() => {
    setLoading(true);
    setLoadError(null);
    Promise.all([
      ai.getPreferences().catch(() => ({ goal: "speed", cost_preference: "free" } as ai.Preferences)),
      ai.listModels().catch(() => [] as ai.OllamaModel[]),
    ])
      .then(([loadedPrefs, models]) => {
        if (!Array.isArray(models)) {
          throw new Error("list_models returned an unexpected response");
        }
        setPrefs(loadedPrefs);
        setInstalledModels(models.map((m) => m.name));
        setLoading(false);
      })
      .catch((err: any) => {
        setLoadError(err?.message ? String(err.message) : String(err));
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) { if (e.key === "Escape") onClose(); }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  // Prefer the live list of installed models; fall back to known defaults
  // when Ollama is unreachable so the dropdown is never empty.
  const modelOptions = installedModels.length > 0 ? installedModels : FALLBACK_MODELS;
  const modelChoices = selectedModel && !modelOptions.includes(selectedModel)
    ? [selectedModel, ...modelOptions]
    : modelOptions;

  async function handleSave() {
    const baseConfig = await getAppConfig();
    await saveAppConfig({
      ...baseConfig,
      provider: "ollama",
      model: selectedModel,
      endpoint,
      effort,
    });

    window.dispatchEvent(new Event("nf_settings_updated"));
    await ai.savePreferences(prefs);
    onClose();
  }

  function handleModelChange(model: string) {
    setSelectedModel(model);
    setEffort(inferEffortForModel(model));
  }

  async function handleClearCache() {
    const count = await ai.clearResponseCache();
    setCacheStatus(`Cleared ${count} cached response${count === 1 ? "" : "s"}`);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()} className="max-h-[85vh] w-[640px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">Settings</h2>
          <button onClick={onClose} aria-label="Close settings" className="rounded px-1.5 py-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
        </div>
        {loading ? <div className="flex items-center justify-center gap-2 py-12 text-xs text-neutral-500"><Spinner /> Loading...</div> : (<>
          {loadError && (
            <div className="mb-5">
              <ErrorBanner message={`Could not load settings: ${loadError}`} onRetry={loadSettings} onDismiss={() => setLoadError(null)} />
            </div>
          )}
          <div className="mb-5 space-y-3">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Default (Ollama)</div>
            <div>
              <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Model</div>
              <select value={selectedModel} onChange={(e) => handleModelChange(e.target.value)} className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                {modelChoices.map((m) => (<option key={m} value={m}>{m}</option>))}
              </select>
              {installedModels.length === 0 && <div className="mt-1 text-[11px] text-neutral-400 dark:text-neutral-500">Could not list installed models - showing defaults. Verify Ollama is running.</div>}
            </div>
          </div>
          <div className="mb-5">
            <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Endpoint</div>
            <input type="text" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} placeholder="http://localhost:11434" className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>
          <div className="mb-5">
            <div className="mb-1 text-xs font-medium uppercase tracking-wide text-neutral-400">Effort</div>
            <select value={effort} onChange={(e) => setEffort(e.target.value as "Light" | "Medium" | "High" | "Extra High")} className="w-full rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
              <option value="Light">Light</option>
              <option value="Medium">Medium</option>
              <option value="High">High</option>
              <option value="Extra High">Extra High</option>
            </select>
            <div className="mt-1 text-[11px] text-neutral-400 dark:text-neutral-500">Changing the model auto-selects a matching effort level; you can still override it here.</div>
          </div>
          <div className="mb-5 border-t border-neutral-100 pt-4 dark:border-neutral-800">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Cloud &amp; Custom Providers</div>
            <div className="mb-2 text-[11px] text-neutral-400 dark:text-neutral-500">
              Any model added here becomes selectable across the app; chat automatically routes to whichever provider owns the model. Ollama remains the default when a model isn&apos;t found below.
            </div>
            <ProviderManager />
          </div>
          <div className="mb-5"><div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">Response Cache</div><div className="flex items-center gap-2"><button onClick={handleClearCache} className="rounded bg-neutral-100 px-3 py-1.5 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700">Clear Cache</button>{cacheStatus && <span className="text-xs text-neutral-500">{cacheStatus}</span>}</div></div>
          {buildInfo && (
            <div className="mb-5 border-t border-neutral-100 pt-4 dark:border-neutral-800">
              <div className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-400">About</div>
              <div className="space-y-0.5 text-[11px] text-neutral-500 dark:text-neutral-400">
                <div>Version: {buildInfo.version}</div>
                <div>Commit: {buildInfo.commit}</div>
                <div>Built: {buildInfo.build_time === "unknown" ? "unknown" : new Date(Number(buildInfo.build_time) * 1000).toLocaleString()}</div>
              </div>
            </div>
          )}
        </>)}
        <div className="flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800"><button onClick={onClose} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Cancel</button><button onClick={handleSave} className="rounded bg-blue-600 px-4 py-1.5 text-xs font-medium text-white shadow-lg transition-colors hover:bg-blue-500">Save</button></div>
      </div>
    </div>
  );
}
