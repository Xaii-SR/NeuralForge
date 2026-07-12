"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { truncateTerminalOutput } from "@/lib/utils/tokenManagement";

let _blockIdCounter = 0;
function nextBlockId(): string {
  _blockIdCounter += 1;
  return `block-${_blockIdCounter}`;
}

export interface CodeBlock {
  id: string;
  file_path: string;
  language: string;
  code: string;
  status: "idle" | "applied" | "accepted" | "rejected" | "running" | "completed";
  blockType?: "file_edit" | "terminal_command";
  output?: string;
}

export interface ComposerMessage {
  role: string;
  content: string;
  file_paths: string[];
  code_blocks: CodeBlock[];
  contextSources?: { file_path: string; start_line: number; end_line: number; text: string; score: number }[];
}

export interface ComposerSession {
  session_id: string;
  active_files: string[];
  message_history: ComposerMessage[];
}

// Singleton terminal buffer accessor for cross-component sharing
let _terminalBufferFn: (() => string) | null = null;
export function setTerminalBufferGetter(fn: () => string) { _terminalBufferFn = fn; }
export function getTerminalBuffer(): string { return _terminalBufferFn?.() ?? ""; }

export interface PendingDiff {
  filePath: string;
  newCode: string;
}

export function useComposer() {
  const [session, setSession] = useState<ComposerSession | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [pendingDiffs, setPendingDiffs] = useState<PendingDiff[]>([]);
  const [activeDiffIndex, setActiveDiffIndex] = useState(0);

  const initialize = useCallback(async (files: string[]) => {
    const sessionId = `composer-${Date.now()}`;
    const result = await invoke<ComposerSession>("initialize_composer_session", { sessionId, initialFiles: files });
    setSession(result); setIsOpen(true); return result;
  }, []);

  const addFile = useCallback(async (filePath: string) => {
    if (!session) return;
    const r = await invoke<ComposerSession>("add_composer_file", { sessionId: session.session_id, filePath });
    setSession(r);
  }, [session]);

  const removeFile = useCallback(async (filePath: string) => {
    if (!session) return;
    const r = await invoke<ComposerSession>("remove_composer_file", { sessionId: session.session_id, filePath });
    setSession(r);
  }, [session]);

  const sendMessageRef = useRef<(content: string) => Promise<void> | undefined>(undefined);
  const sendMessage = useCallback(async (content: string) => {
    if (!session) return;
    // Detect @Codebase queries and fetch semantic context
    let contextSources: ComposerMessage["contextSources"] | null = null;
    let semanticContext: string | null = null;

    // @Codebase detection
    const codebaseMatch = content.match(/@Codebase\s+(.+)/i);
    if (codebaseMatch) {
      const query = codebaseMatch[1].trim();
      try {
        const results = await invoke<any[]>("query_codebase_semantic", { query, maxResults: 5, workspaceRoot: "" });
        contextSources = results.map((r: any) => ({ file_path: r.file_path, start_line: r.start_line, end_line: r.end_line, text: r.text, score: r.score }));
        semanticContext = results.map((r: any) => `File: ${r.file_path}\n${r.text}`).join("\n\n");
      } catch { /* ignore */ }
    }

    // @terminal detection: inject terminal buffer
    if (content.toLowerCase().includes("@terminal")) {
      const buf = getTerminalBuffer();
      if (buf) {
        const termBlock = `--- RECENT TERMINAL OUTPUT ---\n${buf}\n--- END TERMINAL ---`;
        semanticContext = semanticContext ? semanticContext + "\n\n" + termBlock : termBlock;
      }
    }

    const response = await invoke<{ text: ComposerMessage[]; autonomous_sources: any[] }>("send_composer_message", {
      sessionId: session.session_id,
      content,
      semanticContext,
    });
    const allSources = [...(contextSources || []), ...response.autonomous_sources];
    const h = response.text.map((msg, idx) => ({
      ...msg,
      contextSources: idx === 0 && allSources.length > 0 ? allSources : msg.contextSources,
      code_blocks: msg.code_blocks.map((b) => ({
        ...b, id: b.id || nextBlockId(), status: (b.status || "idle") as any,
        blockType: b.blockType || (b.file_path?.startsWith("exec") ? "terminal_command" as const : "file_edit" as const),
      })),
    }));
    setSession((prev) => prev ? { ...prev, message_history: h } : null);
  }, [session]);
  sendMessageRef.current = sendMessage;

  const updateBlockStatus = useCallback((blockId: string, status: any) => {
    if (!session) return;
    setSession({
      ...session,
      message_history: session.message_history.map((msg) => ({
        ...msg,
        code_blocks: msg.code_blocks.map((b) => b.id === blockId ? { ...b, status } : b),
      })),
    });
  }, [session]);

  const executeTerminalBlock = useCallback(async (blockId: string, command: string) => {
    if (!session) return;
    updateBlockStatus(blockId, "running");
    try {
      await invoke("execute_composer_command_stream", { blockId, command, workspaceRoot: "" });
    } catch { updateBlockStatus(blockId, "completed"); }
  }, [session, updateBlockStatus]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listen<{ block_id: string; line: string; done: boolean }>("terminal-stream", (event) => {
      if (disposed) return;
      const { block_id, line, done } = event.payload;
      setSession((prev) => {
        if (!prev) return prev;
        const updated = {
          ...prev,
          message_history: prev.message_history.map((msg) => ({
            ...msg,
            code_blocks: msg.code_blocks.map((b) => {
              if (b.id !== block_id) return b;
              const output = done ? b.output || "" : (b.output || "") + line + "\n";
              return { ...b, output, status: (done ? "completed" as const : "running" as const) };
            }),
          })),
        };
        // When command completes, send truncated output to AI for feedback
        if (done) {
          const block = updated.message_history.flatMap((m) => m.code_blocks).find((b) => b.id === block_id);
          if (block?.output) {
            const truncated = truncateTerminalOutput(block.output);
            sendMessage(truncated);
          }
        }
        return updated;
      });
    }).then((fn) => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const killCommand = useCallback(async (blockId: string) => {
    await invoke("kill_composer_command", { blockId });
    updateBlockStatus(blockId, "completed");
  }, [updateBlockStatus]);

  const close = useCallback(() => setIsOpen(false), []);

  return { session, isOpen, pendingDiffs, setPendingDiffs, activeDiffIndex, setActiveDiffIndex, initialize, addFile, removeFile, sendMessage, updateBlockStatus, executeTerminalBlock, killCommand, close, setIsOpen };
}