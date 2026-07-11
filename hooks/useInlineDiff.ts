"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

export type DiffOp = "Insert" | "Delete" | "Equal";

export interface DiffLinePayload {
  op: DiffOp;
  line: string;
  line_index: number;
  request_id: string;
}

export interface InlineDiffState {
  lines: DiffLinePayload[];
  requestId: string | null;
  active: boolean;
}

const DONE_LINE_INDEX = 4294967295; // usize::MAX

export function useInlineDiff() {
  const [diffState, setDiffState] = useState<InlineDiffState>({ lines: [], requestId: null, active: false });
  const unlistenRef = useRef<(() => void) | null>(null);
  const disposedRef = useRef(false);

  useEffect(() => {
    let disposed = false;
    disposedRef.current = false;

    listen<DiffLinePayload>("inline-diff-stream", (event) => {
      if (disposed) return;
      const payload = event.payload;

      // Check for done marker
      if (payload.line_index === DONE_LINE_INDEX) {
        setDiffState((prev) => ({ ...prev, active: false }));
        return;
      }

      setDiffState((prev) => {
        // New request: reset accumulated lines
        if (prev.requestId !== payload.request_id) {
          return { lines: [payload], requestId: payload.request_id, active: true };
        }
        return { ...prev, lines: [...prev.lines, payload] };
      });
    }).then((unlisten) => {
      if (disposed) { unlisten(); }
      else { unlistenRef.current = unlisten; }
    });

    return () => {
      disposed = true;
      disposedRef.current = true;
      unlistenRef.current?.();
    };
  }, []);

  const clearDiff = useCallback(() => {
    setDiffState({ lines: [], requestId: null, active: false });
  }, []);

  return { diffState, clearDiff };
}