"use client";

import { KeyboardEvent, TextareaHTMLAttributes, useEffect, useRef } from "react";

export interface AutoResizeTextareaProps extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "onKeyDown" | "rows"> {
  onSubmit: () => void;
  maxRows?: number;
}

// Enter sends, Shift+Enter inserts a newline (impossible on a plain
// <input>, which is why this exists as a real <textarea>), and the height
// grows with content up to maxRows before scrolling internally.
export default function AutoResizeTextarea({ onSubmit, maxRows = 8, value, className, ...rest }: AutoResizeTextareaProps) {
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    const lineHeight = parseFloat(getComputedStyle(el).lineHeight || "16") || 16;
    const maxHeight = lineHeight * maxRows;
    const nextHeight = Math.min(el.scrollHeight, maxHeight);
    el.style.height = `${nextHeight}px`;
    el.style.overflowY = el.scrollHeight > maxHeight ? "auto" : "hidden";
  }, [value, maxRows]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      onSubmit();
    }
  }

  return <textarea ref={ref} value={value} onKeyDown={handleKeyDown} rows={1} className={className} {...rest} />;
}
