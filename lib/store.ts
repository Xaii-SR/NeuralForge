export type AIProviderId = string;

export interface AIConfig {
  version: number;
  provider: AIProviderId;
  endpoint: string;
  model: string;
  temperature: number;
  context: number;
  apiKeyRef?: string;
  effort: "Light" | "Medium" | "High" | "Extra High";
}

const STORE_VERSION = 1;
export type EffortLevel = AIConfig["effort"];

export function inferEffortForModel(model: string): EffortLevel {
  const normalized = model.toLowerCase();
  if (/(reason|thinking|r1|o3|o4|opus|pro|max|large|70b|72b|120b|405b)/.test(normalized)) {
    return "Extra High";
  }
  if (/(coder|code|sonnet|medium|32b|34b|40b|mixtral|jamba)/.test(normalized)) {
    return "High";
  }
  if (/(mini|small|flash|haiku|lite|fast|8b|7b|3b|1b)/.test(normalized)) {
    return "Medium";
  }
  return "High";
}

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
