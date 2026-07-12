"use client";

import { useEffect, useRef, useState } from "react";
import type { InlineStatus } from "@/hooks/useInlinePrompt";

export interface InlinePromptWidgetProps {
  x: number; y: number;
  initialValue?: string; placeholder?: string;
  status: InlineStatus;
  onSubmit: (value: string) => Promise<string | null> | string | null;
  onAccept: () => void;
  onReject: () => void;
  onClose: () => void;
}

export default function InlinePromptWidget({
  x, y, initialValue = "",
  placeholder = "Ask AI to edit or generate...",
  status, onSubmit, onAccept, onReject, onClose,
}: InlinePromptWidgetProps) {
  const [value, setValue] = useState(initialValue);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { const el = inputRef.current; if (el) { el.focus(); el.select(); } }, []);

  const handleSubmit = async () => {
    if (!value.trim() || loading) return;
    setLoading(true);
    try { await onSubmit(value); } finally { setLoading(false); }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && value.trim() && !loading && status === "idle") { e.preventDefault(); handleSubmit(); }
    else if (e.key === "Escape" && !loading && status !== "streaming") { e.preventDefault(); onClose(); }
  };

  return (
    <div className="fixed z-[100]" style={{ left: x, top: y }}>
      <div className={`flex items-center gap-2 rounded-lg border px-3 py-2 shadow-2xl transition-colors ${status === "streaming" ? "border-[#3b82f6] bg-[#1a2332]" : status === "review" ? "border-[#22c55e] bg-[#1a2e1a]" : "border-[#555] bg-[#1e1e1e]"}`}>
        <span className="text-xs font-semibold uppercase tracking-wide text-[#888]">AI</span>

        {status === "streaming" && (
          <div className="flex w-64 items-center gap-2">
            <div className="h-1.5 w-1.5 animate-pulse rounded-full bg-blue-500" />
            <span className="text-sm text-blue-400">Generating...</span>
          </div>
        )}

        {status === "review" && (
          <span className="text-sm text-green-400">Review changes</span>
        )}

        {status === "idle" && (
          <input ref={inputRef} type="text" value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={handleKeyDown}
            placeholder={placeholder} className="w-64 bg-transparent text-sm text-white outline-none placeholder:text-[#555]" />
        )}

        {status === "idle" ? (
          <button onClick={handleSubmit} disabled={!value.trim() || loading}
            className="rounded px-2 py-0.5 text-xs font-medium text-[#aaa] transition-colors hover:bg-[#333] hover:text-white disabled:opacity-30">↵</button>
        ) : status === "review" ? (
          <div className="flex items-center gap-1">
            <button onClick={onAccept} className="rounded bg-green-700 px-2 py-0.5 text-xs font-medium text-white transition-colors hover:bg-green-600">✓ Accept</button>
            <button onClick={onReject} className="rounded bg-red-800 px-2 py-0.5 text-xs font-medium text-white transition-colors hover:bg-red-700">✕ Reject</button>
          </div>
        ) : null}
      </div>
    </div>
  );
}