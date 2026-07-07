import { invoke } from "@tauri-apps/api/core";

export interface OllamaModel {
  name: string;
  size_bytes: number;
  parameter_size: string;
  quantization_level: string;
  context_length: number;
  family: string;
}

export interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
}

export interface ProviderMetadata {
  id: string;
  name: string;
  is_local: boolean;
  requires_api_key: boolean;
  configured: boolean;
}

export interface ProviderHealthInfo {
  provider: string;
  healthy: boolean;
  avg_latency_ms: number | null;
  failure_count: number;
  cooldown_seconds_remaining: number | null;
}

export interface VramCheckResult {
  sufficient: boolean;
  required_mb: number;
  available_mb: number;
  message: string;
}

export interface HardwareInfo {
  cpu: { brand: string; physical_cores: number; logical_cores: number; frequency_mhz: number };
  memory: { total_mb: number; available_mb: number };
  gpus: { name: string; vendor: string; vram_mb: number; utilization_percent: number | null }[];
}

export function ollamaHealthCheck(): Promise<boolean> {
  return invoke("ollama_health_check");
}

export function listModels(): Promise<OllamaModel[]> {
  return invoke("list_models");
}

export function pullModel(name: string): Promise<void> {
  return invoke("pull_model", { name });
}

export function removeModel(name: string): Promise<void> {
  return invoke("remove_model", { name });
}

export function listProviders(): Promise<ProviderMetadata[]> {
  return invoke("list_providers");
}

export function getProviderHealth(): Promise<ProviderHealthInfo[]> {
  return invoke("get_provider_health");
}

export function checkVramForModel(
  parameterSize: string,
  quantizationLevel: string
): Promise<VramCheckResult> {
  return invoke("check_vram_for_model", {
    parameterSize,
    quantizationLevel,
  });
}

export function getHardwareInfo(): Promise<HardwareInfo> {
  return invoke("get_hardware_info");
}

export function chatWithModel(
  requestId: string,
  model: string,
  messages: ChatMessage[]
): Promise<void> {
  return invoke("chat_with_model", { requestId, model, messages });
}

export interface IndexStats {
  files_scanned: number;
  files_indexed: number;
  files_skipped_unchanged: number;
  chunks_created: number;
}

export interface SearchResult {
  path: string;
  start_line: number;
  end_line: number;
  content: string;
  score: number;
}

export function indexWorkspace(): Promise<IndexStats> {
  return invoke("index_workspace");
}

export function searchWorkspace(query: string): Promise<SearchResult[]> {
  return invoke("search_workspace", { query });
}

export function getContextForQuery(query: string): Promise<string> {
  return invoke("get_context_for_query", { query });
}
