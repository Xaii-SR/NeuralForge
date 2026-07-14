export interface TelemetryEvent {
  id: string;
  provider: string;
  model: string;
  startTime: number;
  endTime: number;
  durationMs: number;
  success: boolean;
  error?: string;
}

export async function logTelemetry(event: TelemetryEvent): Promise<void> {
  const existing = JSON.parse(localStorage.getItem("nf_telemetry") || "[]");
  existing.unshift(event);
  localStorage.setItem("nf_telemetry", JSON.stringify(existing.slice(0, 500)));
}