"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export type InlineStatus = "idle" | "streaming" | "review";

export interface InlinePromptState {
  isOpen: boolean;
  x: number;
  y: number;
  selectedText: string;
  cursorLine: number;
  selectionRange: { startLine: number; endLine: number };
  status: InlineStatus;
  originalText: string;
  streamedText: string;
}

export interface InlineStreamPayload {
  chunk: string;
  done: boolean;
}

export function useInlinePrompt() {
  const [state, setState] = useState<InlinePromptState>({
    isOpen: false, x: 0, y: 0, selectedText: "", cursorLine: 0,
    selectionRange: { startLine: 0, endLine: 0 },
    status: "idle", originalText: "", streamedText: "",
  });
  const resolveRef = useRef<((value: string | null) => void) | null>(null);
  const streamedRef = useRef("");

  // Listen for inline-stream events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listen<InlineStreamPayload>("inline-stream", (event) => {
      if (disposed) return;
      const { chunk, done } = event.payload;
      if (done) {
        setState((prev) => ({ ...prev, status: "review" }));
      } else if (chunk) {
        streamedRef.current += chunk;
        setState((prev) => ({ ...prev, streamedText: streamedRef.current }));
      }
    }).then((fn) => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const open = useCallback(
    (x: number, y: number, selectedText: string, cursorLine: number, selectionRange: { startLine: number; endLine: number }) => {
      setState({
        isOpen: true, x, y, selectedText, cursorLine, selectionRange,
        status: "idle", originalText: selectedText, streamedText: "",
      });
      streamedRef.current = "";
      return new Promise<string | null>((resolve) => {
        resolveRef.current = resolve;
      });
    },
    []
  );

  const submitInlinePrompt = useCallback(async (prompt: string, filePath: string) => {
    setState((prev) => ({ ...prev, status: "streaming", streamedText: "" }));
    streamedRef.current = "";
    await invoke("stream_inline_edit", { prompt, selectedText: state.originalText, filePath });
  }, [state.originalText]);

  const acceptChanges = useCallback(() => {
    const result = state.streamedText || state.originalText;
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, [state.streamedText, state.originalText]);

  const rejectChanges = useCallback(() => {
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(state.originalText);
      resolveRef.current = null;
    }
  }, [state.originalText]);

  const close = useCallback((result: string | null = null) => {
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, []);

  return { state, open, close, submitInlinePrompt, acceptChanges, rejectChanges };
}