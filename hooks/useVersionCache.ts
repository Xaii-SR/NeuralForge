"use client";

import { useCallback, useRef } from "react";

export function useVersionCache() {
  const cacheRef = useRef<Record<string, string>>({});

  const setSnapshot = useCallback((path: string, text: string) => {
    cacheRef.current[path] = text;
  }, []);

  const getSnapshot = useCallback((path: string): string | null => {
    return cacheRef.current[path] ?? null;
  }, []);

  const clearSnapshot = useCallback((path: string) => {
    delete cacheRef.current[path];
  }, []);

  const clearAll = useCallback(() => {
    cacheRef.current = {};
  }, []);

  return { setSnapshot, getSnapshot, clearSnapshot, clearAll };
}