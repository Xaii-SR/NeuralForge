import { EventBus } from "../../events/EventBus";
import { executeGeneration } from "./executor";
import { GenerationResult } from "../types";

class RuntimeManager {
  public async executeStream(intent: string, overrides?: any): Promise<GenerationResult> {
    EventBus.emit("Checking", {});
    EventBus.emit("Preparing", {});

    const result = await executeGeneration(intent, undefined, overrides);

    if (result.success) {
      const words = result.text.split(" ");
      let fullText = "";
      for (let i = 0; i < words.length; i++) {
        const chunk = i === 0 ? words[i] + " " : " " + words[i] + " ";
        fullText += chunk;
        EventBus.emit("Streaming", { chunk, fullText });
        await new Promise((r) => setTimeout(r, 0));
      }
      EventBus.emit("Complete", { fullText });
    }

    return result;
  }
}

export const runtimeManager = new RuntimeManager();