export type RuntimeEventType = "Idle" | "Checking" | "Preparing" | "Streaming" | "Complete" | "Cancelled" | "Failed";

export interface RuntimeEvents {
  Checking: {};
  Preparing: {};
  Streaming: { chunk: string; fullText: string };
  Complete: { fullText: string };
  Failed: { message: string };
  Cancelled: {};
}

type EventListener = {
  <K extends keyof RuntimeEvents>(type: K, payload: RuntimeEvents[K]): void;
};

class RuntimeEventBus {
  private listeners: EventListener[] = [];

  subscribe(listener: EventListener): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  }

  emit<K extends keyof RuntimeEvents>(type: K, payload: RuntimeEvents[K]): void {
    this.listeners.forEach((l) => l(type, payload));
  }
}

export const EventBus = new RuntimeEventBus();