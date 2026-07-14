import { AIConfig, AIProvider } from "./types";

const providers: Record<string, AIProvider> = {};

export function registerProvider(id: string, provider: AIProvider): void {
  providers[id] = provider;
}

export function getProvider(id: string): AIProvider | undefined {
  return providers[id];
}