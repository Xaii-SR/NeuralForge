"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { ComposerSession } from "@/hooks/useComposer";
import { useMentionMenu } from "@/hooks/useMentionMenu";
import { useDebounce } from "@/hooks/useDebounce";
import MentionMenu from "@/components/composer/MentionMenu";
import ContextPill from "@/components/composer/ContextPill";
import ContextAccordion from "@/components/composer/ContextAccordion";
import type { MentionItem } from "@/components/composer/MentionMenu";
import { useComposer } from "@/hooks/useComposer";
import { invoke } from "@tauri-apps/api/core";

import type { PendingDiff } from "@/hooks/useComposer";

export interface ComposerWindowProps {
  session: ComposerSession;
  onSendMessage: (content: string) => Promise<void>;
  onAddFile: (filePath: string) => Promise<void>;
  onRemoveFile: (filePath: string) => Promise<void>;
  onApplyBlock: (blockId: string, filePath: string, code: string) => void;
  onClose: () => void;
  pendingDiffs?: PendingDiff[];
  setPendingDiffs?: (diffs: PendingDiff[]) => void;
  setAttachedDocs?: React.Dispatch<React.SetStateAction<string[]>>;
}


export default function ComposerWindow({
  session,
  onSendMessage,
  onAddFile,
  onApplyBlock,
  onClose,
  pendingDiffs,
  setPendingDiffs,
  setAttachedDocs,
}: ComposerWindowProps) {
  const [inputValue, setInputValue] = useState("");
  const [applyingBlockId, setApplyingBlockId] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [position, setPosition] = useState({ x: 200, y: 100 });
  const dragOffset = useRef({ x: 0, y: 0 });
  const inputRef = useRef<HTMLInputElement>(null);
  const inputContainerRef = useRef<HTMLDivElement>(null);
  const { state: mention, open: openMention, close: closeMention, setQuery: setMentionQuery, setActiveIndex: setMentionIndex } = useMentionMenu();
  const { hasCustomRules } = useComposer();
  const [suggestedItems, setSuggestedItems] = useState<MentionItem[]>([]);
  const debouncedQuery = useDebounce(mention.query, 150);

  // Debounced workspace file + doc search
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

  useEffect(() => { inputRef.current?.focus(); }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    dragOffset.current = { x: e.clientX - position.x, y: e.clientY - position.y };
  };

  useEffect(() => {
    if (!isDragging) return;
    const handleMouseMove = (e: MouseEvent) => setPosition({ x: e.clientX - dragOffset.current.x, y: e.clientY - dragOffset.current.y });
    const handleMouseUp = () => setIsDragging(false);
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => { window.removeEventListener("mousemove", handleMouseMove); window.removeEventListener("mouseup", handleMouseUp); };
  }, [isDragging]);

  // @ mention detection and keyboard navigation
  const handleInputChange = useCallback((value: string) => {
    setInputValue(value);
    const atIndex = value.lastIndexOf("@");
    if (atIndex >= 0 && (atIndex === 0 || value[atIndex - 1] === " ")) {
      const query = value.slice(atIndex + 1);
      if (!query.includes(" ")) {
        // Calculate input position for the mention menu
        const inputEl = inputRef.current;
        if (inputEl) {
          const rect = inputEl.getBoundingClientRect();
          openMention({ x: rect.left, y: rect.top - 200 }, query);
        }
        setMentionQuery(query);
        return;
      }
    }
    closeMention(null);
  }, [openMention, closeMention, setMentionQuery]);

  const handleInputKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (mention.isOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setMentionIndex(mention.activeIndex + 1);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setMentionIndex(Math.max(0, mention.activeIndex - 1));
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const filtered = suggestedItems.length > 0
          ? suggestedItems
          : [];
        const selected = filtered[mention.activeIndex];
        if (selected) {
          const atIndex = inputValue.lastIndexOf("@");
          const newValue = inputValue.slice(0, atIndex) + selected.label + " ";
          setInputValue(newValue);
          if (selected.type === "file") onAddFile(selected.label);
          closeMention(selected.label);
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        closeMention(null);
        return;
      }
    }

    if (e.key === "Enter" && !e.shiftKey && inputValue.trim() && !mention.isOpen) {
      e.preventDefault();
      handleSubmit();
    }
  }, [mention, inputValue, onAddFile, setMentionIndex, closeMention]);

  const handleSubmit = async () => {
    if (!inputValue.trim()) return;
    await onSendMessage(inputValue);
    setInputValue("");
  };

  return (
    <div className="fixed z-50 flex flex-col rounded-lg border border-neutral-200 bg-white shadow-2xl dark:border-neutral-700 dark:bg-neutral-900" style={{ left: position.x, top: position.y, width: 520 }}>
      <div className="flex cursor-grab items-center justify-between rounded-t-lg border-b border-neutral-200 bg-neutral-50 px-3 py-2 dark:border-neutral-800 dark:bg-neutral-800/60" onMouseDown={handleMouseDown}>
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold uppercase tracking-wide text-neutral-400 dark:text-neutral-500">Composer</span>
          {hasCustomRules && (
            <span className="inline-flex items-center gap-1 rounded border border-zinc-800 bg-zinc-900/50 px-2 py-0.5 text-[10px] text-zinc-400 transition-opacity select-none"
              title="Project-specific instructions are active for this prompt session.">
              📜 .neuralforgerules
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {session.active_files.length > 0 && (
            <div className="flex -space-x-1">
              {session.active_files.slice(0, 3).map((fp) => (
                <span key={fp} className="inline-flex h-5 items-center rounded-full bg-neutral-200 px-2 text-[10px] text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300" title={fp}>{fp.split("/").pop()}</span>
              ))}
              {session.active_files.length > 3 && <span className="inline-flex h-5 items-center rounded-full bg-neutral-200 px-2 text-[10px] text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300">+{session.active_files.length - 3}</span>}
            </div>
          )}
          <button onClick={onClose} aria-label="Close composer" className="text-[10px] text-neutral-400 transition-colors hover:text-neutral-900 dark:text-neutral-500 dark:hover:text-white">✕</button>
        </div>
      </div>

      <div className="max-h-80 min-h-[120px] overflow-y-auto px-3 py-2">
        {session.message_history.length === 0 && <p className="py-8 text-center text-xs text-neutral-400 dark:text-neutral-500">Ask AI to make changes across your files...</p>}
        {session.message_history.map((msg, i) => (
          <div key={i} className={`mb-2 rounded px-3 py-2 text-sm ${msg.role === "user" ? "bg-neutral-100 dark:bg-[#2a2d2e]" : "bg-blue-50 dark:bg-[#1a2332]"}`}>
            <span className={`mb-1 block text-[10px] font-semibold uppercase ${msg.role === "user" ? "text-blue-600 dark:text-blue-400" : "text-green-600 dark:text-green-400"}`}>{msg.role}</span>
            <p className="whitespace-pre-wrap text-[13px] text-neutral-800 dark:text-[#d4d4d4]">{msg.content}</p>
            {(msg as any).code_blocks?.length > 0 && (
              <div className="mt-2 space-y-2">
                {(msg as any).code_blocks.map((block: any, bi: number) => {
                  const isTerminal = block.blockType === "terminal_command";
                  const isExecutable = ["bash", "sh", "shell"].includes((block.language || "").toLowerCase());
                  const handleRun = async () => {
                    try { await invoke("write_to_pty", { sessionId: "default", data: block.code + "\r" }); } catch {}
                  };
                  return (
                    <div key={bi} className={`overflow-hidden rounded border ${isTerminal ? "border-[#3b3b3b] bg-[#0d1117]" : "border-[#444] bg-[#0d1117]"}`}>
                      <div className={`flex items-center justify-between border-b px-3 py-1.5 ${isTerminal ? "border-[#3b3b3b] bg-[#161b22]" : "border-[#333] bg-[#161b22]"}`}>
                        <span className="flex items-center gap-1.5 text-[11px] font-medium text-blue-400">
                          {isTerminal && <span className="text-[#888]">{">"}</span>}
                          {block.file_path || "unknown"}
                        </span>
                        <div className="flex items-center gap-2">
                          {isExecutable && (
                            <button onClick={handleRun} className="rounded bg-emerald-500/10 border border-emerald-500/20 px-2 py-0.5 text-[10px] font-medium text-emerald-400 transition-colors hover:bg-emerald-500/20">▶ Run</button>
                          )}
                          <span className="text-[10px] uppercase text-[#666]">{block.language || "code"}</span>
                        </div>
                      </div>
                      <pre className={`overflow-x-auto p-3 text-[12px] leading-relaxed ${isTerminal ? "text-[#e6e6e6]" : "text-[#d4d4d4]"}`}>{block.code}</pre>
                      {block.output && (
                        <div className="border-t border-[#333] bg-[#0a0e14] p-3 font-mono text-[11px] leading-relaxed text-[#e6e6e6]">
                          {block.output.split("\n").map((line: string, li: number) => (
                            <div key={li} className="whitespace-pre-wrap">{line}</div>
                          ))}
                        </div>
                      )}
                      <div className="flex items-center gap-2 border-t border-[#333] bg-[#161b22] px-3 py-2">
                        {isTerminal ? (
                          block.status === "running" ? (
                            <span className="flex items-center gap-1 text-[11px] text-yellow-400">
                              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-yellow-400" />
                              Running...
                            </span>
                          ) : (
                            <button onClick={() => onApplyBlock(block.id, block.file_path, block.code)} className="rounded bg-[#3b3b3b] px-3 py-1 text-[11px] font-medium text-[#d4d4d4] transition-colors hover:bg-[#555]">$ Run Command</button>
                          )
                        ) : (
                          <>
                            {block.status === "idle" && block.file_path && block.file_path !== "unknown" && !block.file_path?.startsWith("exec") && (
                              <button
                                onClick={() => {
                                  if (setPendingDiffs) setPendingDiffs([{ filePath: block.file_path, newCode: block.code }]);
                                  setApplyingBlockId(block.id);
                                  setTimeout(() => setApplyingBlockId(null), 1000);
                                }}
                                className="rounded bg-blue-700 px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-blue-600"
                              >
                                {applyingBlockId === block.id ? "Applying..." : "✓ Apply"}
                              </button>
                            )}
                            {block.status === "applied" && <span className="flex items-center gap-1 text-[11px] text-yellow-400"><span className="h-1.5 w-1.5 animate-pulse rounded-full bg-yellow-400" />Reviewing in Diff...</span>}
                            {["accepted", "rejected", "completed"].includes(block.status) && <span className="text-[11px] text-[#666]">{block.status === "accepted" ? "Accepted" : block.status === "rejected" ? "Rejected" : block.status === "completed" ? "Completed" : ""}</span>}
                          </>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
            {(msg as any).contextSources?.length > 0 && (
              <ContextAccordion sources={(msg as any).contextSources} />
            )}
            {msg.file_paths.length > 0 && (
              <div className="mt-1 flex flex-wrap gap-1">
                {msg.file_paths.map((fp) => <span key={fp} className="rounded bg-neutral-200 px-1.5 py-0.5 text-[10px] text-neutral-500 dark:bg-neutral-700 dark:text-neutral-400">{fp}</span>)}
              </div>
            )}
          </div>
        ))}
      </div>

      <div ref={inputContainerRef} className="relative flex items-center gap-2 border-t border-neutral-200 px-3 py-2 dark:border-neutral-800">
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={handleInputKeyDown}
          placeholder="Describe changes... (use @ to add files)"
          className="flex-1 bg-transparent text-sm text-neutral-900 outline-none placeholder:text-neutral-400 dark:text-white dark:placeholder:text-neutral-600"
        />
        <button onClick={handleSubmit} disabled={!inputValue.trim() || mention.isOpen} className="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-30">Send</button>
      </div>

      {/* Mention menu */}
          {mention.isOpen && (
        <MentionMenu
          x={mention.coords.x}
          y={mention.coords.y}
          query={mention.query}
          items={suggestedItems}
          activeIndex={mention.activeIndex}
          onSelect={(item) => {
            const atIndex = inputValue.lastIndexOf("@");
            const newValue = inputValue.slice(0, atIndex) + item.label + " ";
            setInputValue(newValue);
            if (item.type === "file") onAddFile(item.label);
            else if (item.type === "doc" && setAttachedDocs) setAttachedDocs((prev: string[]) => [...prev, item.label]);
            closeMention(item.label);
          }}
          onClose={() => closeMention(null)}
        />
      )}
    </div>
  );
}