"use client";

import { useCallback, useRef, useState } from "react";

export interface MentionMenuState {
  isOpen: boolean;
  coords: { x: number; y: number };
  query: string;
  activeIndex: number;
}

export function useMentionMenu() {
  const [state, setState] = useState<MentionMenuState>({
    isOpen: false,
    coords: { x: 0, y: 0 },
    query: "",
    activeIndex: 0,
  });
  const resolveRef = useRef<((value: string | null) => void) | null>(null);

  const open = useCallback((coords: { x: number; y: number }, query: string) => {
    setState({ isOpen: true, coords, query, activeIndex: 0 });
    return new Promise<string | null>((resolve) => {
      resolveRef.current = resolve;
    });
  }, []);

  const close = useCallback((result: string | null = null) => {
    setState((prev) => ({ ...prev, isOpen: false }));
    if (resolveRef.current) {
      resolveRef.current(result);
      resolveRef.current = null;
    }
  }, []);

  const setQuery = useCallback((query: string) => {
    setState((prev) => ({ ...prev, query, activeIndex: 0 }));
  }, []);

  const setActiveIndex = useCallback((index: number) => {
    setState((prev) => ({ ...prev, activeIndex: index }));
  }, []);

  return { state, open, close, setQuery, setActiveIndex };
}