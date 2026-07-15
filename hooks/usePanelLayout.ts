"use client";

import { useCallback, useEffect, useLayoutEffect, useRef } from "react";

export type PanelId = "sidebar" | "chat" | "bottom";

const STORAGE_KEY = "nf_layout_v1";

const DEFAULTS: Record<PanelId, number> = { sidebar: 256, chat: 320, bottom: 288 };
const MIN: Record<PanelId, number> = { sidebar: 160, chat: 240, bottom: 120 };
const MAX: Record<PanelId, number> = { sidebar: 520, chat: 680, bottom: 720 };

/** The center (editor) column may never be squeezed below this width. */
const MIN_CENTER_WIDTH = 320;
/** The editor area above the bottom panel keeps at least this height. */
const MIN_EDITOR_HEIGHT = 160;

const CSS_VAR: Record<PanelId, string> = {
  sidebar: "--nf-sidebar-w",
  chat: "--nf-chat-w",
  bottom: "--nf-bottom-h",
};

function loadStoredSizes(): Record<PanelId, number> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const parsed = raw ? JSON.parse(raw) : null;
    const sizes = { ...DEFAULTS };
    for (const panel of Object.keys(DEFAULTS) as PanelId[]) {
      const value = parsed?.[panel];
      if (typeof value === "number" && Number.isFinite(value)) {
        sizes[panel] = Math.min(MAX[panel], Math.max(MIN[panel], value));
      }
    }
    return sizes;
  } catch {
    return { ...DEFAULTS };
  }
}

/**
 * VS Code-style resizable workspace layout. Panel sizes live in CSS variables
 * on the layout root so dragging never re-renders React (Monaco and xterm are
 * expensive to reflow through state updates); sizes persist to localStorage
 * and are restored on the next launch. This hook is the single owner of
 * layout sizing state.
 */
export function usePanelLayout() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const sizesRef = useRef<Record<PanelId, number>>({ ...DEFAULTS });

  const apply = useCallback((panel: PanelId, px: number) => {
    rootRef.current?.style.setProperty(CSS_VAR[panel], `${px}px`);
  }, []);

  const persist = useCallback(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(sizesRef.current));
    } catch {
      // Persistence is best-effort; resizing still works for the session.
    }
  }, []);

  const clamp = useCallback((panel: PanelId, desired: number, rootRect: DOMRect): number => {
    let max = MAX[panel];
    if (panel === "sidebar") {
      max = Math.min(max, rootRect.width - sizesRef.current.chat - MIN_CENTER_WIDTH);
    } else if (panel === "chat") {
      max = Math.min(max, rootRect.width - sizesRef.current.sidebar - MIN_CENTER_WIDTH);
    } else {
      max = Math.min(max, rootRect.height - MIN_EDITOR_HEIGHT);
    }
    return Math.round(Math.min(Math.max(desired, MIN[panel]), Math.max(max, MIN[panel])));
  }, []);

  // Restore persisted sizes before first paint.
  useLayoutEffect(() => {
    const stored = loadStoredSizes();
    sizesRef.current = stored;
    for (const panel of Object.keys(stored) as PanelId[]) apply(panel, stored[panel]);
  }, [apply]);

  // Re-clamp when the window shrinks so panels never swallow the editor.
  useEffect(() => {
    let frame = 0;
    const onResize = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const root = rootRef.current;
        if (!root) return;
        const rect = root.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) return;
        for (const panel of Object.keys(sizesRef.current) as PanelId[]) {
          const clamped = clamp(panel, sizesRef.current[panel], rect);
          if (clamped !== sizesRef.current[panel]) {
            sizesRef.current[panel] = clamped;
            apply(panel, clamped);
          }
        }
      });
    };
    window.addEventListener("resize", onResize);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", onResize);
    };
  }, [apply, clamp]);

  const startDrag = useCallback(
    (panel: PanelId) => (e: React.PointerEvent<HTMLDivElement>) => {
      const root = rootRef.current;
      if (!root || e.button !== 0) return;
      e.preventDefault();
      const handle = e.currentTarget;
      handle.setPointerCapture(e.pointerId);
      document.body.classList.add(panel === "bottom" ? "nf-resizing-row" : "nf-resizing-col");
      const rect = root.getBoundingClientRect();

      const onMove = (ev: PointerEvent) => {
        const desired =
          panel === "sidebar" ? ev.clientX - rect.left :
          panel === "chat" ? rect.right - ev.clientX :
          rect.bottom - ev.clientY;
        const next = clamp(panel, desired, rect);
        if (next !== sizesRef.current[panel]) {
          sizesRef.current[panel] = next;
          apply(panel, next);
        }
      };

      const end = (ev: PointerEvent) => {
        if (handle.hasPointerCapture(ev.pointerId)) handle.releasePointerCapture(ev.pointerId);
        handle.removeEventListener("pointermove", onMove);
        handle.removeEventListener("pointerup", end);
        handle.removeEventListener("pointercancel", end);
        document.body.classList.remove("nf-resizing-col", "nf-resizing-row");
        persist();
      };

      handle.addEventListener("pointermove", onMove);
      handle.addEventListener("pointerup", end);
      handle.addEventListener("pointercancel", end);
    },
    [apply, clamp, persist]
  );

  const resetPanel = useCallback(
    (panel: PanelId) => {
      sizesRef.current[panel] = DEFAULTS[panel];
      apply(panel, DEFAULTS[panel]);
      persist();
    },
    [apply, persist]
  );

  const nudgePanel = useCallback(
    (panel: PanelId, delta: number) => {
      const root = rootRef.current;
      if (!root) return;
      const next = clamp(panel, sizesRef.current[panel] + delta, root.getBoundingClientRect());
      sizesRef.current[panel] = next;
      apply(panel, next);
      persist();
    },
    [apply, clamp, persist]
  );

  return { rootRef, startDrag, resetPanel, nudgePanel };
}
