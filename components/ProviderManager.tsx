"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import * as providers from "@/lib/providers";
import type { ProviderConfig } from "@/lib/providers";
import Spinner from "@/components/ui/Spinner";

const PROVIDER_TYPES = [
  { value: "ollama", label: "Ollama", baseUrl: "http://localhost:11434", notes: "Local models" },
  { value: "lmstudio", label: "LM Studio", baseUrl: "http://localhost:1234", notes: "Local OpenAI-compatible server" },
  { value: "llamacpp", label: "llama.cpp", baseUrl: "http://localhost:8080", notes: "Local OpenAI-compatible server" },
  { value: "localai", label: "LocalAI", baseUrl: "http://localhost:8080", notes: "Local OpenAI-compatible server" },
  { value: "vllm", label: "vLLM", baseUrl: "http://localhost:8000", notes: "Self-hosted OpenAI-compatible server" },
  { value: "sglang", label: "SGLang", baseUrl: "http://localhost:30000", notes: "Self-hosted OpenAI-compatible server" },
  { value: "openai", label: "OpenAI", baseUrl: "https://api.openai.com/v1", notes: "Native OpenAI-compatible API" },
  { value: "anthropic", label: "Anthropic", baseUrl: "https://api.anthropic.com", notes: "Native Claude adapter" },
  { value: "gemini", label: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta", notes: "Native Gemini adapter" },
  { value: "openrouter", label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1", notes: "Aggregator for many frontier/open models" },
  { value: "deepseek", label: "DeepSeek", baseUrl: "https://api.deepseek.com/v1", notes: "OpenAI-compatible API" },
  { value: "xai", label: "xAI Grok", baseUrl: "https://api.x.ai/v1", notes: "OpenAI-compatible API" },
  { value: "mistral", label: "Mistral AI", baseUrl: "https://api.mistral.ai/v1", notes: "OpenAI-compatible API" },
  { value: "groq", label: "GroqCloud", baseUrl: "https://api.groq.com/openai/v1", notes: "OpenAI-compatible API" },
  { value: "together", label: "Together AI", baseUrl: "https://api.together.xyz/v1", notes: "OpenAI-compatible API" },
  { value: "fireworks", label: "Fireworks AI", baseUrl: "https://api.fireworks.ai/inference/v1", notes: "OpenAI-compatible API" },
  { value: "deepinfra", label: "DeepInfra", baseUrl: "https://api.deepinfra.com/v1/openai", notes: "Provider-specific OpenAI-compatible path" },
  { value: "cerebras", label: "Cerebras Cloud", baseUrl: "https://api.cerebras.ai/v1", notes: "OpenAI-compatible API" },
  { value: "sambanova", label: "SambaNova Cloud", baseUrl: "https://api.sambanova.ai/v1", notes: "OpenAI-compatible API" },
  { value: "perplexity", label: "Perplexity", baseUrl: "https://api.perplexity.ai", notes: "OpenAI-compatible chat API" },
  { value: "ai21", label: "AI21 Labs", baseUrl: "https://api.ai21.com/studio/v1", notes: "Use only if your account exposes OpenAI-compatible chat" },
  { value: "cohere", label: "Cohere", baseUrl: "https://api.cohere.com/compatibility/v1", notes: "OpenAI-compatible endpoint only" },
  { value: "qwen", label: "Alibaba Qwen", baseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", notes: "OpenAI-compatible mode" },
  { value: "moonshot", label: "Moonshot Kimi", baseUrl: "https://api.moonshot.ai/v1", notes: "OpenAI-compatible API" },
  { value: "zhipu", label: "Zhipu GLM", baseUrl: "https://open.bigmodel.cn/api/paas/v4", notes: "Versioned compatible API; adjust if your account expects a different root" },
  { value: "minimax", label: "MiniMax", baseUrl: "https://api.minimax.io/v1", notes: "Use only if your account exposes OpenAI-compatible chat" },
  { value: "baidu", label: "Baidu ERNIE", baseUrl: "https://qianfan.baidubce.com/v2", notes: "Use only if your account exposes OpenAI-compatible chat" },
  { value: "tencent", label: "Tencent Hunyuan", baseUrl: "https://api.hunyuan.cloud.tencent.com/v1", notes: "Use only if your account exposes OpenAI-compatible chat" },
  { value: "bytedance", label: "ByteDance Doubao", baseUrl: "https://ark.cn-beijing.volces.com/api/v3", notes: "OpenAI-compatible API" },
  { value: "01ai", label: "01.AI Yi", baseUrl: "https://api.lingyiwanwu.com/v1", notes: "OpenAI-compatible API" },
  { value: "nvidia", label: "NVIDIA NIM", baseUrl: "https://integrate.api.nvidia.com/v1", notes: "OpenAI-compatible API" },
  { value: "ibm", label: "IBM watsonx / Granite", baseUrl: "", notes: "Enter your deployment endpoint" },
  { value: "azure_openai", label: "Azure OpenAI", baseUrl: "", notes: "Enter your Azure deployment endpoint" },
  { value: "bedrock", label: "AWS Bedrock", baseUrl: "", notes: "Use an OpenAI-compatible gateway or custom endpoint" },
  { value: "vertex", label: "Google Vertex AI", baseUrl: "", notes: "Use Gemini native or an OpenAI-compatible gateway" },
  { value: "openai_compatible", label: "Custom OpenAI-Compatible", baseUrl: "http://localhost:1234", notes: "For any compatible endpoint not listed" },
];

const providerTypeByValue = new Map(PROVIDER_TYPES.map((provider) => [provider.value, provider]));

const TASK_KEYS = [
  { key: "active_model_chat", label: "Chat" },
  { key: "active_model_agent", label: "Agent" },
  { key: "active_model_inline", label: "Inline Edit" },
  { key: "active_model_ghost", label: "Ghost Text" },
];

export default function ProviderManager() {
  const [configs, setConfigs] = useState<ProviderConfig[]>([]);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<ProviderConfig | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [testing, setTesting] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [discoveredModels, setDiscoveredModels] = useState<string[]>([]);
  const [taskModels, setTaskModels] = useState<Record<string, string>>({});
  const [expandProvider, setExpandProvider] = useState<string | null>(null);

  // New provider form
  const [newName, setNewName] = useState("");
  const [newType, setNewType] = useState("openai_compatible");
  const [newUrl, setNewUrl] = useState("");
  const [newKey, setNewKey] = useState("");
  const [editModel, setEditModel] = useState("");
  const selectedProviderType = useMemo(() => providerTypeByValue.get(newType), [newType]);

  const load = useCallback(async () => {
    try {
      const list = await providers.listProviderConfigs();
      setConfigs(list);
    } catch { /* workspace not open */ }
    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    // Load saved task model configs
    TASK_KEYS.forEach(async ({ key }) => {
      try {
        const cfg = await providers.getModelConfig(key);
        if (cfg) setTaskModels((prev) => ({ ...prev, [key]: cfg.model }));
      } catch {}
    });
  }, []);

  async function handleAdd() {
    if (!newName.trim() || !newUrl.trim()) return;
    try {
      await providers.addProviderConfig(newName.trim(), newType, newUrl.trim(), newKey.trim());
      setNewName(""); setNewType("openai_compatible"); setNewUrl(""); setNewKey("");
      setShowAdd(false);
      await load();
    } catch (e: any) { setTestResult(`Error: ${e}`); }
  }

  function handleProviderTypeChange(value: string) {
    const preset = providerTypeByValue.get(value);
    setNewType(value);
    setNewName((current) => current.trim() || preset?.label || "");
    setNewUrl((current) => current.trim() || preset?.baseUrl || "");
  }

  async function handleDelete(id: string) {
    try { await providers.deleteProviderConfig(id); await load(); }
    catch (e: any) { setTestResult(`Error: ${e}`); }
  }

  async function handleToggle(id: string, enabled: boolean) {
    try {
      await providers.updateProviderConfig(id, { enabled: !enabled });
      await load();
    } catch {}
  }

  async function handleTestConnection(config: ProviderConfig) {
    setTesting(true); setTestResult(null);
    try {
      const ok = await providers.testProviderConnection(config.provider_type, config.base_url, config.api_key);
      setTestResult(ok ? "✓ Connection successful" : "✗ Connection failed");
    } catch (e: any) { setTestResult(`✗ ${e}`); }
    setTesting(false);
  }

  async function handleDiscoverModels(config: ProviderConfig) {
    setDiscovering(true); setTestResult(null);
    try {
      const models = await providers.listOpenAiModels(config.base_url, config.api_key);
      const modelNames = models.map((m) => m.id);
      setDiscoveredModels(modelNames);
      // Save discovered models to the provider config
      await providers.updateProviderConfig(config.id, { models: modelNames });
      await load();
      setTestResult(`✓ Discovered ${modelNames.length} models`);
    } catch (e: any) { setTestResult(`✗ ${e}`); }
    setDiscovering(false);
  }

  async function handleSetTaskModel(key: string, model: string, config: ProviderConfig) {
    try {
      await providers.setDefaultModel(key, config.id, config.name, model);
      setTaskModels((prev) => ({ ...prev, [key]: model }));
    } catch {}
  }

  if (loading) return <div className="flex items-center justify-center py-8"><Spinner /></div>;

  return (
    <div className="space-y-4 text-xs">
      {/* Provider List */}
      <div>
        <div className="mb-2 flex items-center justify-between">
          <span className="text-xs font-medium uppercase tracking-wide text-neutral-400">Providers</span>
          <button onClick={() => setShowAdd(!showAdd)} className="rounded bg-blue-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-blue-500">
            {showAdd ? "Cancel" : "+ Add Provider"}
          </button>
        </div>

        {/* Add form */}
        {showAdd && (
          <div className="mb-3 rounded border border-blue-200 bg-blue-50 p-3 space-y-2 dark:border-blue-800 dark:bg-blue-900/20">
            <input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="Name (e.g. My DeepSeek)" className="w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800" />
            <select value={newType} onChange={(e) => handleProviderTypeChange(e.target.value)} className="w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800">
              {PROVIDER_TYPES.map((t) => (<option key={t.value} value={t.value}>{t.label}</option>))}
            </select>
            {selectedProviderType?.notes && (
              <div className="rounded bg-white/70 px-2 py-1 text-[10px] text-neutral-500 dark:bg-neutral-900/40 dark:text-neutral-400">
                {selectedProviderType.notes}
              </div>
            )}
            <input value={newUrl} onChange={(e) => setNewUrl(e.target.value)} placeholder="Base URL (e.g. http://localhost:1234/v1)" className="w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800" />
            <input value={newKey} onChange={(e) => setNewKey(e.target.value)} type="password" placeholder="API Key (optional)" className="w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800" />
            <button onClick={handleAdd} className="w-full rounded bg-green-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-green-500">Save Provider</button>
          </div>
        )}

        {/* Provider cards */}
        <div className="space-y-2">
          {configs.map((cfg) => (
            <div key={cfg.id} className={`rounded border p-2.5 transition-colors ${cfg.enabled ? "border-neutral-200 dark:border-neutral-700" : "border-neutral-100 bg-neutral-50 opacity-60 dark:border-neutral-800 dark:bg-neutral-800/50"}`}>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <button onClick={() => handleToggle(cfg.id, cfg.enabled)} className={`h-3 w-3 rounded-full ${cfg.enabled ? "bg-green-500" : "bg-neutral-300 dark:bg-neutral-600"}`} title={cfg.enabled ? "Disable" : "Enable"} />
                  <div>
                    <div className="font-medium text-neutral-700 dark:text-neutral-200">{cfg.name}</div>
                    <div className="text-[10px] text-neutral-400">{cfg.provider_type} · {cfg.base_url}</div>
                  </div>
                </div>
                <div className="flex gap-1">
                  {cfg.provider_type !== "ollama" && (
                    <button onClick={() => handleTestConnection(cfg)} disabled={testing} className="rounded px-2 py-0.5 text-[10px] text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">
                      {testing ? "..." : "Test"}
                    </button>
                  )}
                  <button onClick={() => setExpandProvider(expandProvider === cfg.id ? null : cfg.id)} className="rounded px-2 py-0.5 text-[10px] text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">
                    {expandProvider === cfg.id ? "▲" : "▼"}
                  </button>
                  <button onClick={() => handleDelete(cfg.id)} className="rounded px-1.5 py-0.5 text-[10px] text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30">✕</button>
                </div>
              </div>

              {/* Expanded view */}
              {expandProvider === cfg.id && (
                <div className="mt-2 space-y-2 border-t border-neutral-100 pt-2 dark:border-neutral-700">
                  {/* Model discovery */}
                  <div className="flex gap-2">
                    <button onClick={() => handleDiscoverModels(cfg)} disabled={discovering} className="rounded bg-neutral-100 px-2.5 py-1 text-[10px] font-medium text-neutral-700 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700">
                      {discovering ? "Discovering..." : "🔄 Discover Models"}
                    </button>
                    {discoveredModels.length > 0 && (
                      <span className="text-[10px] text-neutral-400">{discoveredModels.length} models found</span>
                    )}
                  </div>

                  {/* Model list */}
                  {cfg.models.length > 0 && (
                    <div className="rounded bg-neutral-50 p-1.5 dark:bg-neutral-800/50">
                      <div className="text-[10px] text-neutral-400 mb-1">Models</div>
                      <div className="flex flex-wrap gap-1">
                        {cfg.models.map((m) => (
                          <span key={m} className="rounded bg-white px-1.5 py-0.5 text-[10px] text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300">{m}</span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Capability metadata - provider-level, used by capability-based
                      task routing (see provider_router::select_provider_and_model_for_task
                      on the backend). Per-model speed/cost/reasoning scores don't exist
                      yet - these badges reflect what the provider actually declares. */}
                  <div className="rounded bg-neutral-50 p-1.5 dark:bg-neutral-800/50">
                    <div className="text-[10px] text-neutral-400 mb-1">Capabilities</div>
                    <div className="flex flex-wrap gap-1">
                      <span className="rounded bg-white px-1.5 py-0.5 text-[10px] text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300">{cfg.capabilities.context_length.toLocaleString()} ctx</span>
                      {cfg.capabilities.coding && <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] text-blue-600 dark:bg-blue-900/30 dark:text-blue-400">Coding</span>}
                      {cfg.capabilities.vision && <span className="rounded bg-purple-50 px-1.5 py-0.5 text-[10px] text-purple-600 dark:bg-purple-900/30 dark:text-purple-400">Vision</span>}
                      {(cfg.capabilities.tool_calling || cfg.capabilities.function_calling) && <span className="rounded bg-amber-50 px-1.5 py-0.5 text-[10px] text-amber-600 dark:bg-amber-900/30 dark:text-amber-400">Tools</span>}
                      {cfg.capabilities.streaming && <span className="rounded bg-green-50 px-1.5 py-0.5 text-[10px] text-green-600 dark:bg-green-900/30 dark:text-green-400">Streaming</span>}
                      {cfg.capabilities.fim && <span className="rounded bg-teal-50 px-1.5 py-0.5 text-[10px] text-teal-600 dark:bg-teal-900/30 dark:text-teal-400">FIM</span>}
                    </div>
                  </div>

                  {/* Task model assignment */}
                  <div className="space-y-1.5">
                    <div className="text-[10px] text-neutral-400">Assign to tasks</div>
                    {TASK_KEYS.map(({ key, label }) => (
                      <div key={key} className="flex items-center gap-2">
                        <span className="w-16 text-[10px] text-neutral-500">{label}</span>
                        <select
                          value={taskModels[key] || cfg.models[0] || ""}
                          onChange={(e) => handleSetTaskModel(key, e.target.value, cfg)}
                          className="flex-1 rounded border border-neutral-200 bg-white px-2 py-1 text-[10px] outline-none dark:border-neutral-700 dark:bg-neutral-800"
                        >
                          <option value="">-- Select --</option>
                          {cfg.models.map((m) => (<option key={m} value={m}>{m}</option>))}
                        </select>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Test result toast */}
      {testResult && (
        <div className={`rounded p-2 text-xs ${testResult.startsWith("✓") ? "bg-green-50 text-green-700 dark:bg-green-900/30 dark:text-green-400" : "bg-red-50 text-red-700 dark:bg-red-900/30 dark:text-red-400"}`}>
          {testResult}
        </div>
      )}
    </div>
  );
}
