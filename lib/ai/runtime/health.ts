import { AIConfig } from "../types";
import { getProvider } from "../router";

export interface HealthDiagnostic {
  isHealthy: boolean;
  reason?: string;
}

export async function verifyRuntimeHealth(config: AIConfig, externalSignal?: AbortSignal): Promise<HealthDiagnostic> {
  const provider = getProvider(config.provider);
  if (!provider) {
    return { isHealthy: false, reason: `Unsupported AI provider: ${config.provider}` };
  }

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 5000);

  const abortHandler = () => controller.abort();
  externalSignal?.addEventListener("abort", abortHandler);

  try {
    const isOnline = await provider.checkHealth(config.endpoint, controller.signal);
    if (!isOnline) return { isHealthy: false, reason: "Provider endpoint rejected connection." };
    return { isHealthy: true };
  } catch (err: any) {
    return { isHealthy: false, reason: err.name === "AbortError" ? "Health check timed out or was cancelled." : err.message };
  } finally {
    externalSignal?.removeEventListener("abort", abortHandler);
    clearTimeout(timeoutId);
  }
}