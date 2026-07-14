import { AIProvider } from "./types";

const providers = new Map<string, AIProvider>();

export function normalizeProviderId(id: string): string {
  return id.toLowerCase().trim().replace(/_/g, "").replace(/-/g, "");
}

export function registerProvider(provider: AIProvider): void {
  providers.set(normalizeProviderId(provider.metadata.id), provider);
}

export function getRegisteredProvider(id: string): AIProvider | undefined {
  return providers.get(normalizeProviderId(id));
}

export function getAllProviders(): AIProvider[] {
  return Array.from(providers.values());
}