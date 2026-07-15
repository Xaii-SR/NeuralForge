"use client";

export interface ContextPillProps {
  filePath: string;
  onRemove: (path: string) => void;
}

export default function ContextPill({ filePath, onRemove }: ContextPillProps) {
  const basename = filePath.split("/").pop() || filePath;

  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-neutral-300 bg-neutral-100 py-0.5 pl-2 pr-1 text-[11px] text-neutral-700 dark:border-neutral-700 dark:bg-[#2a2d2e] dark:text-neutral-300">
      <span className="text-[10px] text-blue-600 dark:text-blue-400">@</span>
      <span className="max-w-[120px] truncate">{basename}</span>
      <button
        onClick={() => onRemove(filePath)}
        aria-label={`Remove ${basename}`}
        className="ml-0.5 inline-flex h-3.5 w-3.5 items-center justify-center rounded-full text-[9px] text-neutral-400 transition-colors hover:bg-neutral-300 hover:text-neutral-900 dark:text-neutral-500 dark:hover:bg-neutral-600 dark:hover:text-white"
      >
        ✕
      </button>
    </span>
  );
}