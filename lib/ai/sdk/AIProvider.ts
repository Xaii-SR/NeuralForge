export interface ExecutionContext {
  model: string;
  endpoint: string;
  temperature: number;
  context: number;
  effort: "Light" | "Medium" | "High" | "Extra High";
  signal?: AbortSignal;
}

export abstract class StreamingProvider {
  abstract metadata: { id: string; displayName: string };
  abstract checkHealth(endpoint: string, signal?: AbortSignal): Promise<boolean>;
  abstract generateStream(prompt: string, context: ExecutionContext): AsyncGenerator<string, void, undefined>;

  async generate(prompt: string, context: ExecutionContext): Promise<string> {
    let fullText = "";
    for await (const chunk of this.generateStream(prompt, context)) {
      fullText += chunk;
    }
    return fullText;
  }
}