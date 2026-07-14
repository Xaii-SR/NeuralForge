import { AIError } from "../errors";

let activeSession: string | null = null;

export function acquireGenerationLock(): string {
  if (activeSession) throw new AIError("GENERATION_ACTIVE", "Another generation is currently running.");
  activeSession = crypto.randomUUID?.() || `${Date.now()}`;
  return activeSession;
}

export function releaseGenerationLock(session: string): void {
  if (activeSession === session) activeSession = null;
}