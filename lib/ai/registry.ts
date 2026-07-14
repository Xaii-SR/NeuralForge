import { ModelCapability } from "./types";

const CAPABILITIES: Record<string, ModelCapability> = {
  "qwen2.5-coder:7b": { name: "qwen2.5-coder:7b", provider: "ollama", context: 32768, type: "coding", supportsTools: false, supportsVision: false, supportsStreaming: true },
  "deepseek-coder:latest": { name: "deepseek-coder:latest", provider: "ollama", context: 16384, type: "coding", supportsTools: false, supportsVision: false, supportsStreaming: true },
  "deepseek-v4-pro": { name: "deepseek-v4-pro", provider: "deepseek", context: 131072, type: "coding", supportsTools: true, supportsVision: false, supportsStreaming: true },
  "gpt-4o": { name: "gpt-4o", provider: "openai", context: 128000, type: "general", supportsTools: true, supportsVision: true, supportsStreaming: true },
  "claude-sonnet-5": { name: "claude-sonnet-5", provider: "anthropic", context: 200000, type: "coding", supportsTools: true, supportsVision: true, supportsStreaming: true },
};

const DEFAULT_CAPABILITY: ModelCapability = { name: "unknown", provider: "unknown", context: 8192, type: "general", supportsTools: false, supportsVision: false, supportsStreaming: false };

export function getModelCapabilities(model: string): ModelCapability {
  return CAPABILITIES[model] || { ...DEFAULT_CAPABILITY, name: model };
}