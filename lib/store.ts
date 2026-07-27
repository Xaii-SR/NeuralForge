import { migrateLegacyApiKey } from "@/lib/providers";

export type AIProviderId = string;

export interface AIConfig {
  version: number;
  provider: AIProviderId;
  endpoint: string;
  model: string;
  temperature: number;
  context: number;
  apiKeyRef?: string;
  shareWorkspaceContextWithCloud: boolean;
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
    shareWorkspaceContextWithCloud: false,
    effort: "High",
  };
  const source = old ?? defaultCfg;
  const legacyApiKey = typeof source.apiKey === "string" && source.apiKey
    ? source.apiKey
    : localStorage.getItem("nf_api_key_backup");
  let credentialMigrated = false;
  if (legacyApiKey) {
    const providerHint = typeof source.provider === "string" && source.provider
      ? source.provider
      : defaultCfg.provider;
    try {
      await migrateLegacyApiKey(providerHint, legacyApiKey);
      credentialMigrated = true;
    } catch {
      credentialMigrated = false;
    }
  }

  if (!source.version) {
    const migrated = { ...defaultCfg, ...source, version: STORE_VERSION, effort: source.effort || "High" } as AIConfig;
    if (credentialMigrated) {
      migrated.apiKeyRef = "migrated-key";
    }
    delete (migrated as AIConfig & { apiKey?: string }).apiKey;
    if (!legacyApiKey || credentialMigrated) {
      localStorage.removeItem("nf_api_key_backup");
      localStorage.setItem("nf_app_config", JSON.stringify(migrated));
    }
    return migrated;
  }
  const migrated = { ...source, effort: source.effort || "High" } as AIConfig & { apiKey?: string };
  delete migrated.apiKey;
  if (credentialMigrated) {
    migrated.apiKeyRef = "migrated-key";
    localStorage.removeItem("nf_api_key_backup");
    localStorage.setItem("nf_app_config", JSON.stringify(migrated));
  }
  return {
    ...migrated,
    shareWorkspaceContextWithCloud: migrated.shareWorkspaceContextWithCloud ?? false,
  };
}

export async function getAppConfig(): Promise<AIConfig> {
  const local = safeJSONParse(localStorage.getItem("nf_app_config"));
  return migrateConfig(local);
}

export async function saveAppConfig(config: AIConfig): Promise<void> {
  const existing = safeJSONParse(localStorage.getItem("nf_app_config"));
  if (existing?.apiKey || localStorage.getItem("nf_api_key_backup")) {
    throw new Error("Provider credential migration must complete before settings can be saved.");
  }
  config.version = STORE_VERSION;
  localStorage.setItem("nf_app_config", JSON.stringify(config));
}
