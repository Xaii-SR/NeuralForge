"use client";

import { useCallback, useRef } from "react";

const MAX_LINES = 1000;
const ANSI_REGEX = /\x1b\[[0-9;]*[a-zA-Z]/g;

/**
 * Manages a rolling buffer of terminal output for AI context injection.
 * Strips ANSI escape codes and keeps only the most recent MAX_LINES lines.
 */
export function useTerminal() {
  const bufferRef = useRef<string[]>([]);

  const appendTerminalOutput = useCallback((raw: string) => {
    // Strip ANSI codes
    const clean = raw.replace(ANSI_REGEX, "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    if (!clean.trim()) return;

    const lines = clean.split("\n");
    bufferRef.current.push(...lines);

    // Trim to max lines
    if (bufferRef.current.length > MAX_LINES) {
      bufferRef.current = bufferRef.current.slice(bufferRef.current.length - MAX_LINES);
    }
  }, []);

  const getTerminalBuffer = useCallback(() => {
    return bufferRef.current.join("\n");
  }, []);

  const clearBuffer = useCallback(() => {
    bufferRef.current = [];
  }, []);

  return { appendTerminalOutput, getTerminalBuffer, clearBuffer };
}