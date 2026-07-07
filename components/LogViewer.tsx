"use client";

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  fields: { message?: string; event?: string; [key: string]: unknown };
}

function parseLine(line: string): LogEntry | null {
  try {
    const raw = JSON.parse(line);
    return {
      timestamp: raw.timestamp ?? "",
      level: raw.level ?? "INFO",
      target: raw.target ?? "",
      fields: raw.fields ?? {},
    };
  } catch {
    return null;
  }
}

const LEVEL_COLOR: Record<string, string> = {
  ERROR: "text-red-400",
  WARN: "text-yellow-400",
  INFO: "text-neutral-300",
  DEBUG: "text-neutral-500",
  TRACE: "text-neutral-500",
};

export default function LogViewer() {
  const [entries, setEntries] = useState<LogEntry[]>([]);

  const refresh = useCallback(async () => {
    const lines = await invoke<string[]>("get_recent_logs", { lines: 200 });
    setEntries(lines.map(parseLine).filter((e): e is LogEntry => e !== null));
  }, []);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  }, [refresh]);

  async function handleExport() {
    const destination = await save({ defaultPath: "neuralforge-logs.txt" });
    if (destination) {
      await invoke("export_logs", { destination });
    }
  }

  return (
    <div className="flex h-full w-full flex-col bg-[#1e1e1e]">
      <div className="flex h-7 shrink-0 items-center justify-end gap-2 border-b border-neutral-800 px-2">
        <button
          onClick={refresh}
          className="rounded px-2 py-0.5 text-xs text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
        >
          Refresh
        </button>
        <button
          onClick={handleExport}
          className="rounded px-2 py-0.5 text-xs text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
        >
          Export
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-1 font-mono text-xs">
        {entries.map((entry, i) => (
          <div key={i} className="whitespace-pre-wrap py-0.5">
            <span className="text-neutral-600">{entry.timestamp}</span>{" "}
            <span className={LEVEL_COLOR[entry.level] ?? "text-neutral-300"}>{entry.level}</span>{" "}
            <span className="text-neutral-500">{entry.target}</span>{" "}
            <span className="text-neutral-300">
              {entry.fields.event ?? entry.fields.message ?? ""}
            </span>
          </div>
        ))}
        {entries.length === 0 && <div className="text-neutral-600">No log entries yet</div>}
      </div>
    </div>
  );
}
