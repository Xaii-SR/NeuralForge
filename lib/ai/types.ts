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

export interface ModelCapability {
  name: string;
  provider: AIProviderId;
  context: number;
}

export interface AIProviderMetadata {
  id: AIProviderId;
  displayName: string;
  models: string[];
}

export interface AIProvider {
  metadata: AIProviderMetadata;
  capabilities: ModelCapability[];
  checkHealth(endpoint: string, signal?: AbortSignal): Promise<boolean>;
  generate(prompt: string, config: AIConfig, signal?: AbortSignal): Promise<string>;
}

export interface GenerationResult {
  text: string;
  provider: string;
  model: string;
  durationMs: number;
  timestamp: number;
  success: boolean;
  tokensUsed?: number;
}

export interface PromptHistoryLog {
  id: string;
  timestamp: number;
  input: string;
  generated: string;
  model: string;
  provider: string;
}