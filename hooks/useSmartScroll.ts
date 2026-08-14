"use client";

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

const SCROLL_THRESHOLD_PX = 50;

/**
 * Tracks a Monaco editor's scroll position and determines whether the
 * viewport is near the bottom. Returns `isAutoScrollLocked` — when true,
 * the caller should automatically reveal the last line on content updates.
 *
 * The hook pauses auto-scrolling if the user scrolls up past the threshold,
 * and resumes when they scroll back to the bottom.
 */
type ScrollEditor = { getScrollTop: () => number; getScrollHeight: () => number; getScrollHeightMinusScrollTop: () => number };

export function useSmartScroll(editorRef: RefObject<ScrollEditor | null>) {
  const [isAtBottom, setIsAtBottom] = useState(true);

  const checkScrollPosition = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) return true;
    const clientHeight = typeof (editor as any).getClientHeight === "function" ? (editor as any).getClientHeight() : 0;
    const remaining = editor.getScrollHeight() - editor.getScrollTop() - clientHeight;
    const nearBottom = remaining < SCROLL_THRESHOLD_PX;
    setIsAtBottom((previous) => (previous === nearBottom ? previous : nearBottom));
    return nearBottom;
  }, [editorRef]);

  // Subscribe to scroll events on mount
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || !(editor as any).onDidScrollChange) return;
    const disposable = (editor as any).onDidScrollChange(() => {
      checkScrollPosition();
    });
    return () => { disposable.dispose(); };
  }, [editorRef, checkScrollPosition]);

  return { isAutoScrollLocked: isAtBottom, revealBottom: checkScrollPosition };
}
