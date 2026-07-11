"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

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
}

export interface ComposerSession {
  session_id: string;
  active_files: string[];
  message_history: ComposerMessage[];
}

export function useComposer() {
  const [session, setSession] = useState<ComposerSession | null>(null);
  const [isOpen, setIsOpen] = useState(false);

  const initialize = useCallback(async (files: string[]) => {
    const sessionId = `composer-${Date.now()}`;
    const result = await invoke<ComposerSession>("initialize_composer_session", {
      sessionId,
      initialFiles: files,
    });
    setSession(result);
    setIsOpen(true);
    return result;
  }, []);

  const addFile = useCallback(async (filePath: string) => {
    if (!session) return;
    const result = await invoke<ComposerSession>("add_composer_file", {
      sessionId: session.session_id,
      filePath,
    });
    setSession(result);
  }, [session]);

  const removeFile = useCallback(async (filePath: string) => {
    if (!session) return;
    const result = await invoke<ComposerSession>("remove_composer_file", {
      sessionId: session.session_id,
      filePath,
    });
    setSession(result);
  }, [session]);

  const sendMessage = useCallback(async (content: string) => {
    if (!session) return;
    const history = await invoke<ComposerMessage[]>("send_composer_message", {
      sessionId: session.session_id,
      content,
    });
    const historyWithIds = history.map((msg) => ({
      ...msg,
      code_blocks: msg.code_blocks.map((block) => ({
        ...block,
        id: block.id || nextBlockId(),
        status: (block.status || "idle") as any,
        blockType: block.blockType || (block.file_path?.startsWith("exec") ? "terminal_command" as const : "file_edit" as const),
      })),
    }));
    setSession((prev) => prev ? { ...prev, message_history: historyWithIds } : null);
  }, [session]);

  const updateBlockStatus = useCallback((blockId: string, status: "idle" | "applied" | "accepted" | "rejected" | "running" | "completed") => {
    if (!session) return;
    const updated = session.message_history.map((msg) => ({
      ...msg,
      code_blocks: msg.code_blocks.map((block) =>
        block.id === blockId ? { ...block, status } : block
      ),
    }));
    setSession({ ...session, message_history: updated });
  }, [session]);

  const executeTerminalBlock = useCallback(async (blockId: string, command: string) => {
    if (!session) return;
    // Set status to running
    updateBlockStatus(blockId, "running");
    try {
      const result = await invoke<{ stdout: string; stderr: string; success: boolean }>("execute_composer_command", {
        command,
        workspaceRoot: "", // Will use default cwd
      });
      const output = `Command Output:\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}\nSuccess: ${result.success}`;
      // Update block with output
      const updated = session.message_history.map((msg) => ({
        ...msg,
        code_blocks: msg.code_blocks.map((block) =>
          block.id === blockId ? { ...block, status: "completed" as const, output } : block
        ),
      }));
      setSession({ ...session, message_history: updated });
      // Auto-feedback: send output back to AI
      await sendMessage(output);
    } catch (e) {
      updateBlockStatus(blockId, "completed");
    }
  }, [session, updateBlockStatus, sendMessage]);

  const close = useCallback(() => {
    setIsOpen(false);
  }, []);

  return { session, isOpen, initialize, addFile, removeFile, sendMessage, updateBlockStatus, executeTerminalBlock, close, setIsOpen };
}