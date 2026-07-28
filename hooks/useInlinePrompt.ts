"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getAppConfig } from "@/lib/store";

export type InlineStatus = "idle" | "streaming" | "review";

export interface InlinePromptState {
  isOpen: boolean;
  x: number;
  y: number;
  selectedText: string;
  cursorLine: number;
  selectionRange: { startLine: number; endLine: number };
  filePath: string;
  workspaceGeneration: number;
  documentVersion: number;
  selectionStartColumn: number;
  selectionEndColumn: number;
  status: InlineStatus;
  originalText: string;
  streamedText: string;
  error: string | null;
}

export interface InlineStreamPayload {
  request_id: string;
  workspace_generation: number;
  file_path: string;
  document_version: number;
  selection_start_line: number;
  selection_start_column: number;
  selection_end_line: number;
  selection_end_column: number;
  chunk: string;
  done: boolean;
  status?: "success" | "cancelled" | "error";
  error?: string | null;
}

export function useInlinePrompt() {
  const [state, setState] = useState<InlinePromptState>({
    isOpen: false, x: 0, y: 0, selectedText: "", cursorLine: 0,
    selectionRange: { startLine: 0, endLine: 0 },
    filePath: "", workspaceGeneration: 0, documentVersion: 0,
    selectionStartColumn: 0, selectionEndColumn: 0,
    status: "idle", originalText: "", streamedText: "", error: null,
  });
  const resolveRef = useRef<((value: string | null) => void) | null>(null);
  const streamedRef = useRef("");
  const activeRequestRef = useRef<{
    requestId: string;
    workspaceGeneration: number;
    filePath: string;
    documentVersion: number;
  } | null>(null);

  const cancelActiveRequest = useCallback(() => {
    const active = activeRequestRef.current;
    if (!active) return;
    void invoke<boolean>("cancel_ai_request", { requestId: active.requestId });
  }, []);

  // Listen for inline-stream events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listen<InlineStreamPayload>("inline-stream", (event) => {
      if (disposed) return;
      const { chunk, done, error, status } = event.payload;
      const active = activeRequestRef.current;
      if (!active
        || event.payload.request_id !== active.requestId
        || event.payload.workspace_generation !== active.workspaceGeneration
        || event.payload.file_path !== active.filePath
        || event.payload.document_version !== active.documentVersion) return;
      if (error) {
        activeRequestRef.current = null;
        setState((prev) => ({ ...prev, status: "idle", error }));
        return;
      }
      if (done) {
        activeRequestRef.current = null;
        setState((prev) => ({
          ...prev,
          status: status === "success" ? "review" : "idle",
          error: status === "cancelled" ? "Inline Edit cancelled." : prev.error,
        }));
      } else if (chunk) {
        streamedRef.current += chunk;
        setState((prev) => ({ ...prev, streamedText: streamedRef.current }));
      }
    }).then((fn) => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const open = useCallback(
    (
      x: number,
      y: number,
      selectedText: string,
      cursorLine: number,
      selectionRange: { startLine: number; endLine: number },
      metadata: {
        filePath: string;
        workspaceGeneration: number;
        documentVersion: number;
        selectionStartColumn: number;
        selectionEndColumn: number;
      },
    ) => {
      cancelActiveRequest();
      activeRequestRef.current = null;
      setState({
        isOpen: true, x, y, selectedText, cursorLine, selectionRange,
        ...metadata,
        status: "idle", originalText: selectedText, streamedText: "", error: null,
      });
      streamedRef.current = "";
      return new Promise<string | null>((resolve) => {
        resolveRef.current = resolve;
      });
    },
    [cancelActiveRequest]
  );

  const submitInlinePrompt = useCallback(async (prompt: string) => {
    cancelActiveRequest();
    const requestId = crypto.randomUUID();
    const active = {
      requestId,
      workspaceGeneration: state.workspaceGeneration,
      filePath: state.filePath,
      documentVersion: state.documentVersion,
    };
    activeRequestRef.current = active;
    setState((prev) => ({ ...prev, status: "streaming", streamedText: "", error: null }));
    streamedRef.current = "";
    try {
      const config = await getAppConfig();
      await invoke("stream_inline_edit", {
        requestId,
        workspaceGeneration: state.workspaceGeneration,
        prompt,
        selectedText: state.originalText,
        filePath: state.filePath,
        documentVersion: state.documentVersion,
        selectionStartLine: state.selectionRange.startLine,
        selectionStartColumn: state.selectionStartColumn,
        selectionEndLine: state.selectionRange.endLine,
        selectionEndColumn: state.selectionEndColumn,
        shareWorkspaceContext: config.shareWorkspaceContextWithCloud,
      });
    } catch (error) {
      if (activeRequestRef.current?.requestId !== requestId) return;
      activeRequestRef.current = null;
      setState((prev) => ({ ...prev, status: "idle", error: String(error) }));
    }
  }, [cancelActiveRequest, state]);

  const acceptChanges = useCallback(() => {
    const result = state.streamedText || state.originalText;
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, [state.streamedText, state.originalText, state.filePath, state.documentVersion, state.workspaceGeneration]);

  const rejectChanges = useCallback(() => {
    cancelActiveRequest();
    activeRequestRef.current = null;
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(state.originalText);
      resolveRef.current = null;
    }
  }, [cancelActiveRequest, state.originalText]);

  const close = useCallback((result: string | null = null) => {
    cancelActiveRequest();
    activeRequestRef.current = null;
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, [cancelActiveRequest]);

  const failReview = useCallback((error: string) => {
    setState((prev) => ({ ...prev, status: "idle", error }));
  }, []);

  return { state, open, close, submitInlinePrompt, acceptChanges, rejectChanges, failReview };
}
