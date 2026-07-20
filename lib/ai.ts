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

export function getEnrichedContext(query: string, maxTokens: number): Promise<string> {
  return invoke("get_enriched_context", { query, maxTokens });
}

export interface FileCandidate {
  path: string;
  score: number;
  match_kind: string;
}

export interface ResolutionResult {
  resolved: string | null;
  candidates: FileCandidate[];
}

export function resolveFileReference(query: string): Promise<ResolutionResult> {
  return invoke("resolve_file_reference", { query });
}

export interface Preferences {
  goal: "speed" | "quality";
  cost_preference: "free" | "cheap" | "quality_first";
}

export interface CostEstimate {
  estimated_tokens: number;
  estimated_cost_usd: number;
  is_free: boolean;
}

export interface AutoSelection {
  provider: string;
  model: string;
  reason: string;
  estimated_cost_usd: number;
  is_free: boolean;
}

export function savePreferences(prefs: Preferences): Promise<void> {
  return invoke("save_preferences", { goal: prefs.goal, costPreference: prefs.cost_preference });
}

export function getPreferences(): Promise<Preferences> {
  return invoke("get_preferences");
}

export function estimateCostForPrompt(prompt: string): Promise<CostEstimate> {
  return invoke("estimate_cost_for_prompt", { prompt });
}

export function clearResponseCache(): Promise<number> {
  return invoke("clear_response_cache");
}

export function autoSelectModel(prompt: string): Promise<AutoSelection> {
  return invoke("auto_select_model", { prompt });
}

// ── Session persistence (v1.3.0 Phase 4A) ──────────────────────────────
// Thin wrappers over the Phase 2 IPC commands (database::*), matching this
// file's existing convention. Session/SessionMessage shapes mirror the
// Rust structs in database/sessions.rs exactly.

export interface Session {
  id: string;
  workspace_path: string;
  title: string;
  provider: string | null;
  active_model: string | null;
  last_message_preview: string | null;
  created_at: number;
  updated_at: number;
}

export interface SessionMessage {
  id: number;
  session_id: string;
  role: string;
  content: string;
  status: string;
  timestamp: number;
}

export function createSession(
  title: string,
  provider?: string | null,
  model?: string | null
): Promise<Session> {
  return invoke("create_session", { title, provider: provider ?? null, model: model ?? null });
}

export function listSessions(): Promise<Session[]> {
  return invoke("list_sessions");
}

export function getSessionMessages(sessionId: string): Promise<SessionMessage[]> {
  return invoke("get_session_messages", { sessionId });
}

export function appendSessionMessage(
  sessionId: string,
  role: string,
  content: string,
  status: string
): Promise<void> {
  return invoke("append_session_message", { sessionId, role, content, status });
}

export function updateSessionMetadata(
  sessionId: string,
  title: string,
  lastMessagePreview: string
): Promise<void> {
  return invoke("update_session_metadata", { sessionId, title, lastMessagePreview });
}

