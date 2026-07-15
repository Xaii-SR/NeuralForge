"use client";

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InlineStatus } from "@/hooks/useInlinePrompt";
import { useMentionMenu } from "@/hooks/useMentionMenu";
import { useDebounce } from "@/hooks/useDebounce";
import MentionMenu from "@/components/composer/MentionMenu";
import type { MentionItem } from "@/components/composer/MentionMenu";

export interface InlinePromptWidgetProps {
  x: number; y: number;
  initialValue?: string; placeholder?: string;
  status: InlineStatus;
  error?: string | null;
  onSubmit: (value: string, context?: string) => Promise<string | null> | string | null;
  onAccept: () => void;
  onReject: () => void;
  onClose: () => void;
}

export default function InlinePromptWidget({
  x, y, initialValue = "",
  placeholder = "Ask AI to edit or generate...",
  status, error, onSubmit, onAccept, onReject, onClose,
}: InlinePromptWidgetProps) {
  const [value, setValue] = useState(initialValue);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const { state: mention, open: openMention, close: closeMention, setQuery: setMentionQuery, setActiveIndex: setMentionIndex } = useMentionMenu();
  const [suggestedItems, setSuggestedItems] = useState<MentionItem[]>([]);
  const [attachedItems, setAttachedItems] = useState<MentionItem[]>([]);
  const debouncedQuery = useDebounce(mention.query, 150);

  // Debounced workspace file + doc search for inline mentions
  useEffect(() => {
    if (!mention.isOpen || !debouncedQuery) { setSuggestedItems([]); return; }
    Promise.all([
      invoke<string[]>("search_workspace_files", { query: debouncedQuery, maxResults: 10, workspaceRoot: "" }),
      invoke<string[]>("list_cached_docs"),
    ]).then(([files, docs]) => {
      const fileItems: MentionItem[] = files.map((f) => ({ label: f, type: "file" }));
      const docItems: MentionItem[] = docs.map((d) => ({ label: d, type: "doc" }));
      setSuggestedItems([...fileItems, ...docItems]);
    }).catch(() => setSuggestedItems([]));
  }, [debouncedQuery, mention.isOpen]);

  useEffect(() => { const el = inputRef.current; if (el) { el.focus(); el.select(); } }, []);

  // Resolve context from attached items
  const resolveContext = async (): Promise<string | null> => {
    if (attachedItems.length === 0) return null;
    const parts: string[] = [];
    for (const item of attachedItems) {
      try {
        if (item.type === "file") {
          const content = await invoke<string>("read_file", { path: item.label });
          parts.push(`--- FILE: ${item.label} ---\n${content}\n--- END FILE ---`);
        } else if (item.type === "doc") {
          const content = await invoke<string>("read_cached_doc", { name: item.label });
          parts.push(`--- DOCUMENTATION: ${item.label} ---\n${content}\n--- END DOCUMENTATION ---`);
        }
      } catch { /* skip unresolvable items */ }
    }
    return parts.length > 0 ? parts.join("\n\n") : null;
  };

  const handleSubmit = async () => {
    if (!value.trim() || loading) return;
    setLoading(true);
    try {
      const context = await resolveContext();
      await onSubmit(value, context || undefined);
    } finally { setLoading(false); }
  };

  // @ mention detection
  const handleChange = (val: string) => {
    setValue(val);
    const atIndex = val.lastIndexOf("@");
    if (atIndex >= 0 && (atIndex === 0 || val[atIndex - 1] === " ")) {
      const query = val.slice(atIndex + 1);
      if (!query.includes(" ")) {
        const el = inputRef.current;
        if (el) {
          const rect = el.getBoundingClientRect();
          openMention({ x: rect.left, y: rect.top - 200 }, query);
        }
        setMentionQuery(query);
        return;
      }
    }
    closeMention(null);
  };

  const selectMention = (item: MentionItem) => {
    const atIndex = value.lastIndexOf("@");
    const newValue = value.slice(0, atIndex) + item.label + " ";
    setValue(newValue);
    if (!attachedItems.find((a) => a.label === item.label)) {
      setAttachedItems((prev) => [...prev, item]);
    }
    closeMention(item.label);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (mention.isOpen) {
      if (e.key === "ArrowDown") { e.preventDefault(); setMentionIndex(mention.activeIndex + 1); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); setMentionIndex(Math.max(0, mention.activeIndex - 1)); return; }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const sel = suggestedItems[mention.activeIndex];
        if (sel) selectMention(sel);
        return;
      }
      if (e.key === "Escape") { e.preventDefault(); closeMention(null); return; }
    }
    if (e.key === "Enter" && value.trim() && !loading && status === "idle" && !mention.isOpen) { e.preventDefault(); handleSubmit(); }
    else if (e.key === "Escape" && !loading && !mention.isOpen) { e.preventDefault(); onClose(); }
  };

  return (
    <div className="fixed z-[100]" style={{ left: x, top: y }}>
      {/* Attached context pills */}
      {attachedItems.length > 0 && (
        <div className="mb-1 flex flex-wrap gap-1">
          {attachedItems.map((item) => (
            <span key={item.label}
              className="inline-flex items-center gap-1 rounded border border-neutral-300 bg-white px-2 py-0.5 text-[10px] text-neutral-600 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-400">
              {item.type === "doc" ? "📚" : "@"} {item.label.split("/").pop()}
              <button onClick={() => setAttachedItems((p) => p.filter((a) => a.label !== item.label))} aria-label={`Remove ${item.label}`}
                className="ml-0.5 text-neutral-400 transition-colors hover:text-neutral-900 dark:text-neutral-500 dark:hover:text-white">✕</button>
            </span>
          ))}
        </div>
      )}
      <div className={`flex items-center gap-2 rounded-lg border px-3 py-2 shadow-2xl transition-colors ${status === "streaming" ? "border-blue-500 bg-blue-50 dark:bg-[#1a2332]" : status === "review" ? "border-green-500 bg-green-50 dark:bg-[#1a2e1a]" : "border-neutral-300 bg-white dark:border-neutral-600 dark:bg-neutral-900"}`}>
        <span className="text-xs font-semibold uppercase tracking-wide text-neutral-400 dark:text-neutral-500">AI</span>

        {status === "streaming" && (
          <div className="flex w-64 items-center gap-2">
            <div className="h-1.5 w-1.5 animate-pulse rounded-full bg-blue-500" />
            <span className="text-sm text-blue-600 dark:text-blue-400">Generating...</span>
          </div>
        )}

        {status === "review" && (
          <span className="text-sm text-green-600 dark:text-green-400">Review changes</span>
        )}

        {status === "idle" && (
          <input ref={inputRef} type="text" value={value} onChange={(e) => handleChange(e.target.value)} onKeyDown={handleKeyDown}
            placeholder={placeholder} className="w-64 bg-transparent text-sm text-neutral-900 outline-none placeholder:text-neutral-400 dark:text-white dark:placeholder:text-neutral-600" />
        )}

        {status === "idle" ? (
          <button onClick={handleSubmit} disabled={!value.trim() || loading} aria-label="Submit prompt"
            className="rounded px-2 py-0.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 hover:text-neutral-900 disabled:opacity-30 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-white">↵</button>
        ) : status === "review" ? (
          <div className="flex items-center gap-1">
            <button onClick={onAccept} className="rounded bg-green-700 px-2 py-0.5 text-xs font-medium text-white transition-colors hover:bg-green-600">✓ Accept</button>
            <button onClick={onReject} className="rounded bg-red-800 px-2 py-0.5 text-xs font-medium text-white transition-colors hover:bg-red-700">✕ Reject</button>
          </div>
        ) : null}
      </div>

      {error && (
        <div className="mt-1 max-w-80 rounded border border-red-800 bg-red-950/90 px-2 py-1 text-xs text-red-300 shadow-xl">
          {error}
        </div>
      )}

      {/* Mention menu */}
      {mention.isOpen && (
        <MentionMenu
          x={mention.coords.x}
          y={mention.coords.y}
          query={mention.query}
          items={suggestedItems}
          activeIndex={mention.activeIndex}
          onSelect={selectMention}
          onClose={() => closeMention(null)}
        />
      )}
    </div>
  );
}