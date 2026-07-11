"use client";

import { useCallback, useEffect } from "react";

export interface DiffActionBarProps {
  isDiffMode: boolean;
  onAccept: () => void;
  onReject: () => void;
}

export default function DiffActionBar({ isDiffMode, onAccept, onReject }: DiffActionBarProps) {
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!isDiffMode) return;

    // Ctrl+Enter or Cmd+Enter: Accept
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      onAccept();
      return;
    }

    // Escape: Reject
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
    <div className="flex items-center justify-center gap-3 border-b border-[#333] bg-[#1e1e1e] px-4 py-2">
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