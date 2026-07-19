import { invoke } from "@tauri-apps/api/core";

export interface ProviderConfig {
  id: string;
  name: string;
  provider_type: string;
  base_url: string;
  api_key: string;
  models: string[];
  enabled: boolean;
  is_default: boolean;
  capabilities: ProviderCapabilities;
  created_at: number;
}

export interface ProviderCapabilities {
  chat: boolean;
  streaming: boolean;
  coding: boolean;
  vision: boolean;
  tool_calling: boolean;
  function_calling: boolean;
  embeddings: boolean;
  fim: boolean;
  context_length: number;
}

export interface ModelConfig {
  provider_id: string;
  provider_name: string;
  model: string;
}

export interface OpenAiModel {
  id: string;
  object: string;
  owned_by: string;
}

// Provider CRUD
export function listProviderConfigs(): Promise<ProviderConfig[]> {
  return invoke("list_provider_configs");
}

export function addProviderConfig(
  name: string,
  provider_type: string,
  base_url: string,
  api_key: string,
): Promise<ProviderConfig> {
  return invoke("add_provider_config", { name, providerType: provider_type, baseUrl: base_url, apiKey: api_key });
}

export function updateProviderConfig(
  id: string,
  fields: {
    name?: string;
    base_url?: string;
    api_key?: string;
    enabled?: boolean;
    models?: string[];
  },
): Promise<ProviderConfig> {
  return invoke("update_provider_config", {
    id,
    name: fields.name ?? null,
    baseUrl: fields.base_url ?? null,
    apiKey: fields.api_key ?? null,
    enabled: fields.enabled ?? null,
    models: fields.models ?? null,
  });
}

export function deleteProviderConfig(id: string): Promise<void> {
  return invoke("delete_provider_config", { id });
}

// Model configuration per task type
export function setDefaultModel(key: string, providerId: string, providerName: string, model: string): Promise<void> {
  return invoke("set_default_model", { key, providerId, providerName, model });
}

export function getModelConfig(key: string): Promise<ModelConfig | null> {
  return invoke("get_model_config", { key });
}

// OpenAI-compatible testing
export function testOpenAiConnection(baseUrl: string, apiKey: string): Promise<boolean> {
  return invoke("test_openai_compatible_connection", { baseUrl, apiKey });
}

// Dispatches to the correct adapter's real health check based on
// providerType (ollama/openai_compatible/anthropic/gemini/...), instead of
// always testing via the OpenAI-compatible client regardless of provider.
export function testProviderConnection(providerType: string, baseUrl: string, apiKey: string): Promise<boolean> {
  return invoke("test_provider_connection", { providerType, baseUrl, apiKey });
}

export function listOpenAiModels(baseUrl: string, apiKey: string): Promise<OpenAiModel[]> {
  return invoke("list_openai_compatible_models", { baseUrl, apiKey });
}