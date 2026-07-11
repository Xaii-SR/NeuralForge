"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export interface CodeBlock {
  file_path: string;
  language: string;
  code: string;
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
    setSession((prev) => prev ? { ...prev, message_history: history } : null);
  }, [session]);

  const close = useCallback(() => {
    setIsOpen(false);
  }, []);

  return { session, isOpen, initialize, addFile, removeFile, sendMessage, close, setIsOpen };
}