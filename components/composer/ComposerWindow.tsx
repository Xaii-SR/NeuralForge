"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { ComposerSession } from "@/hooks/useComposer";
import { useMentionMenu } from "@/hooks/useMentionMenu";
import { useDebounce } from "@/hooks/useDebounce";
import MentionMenu from "@/components/composer/MentionMenu";
import ContextPill from "@/components/composer/ContextPill";
import ContextAccordion from "@/components/composer/ContextAccordion";
import { invoke } from "@tauri-apps/api/core";

export interface ComposerWindowProps {
  session: ComposerSession;
  onSendMessage: (content: string) => Promise<void>;
  onAddFile: (filePath: string) => Promise<void>;
  onRemoveFile: (filePath: string) => Promise<void>;
  onApplyBlock: (blockId: string, filePath: string, code: string) => void;
  onClose: () => void;
}


export default function ComposerWindow({
  session,
  onSendMessage,
  onAddFile,
  onApplyBlock,
  onClose,
}: ComposerWindowProps) {
  const [inputValue, setInputValue] = useState("");
  const [isDragging, setIsDragging] = useState(false);
  const [position, setPosition] = useState({ x: 200, y: 100 });
  const dragOffset = useRef({ x: 0, y: 0 });
  const inputRef = useRef<HTMLInputElement>(null);
  const inputContainerRef = useRef<HTMLDivElement>(null);
  const { state: mention, open: openMention, close: closeMention, setQuery: setMentionQuery, setActiveIndex: setMentionIndex } = useMentionMenu();
  const [suggestedFiles, setSuggestedFiles] = useState<string[]>([]);
  const debouncedQuery = useDebounce(mention.query, 150);

  // Debounced workspace file search
  useEffect(() => {
    if (!mention.isOpen || !debouncedQuery) { setSuggestedFiles([]); return; }
    invoke<string[]>("search_workspace_files", { query: debouncedQuery, maxResults: 10, workspaceRoot: "" })
      .then(setSuggestedFiles)
      .catch(() => setSuggestedFiles([]));
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
        const filtered = suggestedFiles.length > 0
          ? suggestedFiles
          : (mention.query ? [] : []);
        const selected = filtered[mention.activeIndex];
        if (selected) {
          // Remove @query from input, add file
          const atIndex = inputValue.lastIndexOf("@");
          const newValue = inputValue.slice(0, atIndex) + selected + " ";
          setInputValue(newValue);
          onAddFile(selected);
          closeMention(selected);
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
    <div className="fixed z-50 flex flex-col rounded-lg border border-[#444] bg-[#1e1e1e] shadow-2xl" style={{ left: position.x, top: position.y, width: 520 }}>
      <div className="flex cursor-grab items-center justify-between rounded-t-lg border-b border-[#333] bg-[#252526] px-3 py-2" onMouseDown={handleMouseDown}>
        <span className="text-xs font-semibold uppercase tracking-wide text-[#888]">Composer</span>
        <div className="flex items-center gap-2">
          {session.active_files.length > 0 && (
            <div className="flex -space-x-1">
              {session.active_files.slice(0, 3).map((fp) => (
                <span key={fp} className="inline-flex h-5 items-center rounded-full bg-[#333] px-2 text-[10px] text-[#aaa]" title={fp}>{fp.split("/").pop()}</span>
              ))}
              {session.active_files.length > 3 && <span className="inline-flex h-5 items-center rounded-full bg-[#333] px-2 text-[10px] text-[#aaa]">+{session.active_files.length - 3}</span>}
            </div>
          )}
          <button onClick={onClose} className="text-[10px] text-[#666] hover:text-white">✕</button>
        </div>
      </div>

      <div className="max-h-80 min-h-[120px] overflow-y-auto px-3 py-2">
        {session.message_history.length === 0 && <p className="py-8 text-center text-xs text-[#555]">Ask AI to make changes across your files...</p>}
        {session.message_history.map((msg, i) => (
          <div key={i} className={`mb-2 rounded px-3 py-2 text-sm ${msg.role === "user" ? "bg-[#2a2d2e]" : "bg-[#1a2332]"}`}>
            <span className={`mb-1 block text-[10px] font-semibold uppercase ${msg.role === "user" ? "text-blue-400" : "text-green-400"}`}>{msg.role}</span>
            <p className="whitespace-pre-wrap text-[13px] text-[#d4d4d4]">{msg.content}</p>
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
                            {block.status === "idle" && <button onClick={() => onApplyBlock(block.id, block.file_path, block.code)} className="rounded bg-blue-700 px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-blue-600">Apply</button>}
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
                {msg.file_paths.map((fp) => <span key={fp} className="rounded bg-[#333] px-1.5 py-0.5 text-[10px] text-[#888]">{fp}</span>)}
              </div>
            )}
          </div>
        ))}
      </div>

      <div ref={inputContainerRef} className="relative flex items-center gap-2 border-t border-[#333] px-3 py-2">
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={handleInputKeyDown}
          placeholder="Describe changes... (use @ to add files)"
          className="flex-1 bg-transparent text-sm text-white outline-none placeholder:text-[#555]"
        />
        <button onClick={handleSubmit} disabled={!inputValue.trim() || mention.isOpen} className="rounded bg-blue-700 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-600 disabled:opacity-30">Send</button>
      </div>

      {/* Mention menu */}
          {mention.isOpen && (
        <MentionMenu
          x={mention.coords.x}
          y={mention.coords.y}
          query={mention.query}
          items={suggestedFiles}
          activeIndex={mention.activeIndex}
          onSelect={(file) => {
            const atIndex = inputValue.lastIndexOf("@");
            const newValue = inputValue.slice(0, atIndex) + file + " ";
            setInputValue(newValue);
            onAddFile(file);
            closeMention(file);
          }}
          onClose={() => closeMention(null)}
        />
      )}
    </div>
  );
}