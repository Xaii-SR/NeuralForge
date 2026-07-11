"use client";

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
  const unlistenRef = useRef<(() => void) | null>(null);
  const disposedRef = useRef(false);

  useEffect(() => {
    let disposed = false;
    disposedRef.current = false;

    listen<GhostTextStreamPayload>("ghost-text-stream", (event) => {
      if (disposed) return;
      const { token, done, request_id } = event.payload;

      setGhost((prev) => {
        // New request: reset accumulated text
        if (prev.requestId !== request_id) {
          return { text: token, requestId: request_id, active: !done };
        }
        // Ongoing stream: append token
        if (done) {
          // Keep final text but mark inactive (waiting for Tab or dismiss)
          return { ...prev, active: true };
        }
        return { ...prev, text: prev.text + token };
      });
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlistenRef.current = unlisten;
      }
    });

    return () => {
      disposed = true;
      disposedRef.current = true;
      unlistenRef.current?.();
    };
  }, []);

  const acceptGhost = useCallback(() => {
    const text = ghost.text;
    setGhost({ text: "", requestId: null, active: false });
    return text;
  }, [ghost.text]);

  const dismissGhost = useCallback(() => {
    setGhost({ text: "", requestId: null, active: false });
  }, []);

  return { ghost, acceptGhost, dismissGhost };
}