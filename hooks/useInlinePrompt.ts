"use client";

import { useCallback, useRef, useState } from "react";

export interface InlinePromptState {
  isOpen: boolean;
  x: number;
  y: number;
  selectedText: string;
  cursorLine: number;
  selectionRange: { startLine: number; endLine: number };
}

export function useInlinePrompt() {
  const [state, setState] = useState<InlinePromptState>({
    isOpen: false, x: 0, y: 0, selectedText: "", cursorLine: 0,
    selectionRange: { startLine: 0, endLine: 0 },
  });
  const resolveRef = useRef<((value: string | null) => void) | null>(null);

  const open = useCallback(
    (x: number, y: number, selectedText: string, cursorLine: number, selectionRange: { startLine: number; endLine: number }) => {
      setState({ isOpen: true, x, y, selectedText, cursorLine, selectionRange });
      return new Promise<string | null>((resolve) => {
        resolveRef.current = resolve;
      });
    },
    []
  );

  const close = useCallback((result: string | null = null) => {
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, []);

  return { state, open, close };
}