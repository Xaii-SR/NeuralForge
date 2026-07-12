"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export interface GhostTextStreamPayload {
  token: string;
  done: boolean;
  request_id: string;
}
export interface GhostTextState {
  text: string;
  requestId: string | null;
  active: boolean;
}

export function useGhostText() {
  const [ghost, setGhost] = useState<GhostTextState>({ text: "", requestId: null, active: false });
  const [suggestion, setSuggestion] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  // Listen for ghost-text-stream events
  useEffect(() => {
    let disposed = false;
    listen<GhostTextStreamPayload>("ghost-text-stream", (event) => {
      if (disposed) return;
      const { token, done, request_id } = event.payload;
      setGhost((prev) => {
        if (prev.requestId !== request_id) return { text: token, requestId: request_id, active: !done };
        if (done) return { ...prev, active: true };
        return { ...prev, text: prev.text + token };
      });
    }).then((fn) => { if (disposed) fn(); else unlistenRef.current = fn; });
    return () => { disposed = true; unlistenRef.current?.(); };
  }, []);

  // Debounced FIM ghost text trigger (300ms)
  const triggerGhostText = useCallback((prefix: string, suffix: string, path: string) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        const result = await invoke<string>("fetch_ghost_suggestion", { prefix, suffix, filePath: path });
        setSuggestion(result);
      } catch { setSuggestion(null); }
    }, 300);
  }, []);

  const clearSuggestion = useCallback(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    setSuggestion(null);
  }, []);

  const acceptGhost = useCallback(() => {
    const text = ghost.text;
    setGhost({ text: "", requestId: null, active: false });
    return text;
  }, [ghost.text]);

  const dismissGhost = useCallback(() => {
    setGhost({ text: "", requestId: null, active: false });
  }, []);

  return { ghost, suggestion, triggerGhostText, clearSuggestion, acceptGhost, dismissGhost };
}