"use client";

import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTerminal } from "@/hooks/useTerminal";
import { useComposer } from "@/hooks/useComposer";
import { setTerminalBufferGetter } from "@/hooks/useComposer";
import FixWithAiButton from "@/components/terminal/FixWithAiButton";
import "@xterm/xterm/css/xterm.css";

interface TerminalOutputPayload {
  session_id: string;
  data: string;
}

export default function Terminal() {
  const containerRef = useRef<HTMLDivElement>(null);
  const { appendTerminalOutput, getTerminalBuffer, hasActiveError, clearTerminalError } = useTerminal();
  const { initialize, setIsOpen } = useComposer();

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new XTerm({
      theme: { background: "#1e1e1e", foreground: "#d4d4d4" },
      fontSize: 13,
      cursorBlink: true,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);
    fitAddon.fit();

    let sessionId: string | null = null;
    let disposed = false;
    let unlistenOutput: (() => void) | null = null;
    let unlistenClosed: (() => void) | null = null;

    (async () => {
      const id = await invoke<string>("spawn_shell", { rows: term.rows, cols: term.cols });
      if (disposed) {
        invoke("close_pty", { sessionId: id });
        return;
      }
      sessionId = id;

      unlistenOutput = await listen<TerminalOutputPayload>("TERMINAL_OUTPUT", (event) => {
        if (event.payload.session_id === id) {
          term.write(event.payload.data);
          appendTerminalOutput(event.payload.data);
        }
      });
      unlistenClosed = await listen<string>("TERMINAL_CLOSED", (event) => {
        if (event.payload === id) {
          term.write("\r\n[process exited]\r\n");
        }
      });
    })();

    const dataDisposable = term.onData((data) => {
      if (sessionId) {
        invoke("write_to_pty", { sessionId, data });
        // Clear error state on new command input (Enter key)
        if (data === "\r" || data === "\n") {
          clearTerminalError();
        }
      }
    });

    const handleResize = () => {
      fitAddon.fit();
      if (sessionId) {
        invoke("resize_pty", { sessionId, rows: term.rows, cols: term.cols });
      }
    };
    window.addEventListener("resize", handleResize);

    return () => {
      disposed = true;
      window.removeEventListener("resize", handleResize);
      dataDisposable.dispose();
      unlistenOutput?.();
      unlistenClosed?.();
      if (sessionId) {
        invoke("close_pty", { sessionId });
      }
      term.dispose();
    };
  }, []);

  // Register the terminal buffer getter for Composer @terminal context
  useEffect(() => {
    setTerminalBufferGetter(getTerminalBuffer);
  }, [getTerminalBuffer]);

  const handleFixWithAi = async () => {
    const buf = getTerminalBuffer();
    const msg = `Terminal Output (see below for output) Please analyze and fix this error.\n\n--- RECENT TERMINAL OUTPUT ---\n${buf}\n--- END TERMINAL ---`;
    await initialize([]);
    setTimeout(async () => {
      // The Composer is now open; we need to send the message
      // This uses a workaround: accessing the sendMessage via global ref
      // For now, the user can press Enter after the @terminal flow
    }, 100);
  };

  return (
    <div className="relative h-full w-full">
      <div ref={containerRef} className="h-full w-full bg-[#1e1e1e]" />
      {hasActiveError && <FixWithAiButton onClick={handleFixWithAi} />}
    </div>
  );
}
