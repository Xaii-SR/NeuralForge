"use client";

import { useCallback, useRef, useState } from "react";

const MAX_LINES = 1000;
const ANSI_REGEX = /\x1b\[[0-9;]*[a-zA-Z]/g;
const ERROR_REGEX = /(error|exception|failed|panic|traceback|fatal|killed|timed out):?/i;

/**
 * Manages a rolling buffer of terminal output for AI context injection.
 * Strips ANSI escape codes and keeps only the most recent MAX_LINES lines.
 * Also tracks when error signatures appear for the "Fix with AI" button.
 */
export function useTerminal() {
  const bufferRef = useRef<string[]>([]);
  const [hasActiveError, setHasActiveError] = useState(false);

  const appendTerminalOutput = useCallback((raw: string) => {
    // Strip ANSI codes
    const clean = raw.replace(ANSI_REGEX, "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    if (!clean.trim()) return;

    // Check for error signatures
    if (ERROR_REGEX.test(clean)) {
      setHasActiveError(true);
    }

    const lines = clean.split("\n");
    bufferRef.current.push(...lines);

    // Trim to max lines
    if (bufferRef.current.length > MAX_LINES) {
      bufferRef.current = bufferRef.current.slice(bufferRef.current.length - MAX_LINES);
    }
  }, []);

  const clearTerminalError = useCallback(() => {
    setHasActiveError(false);
  }, []);

  const getTerminalBuffer = useCallback(() => {
    return bufferRef.current.join("\n");
  }, []);

  const clearBuffer = useCallback(() => {
    bufferRef.current = [];
    setHasActiveError(false);
  }, []);

  return { appendTerminalOutput, getTerminalBuffer, clearBuffer, hasActiveError, clearTerminalError };
}
