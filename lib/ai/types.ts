export interface AIConfig {
  version: number;
  provider: "ollama" | "lmstudio" | "openai_compatible";
  endpoint: string;
  model: string;
  temperature: number;
  context: number;
}

export interface ModelCapability {
  name: string;
  provider: string;
  context: number;
  type: "coding" | "reasoning" | "vision" | "general";
  supportsTools: boolean;
  supportsVision: boolean;
  supportsStreaming: boolean;
}

export interface PromptHistoryLog {
  id: string;
  timestamp: number;
  input: string;
  generated: string;
  model: string;
  provider: string;
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

export interface AIProvider {
  checkHealth(endpoint: string, signal?: AbortSignal): Promise<boolean>;
  generate(prompt: string, config: AIConfig, signal?: AbortSignal): Promise<string>;
  generateStream?(prompt: string, config: AIConfig, signal?: AbortSignal): AsyncGenerator<string>;
}