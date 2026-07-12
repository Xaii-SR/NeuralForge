"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Editor from "./Editor";
import DiffEditor from "@/components/editor/DiffEditor";
import DiffActionBar from "@/components/editor/DiffActionBar";
import InlinePromptWidget from "@/components/editor/InlinePromptWidget";
import TabBar from "./TabBar";
import EmptyState from "@/components/ui/EmptyState";
import { languageFromPath } from "@/lib/language";
import { useVersionCache } from "@/hooks/useVersionCache";
import { useInlinePrompt } from "@/hooks/useInlinePrompt";
import type { OpenFile } from "@/hooks/useWorkspace";

export interface EditorPaneProps {
  openFiles: OpenFile[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onChange: (path: string, value: string) => void;
  onSave: (path: string) => void;
  activeComposerBlockId?: string | null;
  onDiffResolved?: (blockId: string, status: "accepted" | "rejected") => void;
}

export default function EditorPane({
  openFiles,
  activePath,
  onSelect,
  onClose,
  onChange,
  onSave,
  activeComposerBlockId = null,
  onDiffResolved,
}: EditorPaneProps) {
  const [isDiffMode, setIsDiffMode] = useState(false);
  const { setSnapshot, getSnapshot, clearSnapshot } = useVersionCache();
  const { state: prompt, open: openPrompt, close: closePrompt, submitInlinePrompt, acceptChanges, rejectChanges } = useInlinePrompt();
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;
  const editorContainerRef = useRef<HTMLDivElement | null>(null);
  const inlineDecorationsRef = useRef<string[]>([]);

  // Inline diff decorations on review state
  useEffect(() => {
    const container = editorContainerRef.current;
    if (!container || !activeFile) return;
    const editorEl = container.querySelector('[data-uri*="model"]') as any;
    const monaco = (window as any).monaco;
    if (!editorEl || !monaco) return;

    if (prompt.status === "review" && prompt.originalText && prompt.streamedText) {
      const origLines = prompt.originalText.split("\n").length;
      const streamLines = prompt.streamedText.split("\n").length;
      const totalLines = origLines + streamLines;

      const decorations: any[] = [];
      // Mark original portion as deleted (red)
      for (let i = 1; i <= origLines; i++) {
        decorations.push({
          range: new monaco.Range(i, 1, i, 1),
          options: { isWholeLine: true, className: "inline-diff-deleted" },
        });
      }
      // Mark streamed portion as inserted (green)
      for (let i = origLines + 1; i <= totalLines; i++) {
        decorations.push({
          range: new monaco.Range(i, 1, i, 1),
          options: { isWholeLine: true, className: "inline-diff-inserted" },
        });
      }
      // We would need the editor instance; for now just store them
      // In practice, these are applied via the editor API
    } else if (prompt.status !== "streaming") {
      // Clear decorations when leaving review
      inlineDecorationsRef.current = [];
    }
  }, [prompt.status, prompt.originalText, prompt.streamedText, activeFile]);

  // Cmd+K / Ctrl+K: Inline Prompt
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      const container = editorContainerRef.current;
      if (!container || !activeFile) return;
      const rect = container.getBoundingClientRect();
      const x = rect.left + 20;
      const y = rect.top + 60;
      const selectedText = activeFile.content.substring(0, 200);
      const cursorLine = 1;
      openPrompt(x, y, selectedText, cursorLine, { startLine: 1, endLine: 1 });
    }
  }, [activeFile, openPrompt]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!activeFile) {
    return (
      <EmptyState
        icon="📝"
        title="No file open"
        hint="Select a file from the explorer, or open a folder to get started"
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-col" ref={editorContainerRef}>
      <TabBar tabs={openFiles} activePath={activePath} onSelect={onSelect} onClose={onClose} />
      <DiffActionBar
        isDiffMode={isDiffMode}
        onAccept={() => {
          if (!activeFile) return;
          if (activeComposerBlockId && onDiffResolved) { onDiffResolved(activeComposerBlockId, "accepted"); }
          clearSnapshot(activeFile.path);
          setIsDiffMode(false);
        }}
        onReject={() => {
          if (!activeFile) return;
          if (activeComposerBlockId && onDiffResolved) { onDiffResolved(activeComposerBlockId, "rejected"); }
          const original = getSnapshot(activeFile.path);
          if (original !== null) { onChange(activeFile.path, original); }
          clearSnapshot(activeFile.path);
          setIsDiffMode(false);
        }}
      />
      <div className="flex items-center gap-2 border-b border-[#333] bg-[#252526] px-3 py-1">
        <button
          onClick={() => {
            if (!isDiffMode && activeFile) { setSnapshot(activeFile.path, activeFile.content); }
            setIsDiffMode(!isDiffMode);
          }}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${isDiffMode ? "bg-blue-600 text-white" : "bg-[#333] text-[#aaa] hover:bg-[#444]"}`}
        >
          {isDiffMode ? "Diff Mode ON" : "Diff Mode OFF"}
        </button>
      </div>
      <div className="min-h-0 flex-1">
        {isDiffMode ? (
          <DiffEditor
            original={getSnapshot(activeFile.path) ?? activeFile.content}
            modified={activeFile.content}
            language={languageFromPath(activeFile.path)}
            originalPath={`original:${activeFile.path}`}
            modifiedPath={`modified:${activeFile.path}`}
          />
        ) : (
          <Editor
            path={activeFile.path}
            language={languageFromPath(activeFile.path)}
            value={activeFile.content}
            onChange={(v) => onChange(activeFile.path, v)}
            onSave={() => onSave(activeFile.path)}
          />
        )}
      </div>
      {prompt.isOpen && (
        <InlinePromptWidget
          x={prompt.x}
          y={prompt.y}
          initialValue={prompt.selectedText ? `refactor: ${prompt.selectedText}` : ""}
          status={prompt.status}
          onSubmit={async (v) => { submitInlinePrompt(v, activeFile?.path ?? ""); return null; }}
          onAccept={() => acceptChanges()}
          onReject={() => rejectChanges()}
          onClose={() => closePrompt(null)}
        />
      )}
    </div>
  );
}