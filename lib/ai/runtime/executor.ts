import { getAppConfig, savePromptHistory } from "../../store";
import { getRegisteredProvider } from "../provider-registry";
import { verifyRuntimeHealth } from "./health";
import { logTelemetry } from "../telemetry";
import { getModelCapabilities } from "../registry";
import { acquireGenerationLock, releaseGenerationLock } from "./generation-lock";
import { AIError } from "../errors";
import { AIConfig, GenerationResult } from "../types";

export async function executeGeneration(
  userIntent: string,
  uiSignal?: AbortSignal,
  overrides?: Partial<AIConfig>
): Promise<GenerationResult> {
  const startTime = Date.now();
  const lockId = acquireGenerationLock();

  try {
    const baseConfig = await getAppConfig();
    const config = { ...baseConfig, ...overrides };

    const provider = getRegisteredProvider(config.provider);
    if (!provider) throw new AIError("INVALID_CONFIG", `Unsupported AI provider: ${config.provider}`);

    const capabilities = getModelCapabilities(config.model);
    const eventId = crypto.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;

    if (config.context > capabilities.context) {
      throw new AIError("CONTEXT_EXCEEDED", `Model ${config.model} supports a maximum of ${capabilities.context} tokens.`);
    }

    const health = await verifyRuntimeHealth(config, uiSignal);
    if (!health.isHealthy) throw new AIError("AI_OFFLINE", `AI OFFLINE: ${health.reason}\nVerify ${config.provider} is running at ${config.endpoint}`);

    const timeoutMs =
      config.effort === "Light" ? 60000 :
      config.effort === "Medium" ? 120000 :
      config.effort === "High" ? 240000 : 300000;

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    const abortHandler = () => controller.abort();
    uiSignal?.addEventListener("abort", abortHandler, { once: true });

    const metaPromptSystemInstruction = "You are an Elite 100% Precision System Prompt Architect. Given a target objective, generate a complete professional system prompt with clear Personas, strict constraints, and formatting parameters. Output only the system prompt template.";

    try {
      const finalPrompt = await provider.generate(
        `${metaPromptSystemInstruction}\n\nTarget Objective to Engineer: ${userIntent}`,
        config,
        controller.signal
      );

      const durationMs = Date.now() - startTime;
      await savePromptHistory({ id: eventId, timestamp: startTime, input: userIntent, generated: finalPrompt, model: config.model, provider: config.provider });
      await logTelemetry({ id: eventId, provider: config.provider, model: config.model, startTime, endTime: Date.now(), durationMs, success: true });

      return { text: finalPrompt, provider: config.provider, model: config.model, durationMs, timestamp: startTime, success: true };
    } catch (err: any) {
      await logTelemetry({ id: eventId, provider: config.provider, model: config.model, startTime, endTime: Date.now(), durationMs: Date.now() - startTime, success: false, error: err.message });
      throw new AIError(err.name === "AbortError" ? "TIMEOUT" : "GENERATION_FAILED", err.name === "AbortError" ? `TIMEOUT OR CANCELLED: Engine failed to respond within ${timeoutMs/1000}s or request was aborted.` : err.message);
    } finally {
      uiSignal?.removeEventListener("abort", abortHandler);
      clearTimeout(timeoutId);
    }
  } finally {
    releaseGenerationLock(lockId);
  }
}