"use client";

import { useState } from "react";

export interface CopyButtonProps {
  text: string;
  className?: string;
}

/// Small, corner-positioned copy-to-clipboard button, matching VS Code's
/// hover-to-reveal convention on code blocks. Callers are responsible for
/// only rendering this once the surrounding text is complete (not
/// mid-stream) and for wrapping it in a `relative` container so absolute
/// positioning (e.g. `absolute top-1 right-1`) places it in the corner.
export default function CopyButton({ text, className }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard API can fail (permissions, insecure context) - the
      // button simply doesn't confirm; no error banner for a non-critical
      // convenience action.
    }
  }

  return (
    <button
      onClick={handleCopy}
      title={copied ? "Copied!" : "Copy to clipboard"}
      aria-label="Copy to clipboard"
      className={`rounded border border-neutral-200 bg-white px-1.5 py-1 text-[11px] text-neutral-400 shadow-sm transition-colors hover:bg-neutral-100 hover:text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-500 dark:hover:bg-neutral-700 dark:hover:text-neutral-200 ${className ?? ""}`}
    >
      {copied ? "✓" : "⧉"}
    </button>
  );
}
