"use client";

import { useCallback, useEffect } from "react";

export interface DiffActionBarProps {
  isDiffMode: boolean;
  onAccept: () => void;
  onReject: () => void;
  totalDiffs?: number;
  activeDiffIndex?: number;
  onPrevDiff?: () => void;
  onNextDiff?: () => void;
}

export default function DiffActionBar({
  isDiffMode, onAccept, onReject,
  totalDiffs, activeDiffIndex, onPrevDiff, onNextDiff,
}: DiffActionBarProps) {
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!isDiffMode) return;

    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      onAccept();
      return;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      onReject();
      return;
    }
  }, [isDiffMode, onAccept, onReject]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!isDiffMode) return null;

  return (
    <div className="flex items-center justify-center gap-3 border-b border-neutral-200 bg-neutral-50 px-4 py-2 dark:border-neutral-800 dark:bg-neutral-900">
      {totalDiffs != null && totalDiffs > 1 && onPrevDiff && onNextDiff && (
        <div className="mr-2 flex items-center gap-1">
          <button onClick={onPrevDiff} aria-label="Previous file" className="rounded bg-neutral-200 px-2 py-0.5 text-xs text-neutral-600 transition-colors hover:bg-neutral-300 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700">◀</button>
          <span className="text-xs text-neutral-500">File {(activeDiffIndex ?? 0) + 1} of {totalDiffs}</span>
          <button onClick={onNextDiff} aria-label="Next file" className="rounded bg-neutral-200 px-2 py-0.5 text-xs text-neutral-600 transition-colors hover:bg-neutral-300 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700">▶</button>
        </div>
      )}
      <button
        onClick={onAccept}
        className="flex items-center gap-1.5 rounded bg-green-700 px-4 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-green-600"
      >
        <span>✓</span>
        <span>Accept</span>
        <span className="ml-1 text-[10px] text-green-300">⌘⏎</span>
      </button>
      <button
        onClick={onReject}
        className="flex items-center gap-1.5 rounded bg-red-800 px-4 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-red-700"
      >
        <span>✕</span>
        <span>Reject</span>
        <span className="ml-1 text-[10px] text-red-300">Esc</span>
      </button>
    </div>
  );
}