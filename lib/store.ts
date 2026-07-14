import { AIConfig, PromptHistoryLog } from "./ai/types";

const STORE_VERSION = 1;

function safeJSONParse(value: string | null) {
  try { return value ? JSON.parse(value) : null; }
  catch { return null; }
}

function migrateConfig(old: any): AIConfig {
  const defaultCfg: AIConfig = {
    version: STORE_VERSION,
    provider: "ollama",
    endpoint: "http://localhost:11434",
    model: "qwen2.5-coder:7b",
    temperature: 0.2,
    context: 8192,
  };
  if (!old) return defaultCfg;
  if (!old.version) return { ...defaultCfg, ...old, version: STORE_VERSION };
  return old as AIConfig;
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