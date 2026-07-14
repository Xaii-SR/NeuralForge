import { ModelCapability } from "./types";

const CAPABILITIES: Record<string, ModelCapability> = {
  "qwen2.5-coder:7b": { name: "qwen2.5-coder:7b", provider: "ollama", context: 32768 },
  "deepseek-coder:latest": { name: "deepseek-coder:latest", provider: "ollama", context: 16384 },
  "deepseek-v4-pro": { name: "deepseek-v4-pro", provider: "deepseek", context: 131072 },
  "gpt-4o": { name: "gpt-4o", provider: "openai", context: 128000 },
  "claude-sonnet-5": { name: "claude-sonnet-5", provider: "anthropic", context: 200000 },
};

const DEFAULT_CAPABILITY: ModelCapability = { name: "unknown", provider: "unknown", context: 8192 };

export function getModelCapabilities(model: string): ModelCapability {
  return CAPABILITIES[model] || { ...DEFAULT_CAPABILITY, name: model };
}