"use client";

import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

export function useEvent<T>(eventName: string, handler: (payload: T) => void) {
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;

    listen<T>(eventName, (event) => handler(event.payload)).then((fn) => {
      if (disposed) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [eventName]);
}
