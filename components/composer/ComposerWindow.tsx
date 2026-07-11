"use client";

import { useEffect, useRef, useState } from "react";
import type { ComposerSession } from "@/hooks/useComposer";

export interface ComposerWindowProps {
  session: ComposerSession;
  onSendMessage: (content: string) => Promise<void>;
  onAddFile: (filePath: string) => Promise<void>;
  onRemoveFile: (filePath: string) => Promise<void>;
  onClose: () => void;
}

export default function ComposerWindow({
  session,
  onSendMessage,
  onClose,
}: ComposerWindowProps) {
  const [inputValue, setInputValue] = useState("");
  const [isDragging, setIsDragging] = useState(false);
  const [position, setPosition] = useState({ x: 200, y: 100 });
  const dragOffset = useRef({ x: 0, y: 0 });
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-focus the input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    dragOffset.current = { x: e.clientX - position.x, y: e.clientY - position.y };
  };

  useEffect(() => {
    if (!isDragging) return;
    const handleMouseMove = (e: MouseEvent) => {
      setPosition({ x: e.clientX - dragOffset.current.x, y: e.clientY - dragOffset.current.y });
    };
    const handleMouseUp = () => setIsDragging(false);
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [isDragging]);

  const handleSubmit = async () => {
    if (!inputValue.trim()) return;
    await onSendMessage(inputValue);
    setInputValue("");
  };

  return (
    <div
      className="fixed z-50 flex flex-col rounded-lg border border-[#444] bg-[#1e1e1e] shadow-2xl"
      style={{ left: position.x, top: position.y, width: 520 }}
    >
      {/* Title bar / drag handle */}
      <div
        className="flex cursor-grab items-center justify-between rounded-t-lg border-b border-[#333] bg-[#252526] px-3 py-2"
        onMouseDown={handleMouseDown}
      >
        <span className="text-xs font-semibold uppercase tracking-wide text-[#888]">Composer</span>
        <div className="flex items-center gap-2">
          {session.active_files.length > 0 && (
            <div className="flex -space-x-1">
              {session.active_files.slice(0, 3).map((fp) => (
                <span
                  key={fp}
                  className="inline-flex h-5 items-center rounded-full bg-[#333] px-2 text-[10px] text-[#aaa]"
                  title={fp}
                >
                  {fp.split("/").pop()}
                </span>
              ))}
              {session.active_files.length > 3 && (
                <span className="inline-flex h-5 items-center rounded-full bg-[#333] px-2 text-[10px] text-[#aaa]">
                  +{session.active_files.length - 3}
                </span>
              )}
            </div>
          )}
          <button onClick={onClose} className="text-[10px] text-[#666] hover:text-white">
            ✕
          </button>
        </div>
      </div>

      {/* Message history */}
      <div className="max-h-80 min-h-[120px] overflow-y-auto px-3 py-2">
        {session.message_history.length === 0 && (
          <p className="py-8 text-center text-xs text-[#555]">Ask AI to make changes across your files...</p>
        )}
        {session.message_history.map((msg, i) => (
          <div key={i} className={`mb-2 rounded px-3 py-2 text-sm ${msg.role === "user" ? "bg-[#2a2d2e]" : "bg-[#1a2332]"}`}>
            <span className={`mb-1 block text-[10px] font-semibold uppercase ${msg.role === "user" ? "text-blue-400" : "text-green-400"}`}>
              {msg.role}
            </span>
            <p className="text-[13px] text-[#d4d4d4]">{msg.content}</p>
            {msg.file_paths.length > 0 && (
              <div className="mt-1 flex flex-wrap gap-1">
                {msg.file_paths.map((fp) => (
                  <span key={fp} className="rounded bg-[#333] px-1.5 py-0.5 text-[10px] text-[#888]">
                    {fp}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Input area */}
      <div className="flex items-center gap-2 border-t border-[#333] px-3 py-2">
        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); handleSubmit(); } }}
          placeholder="Describe the changes you want..."
          className="flex-1 bg-transparent text-sm text-white outline-none placeholder:text-[#555]"
        />
        <button
          onClick={handleSubmit}
          disabled={!inputValue.trim()}
          className="rounded bg-blue-700 px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-blue-600 disabled:opacity-30"
        >
          Send
        </button>
      </div>
    </div>
  );
}