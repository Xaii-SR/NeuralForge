"use client";

import { useCallback, useEffect, useRef } from "react";

const SCROLL_THRESHOLD_PX = 50;

/**
 * Tracks a Monaco editor's scroll position and determines whether the
 * viewport is near the bottom. Returns `isAutoScrollLocked` — when true,
 * the caller should automatically reveal the last line on content updates.
 *
 * The hook pauses auto-scrolling if the user scrolls up past the threshold,
 * and resumes when they scroll back to the bottom.
 */
export function useSmartScroll(
  editor: { getScrollTop: () => number; getScrollHeight: () => number; getScrollHeightMinusScrollTop: () => number } | null,
) {
  const isAtBottomRef = useRef(true);

  const checkScrollPosition = useCallback(() => {
    if (!editor) return true;
    const clientHeight = typeof (editor as any).getClientHeight === "function" ? (editor as any).getClientHeight() : 0;
    const remaining = editor.getScrollHeight() - editor.getScrollTop() - clientHeight;
    const nearBottom = remaining < SCROLL_THRESHOLD_PX;
    isAtBottomRef.current = nearBottom;
    return nearBottom;
  }, [editor]);

  // Subscribe to scroll events on mount
  useEffect(() => {
    if (!editor || !(editor as any).onDidScrollChange) return;
    const disposable = (editor as any).onDidScrollChange(() => {
      checkScrollPosition();
    });
    return () => { disposable.dispose(); };
  }, [editor, checkScrollPosition]);

  const isAutoScrollLocked = isAtBottomRef.current;

  return { isAutoScrollLocked, revealBottom: checkScrollPosition };
}