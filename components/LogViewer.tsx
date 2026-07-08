"use client";

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import EmptyState from "@/components/ui/EmptyState";

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
  ERROR: "text-red-500 dark:text-red-400",
  WARN: "text-yellow-600 dark:text-yellow-400",
  INFO: "text-neutral-600 dark:text-neutral-300",
  DEBUG: "text-neutral-400 dark:text-neutral-500",
  TRACE: "text-neutral-400 dark:text-neutral-500",
};

export default function LogViewer() {
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [exportStatus, setExportStatus] = useState<string | null>(null);

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
      setExportStatus(`Exported to ${destination}`);
      setTimeout(() => setExportStatus(null), 4000);
    }
  }

  return (
    <div className="flex h-full w-full flex-col bg-white dark:bg-[#1e1e1e]">
      <div className="flex h-8 shrink-0 items-center justify-between gap-2 border-b border-neutral-200 px-2 dark:border-neutral-800">
        <span className="text-[10px] text-neutral-400 dark:text-neutral-600">
          {exportStatus ?? `${entries.length} entries`}
        </span>
        <div className="flex gap-1">
          <button
            onClick={refresh}
            className="rounded px-2 py-0.5 text-xs text-neutral-500 transition-colors hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
          >
            Refresh
          </button>
          <button
            onClick={handleExport}
            className="rounded px-2 py-0.5 text-xs text-neutral-500 transition-colors hover:bg-neutral-100 hover:text-neutral-800 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
          >
            Export
          </button>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-1 font-mono text-xs">
        {entries.map((entry, i) => (
          <div key={i} className="whitespace-pre-wrap py-0.5">
            <span className="text-neutral-400 dark:text-neutral-600">{entry.timestamp}</span>{" "}
            <span className={LEVEL_COLOR[entry.level] ?? "text-neutral-600 dark:text-neutral-300"}>{entry.level}</span>{" "}
            <span className="text-neutral-500 dark:text-neutral-500">{entry.target}</span>{" "}
            <span className="text-neutral-700 dark:text-neutral-300">
              {entry.fields.event ?? entry.fields.message ?? ""}
            </span>
          </div>
        ))}
        {entries.length === 0 && <EmptyState icon="📜" title="No log entries yet" hint="Activity will appear here as you use the app" />}
      </div>
    </div>
  );
}
