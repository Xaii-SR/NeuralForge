import { AIConfig, PromptHistoryLog } from "./ai/types";

const STORE_VERSION = 1;

function safeJSONParse(value: string | null) {
  try { return value ? JSON.parse(value) : null; }
  catch { return null; }
}

export async function migrateConfig(old: any): Promise<AIConfig> {
  const defaultCfg: AIConfig = {
    version: STORE_VERSION,
    provider: "ollama",
    endpoint: "http://localhost:11434",
    model: "qwen2.5-coder:7b",
    temperature: 0.2,
    context: 8192,
    effort: "High",
  };
  if (!old) return defaultCfg;

  if (!old.version) {
    const migrated = { ...defaultCfg, ...old, version: STORE_VERSION, effort: old.effort || "High" } as AIConfig;
    if (old.apiKey) {
      localStorage.setItem("nf_api_key_backup", old.apiKey);
      delete (migrated as any).apiKey;
      migrated.apiKeyRef = "migrated-key";
    }
    return migrated;
  }
  return { ...old, effort: old.effort || "High" } as AIConfig;
}

export async function getAppConfig(): Promise<AIConfig> {
  const local = safeJSONParse(localStorage.getItem("nf_app_config"));
  return migrateConfig(local);
}

export async function saveAppConfig(config: AIConfig): Promise<void> {
  config.version = STORE_VERSION;
  localStorage.setItem("nf_app_config", JSON.stringify(config));
}

export async function getPromptHistory(): Promise<PromptHistoryLog[]> {
  return safeJSONParse(localStorage.getItem("nf_prompt_history")) || [];
}

export async function savePromptHistory(log: PromptHistoryLog): Promise<void> {
  const history = await getPromptHistory();
  const updated = [log, ...history].slice(0, 100);
  localStorage.setItem("nf_prompt_history", JSON.stringify(updated));
}