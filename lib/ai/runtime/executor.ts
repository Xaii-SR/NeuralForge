import { getAppConfig, savePromptHistory } from "../../store";
import { getProvider } from "../router";
import { verifyRuntimeHealth } from "./health";
import { logTelemetry } from "../telemetry";
import { getModelCapabilities } from "../registry";
import { GenerationResult } from "../types";

export async function executeGeneration(userIntent: string, uiSignal?: AbortSignal): Promise<GenerationResult> {
  const startTime = Date.now();
  const config = await getAppConfig();
  const provider = getProvider(config.provider);

  if (!provider) throw new Error(`Unsupported AI provider: ${config.provider}`);

  const capabilities = getModelCapabilities(config.model);
  const eventId = crypto.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;

  if (config.context > capabilities.context) {
    throw new Error(`Model ${config.model} supports a maximum of ${capabilities.context} tokens.`);
  }

  const health = await verifyRuntimeHealth(config, uiSignal);
  if (!health.isHealthy) throw new Error(`AI OFFLINE: ${health.reason}\nVerify ${config.provider} is running at ${config.endpoint}`);

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 120000);

  const abortHandler = () => controller.abort();
  uiSignal?.addEventListener("abort", abortHandler);

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
    throw new Error(err.name === "AbortError" ? `TIMEOUT OR CANCELLED: Engine failed to respond within 120s or request was aborted.` : err.message);
  } finally {
    uiSignal?.removeEventListener("abort", abortHandler);
    clearTimeout(timeoutId);
  }
}