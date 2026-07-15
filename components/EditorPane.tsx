"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Editor from "./Editor";
import DiffEditor from "@/components/editor/DiffEditor";
import DiffActionBar from "@/components/editor/DiffActionBar";
import InlinePromptWidget from "@/components/editor/InlinePromptWidget";
import TabBar from "./TabBar";
import { invoke } from "@tauri-apps/api/core";
import EmptyState from "@/components/ui/EmptyState";
import { languageFromPath } from "@/lib/language";
import { useVersionCache } from "@/hooks/useVersionCache";
import { useComposer } from "@/hooks/useComposer";
import { useInlinePrompt } from "@/hooks/useInlinePrompt";
import { useInlineDiff } from "@/hooks/useInlineDiff";
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
  openFiles, activePath, onSelect, onClose, onChange, onSave,
  activeComposerBlockId = null, onDiffResolved,
}: EditorPaneProps) {
  const [isDiffMode, setIsDiffMode] = useState(false);
  const { setSnapshot, getSnapshot, clearSnapshot } = useVersionCache();
  const { state: prompt, open: openPrompt, close: closePrompt, submitInlinePrompt, acceptChanges, rejectChanges } = useInlinePrompt();
  const { diffState, clearDiff } = useInlineDiff();
  const { pendingDiffs, setPendingDiffs, activeDiffIndex, setActiveDiffIndex } = useComposer();
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;
  const editorContainerRef = useRef<HTMLDivElement | null>(null);
  const inlineDecorationsRef = useRef<string[]>([]);
  const selectionRangeRef = useRef<any>(null);
  const [diffOriginal, setDiffOriginal] = useState<string>("");
  const [diffLanguage, setDiffLanguage] = useState("text");

  const currentDiff = pendingDiffs[activeDiffIndex] ?? null;
  const isDiffReview = currentDiff !== null;

  // When currentDiff changes, fetch the original file content
  useEffect(() => {
    if (!currentDiff) return;
    const f = openFiles.find((x) => x.path === currentDiff.filePath);
    if (f) {
      setDiffOriginal(f.content);
      setDiffLanguage(languageFromPath(f.path));
    } else {
      invoke<string>("read_file", { path: currentDiff.filePath })
        .then(setDiffOriginal)
        .catch(() => setDiffOriginal("// Could not read file"));
      setDiffLanguage(languageFromPath(currentDiff.filePath));
    }
  }, [currentDiff, openFiles]);

  const handleDiffAccept = async () => {
    if (!currentDiff) return;
    try {
      await invoke("write_file", { path: currentDiff.filePath, contents: currentDiff.newCode });
      if (activeFile?.path === currentDiff.filePath) {
        onChange(currentDiff.filePath, currentDiff.newCode);
      }
    } catch { /* ignore */ }
    removeActiveDiff();
  };

  const handleDiffReject = () => {
    removeActiveDiff();
  };

  const removeActiveDiff = () => {
    setPendingDiffs((prev) => {
      const next = [...prev];
      next.splice(activeDiffIndex, 1);
      if (activeDiffIndex > next.length - 1) {
        setActiveDiffIndex(Math.max(0, next.length - 1));
      }
      return next;
    });
  };

  const clearDecorations = useCallback(() => {
    const editor = (window as any).__neuralforge_editor;
    if (editor && inlineDecorationsRef.current.length > 0) {
      inlineDecorationsRef.current = editor.deltaDecorations(inlineDecorationsRef.current, []);
    }
    editor?.focus();
  }, []);

  const handleAccept = useCallback(() => {
    const editor = (window as any).__neuralforge_editor;
    const monaco = (window as any).monaco;
    const range = selectionRangeRef.current;
    if (editor && monaco && range && activeFile) {
      editor.executeEdits("inline-prompt-accept", [
        { range, text: prompt.streamedText, forceMoveMarkers: true },
      ]);
      onChange(activeFile.path, editor.getValue());
    }
    clearDecorations();
    clearDiff();
    acceptChanges();
  }, [clearDecorations, clearDiff, acceptChanges, prompt.streamedText, activeFile, onChange]);

  const handleReject = useCallback(() => { clearDecorations(); clearDiff(); rejectChanges(); }, [clearDecorations, clearDiff, rejectChanges]);

  // Real per-line diff decorations, streamed from the backend's stream_inline_diff.
  useEffect(() => {
    const editor = (window as any).__neuralforge_editor;
    const monaco = (window as any).monaco;
    if (!editor || !monaco) return;
    if (diffState.active || diffState.lines.length > 0) {
      const decorations: any[] = [];
      for (const dl of diffState.lines) {
        const lineNum = dl.line_index + 1;
        if (dl.op === "Insert") {
          decorations.push({ range: new monaco.Range(lineNum, 1, lineNum, 1), options: { isWholeLine: true, className: "inline-diff-inserted" } });
        } else if (dl.op === "Delete") {
          decorations.push({ range: new monaco.Range(lineNum, 1, lineNum, 1), options: { isWholeLine: true, className: "inline-diff-deleted" } });
        }
      }
      inlineDecorationsRef.current = editor.deltaDecorations(inlineDecorationsRef.current, decorations);
    } else if (inlineDecorationsRef.current.length > 0) {
      inlineDecorationsRef.current = editor.deltaDecorations(inlineDecorationsRef.current, []);
    }
  }, [diffState]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    // Inline prompt review state: Cmd+Enter = accept, Escape = reject
    if (prompt.status === "review") {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        handleAccept();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        handleReject();
        return;
      }
    }

    if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      const c = editorContainerRef.current;
      if (!c || !activeFile) return;
      const editor = (window as any).__neuralforge_editor;
      const selection = editor?.getSelection?.();
      const model = editor?.getModel?.();
      const selectedText = selection && model ? model.getValueInRange(selection) : "";
      selectionRangeRef.current = selection ?? null;
      const cursorLine = selection?.positionLineNumber ?? 1;
      const r = c.getBoundingClientRect();
      openPrompt(r.left + 20, r.top + 60, selectedText, cursorLine, { startLine: cursorLine, endLine: cursorLine });
    }
  }, [activeFile, openPrompt, prompt.status, handleAccept, handleReject]);
  useEffect(() => { window.addEventListener("keydown", handleKeyDown); return () => window.removeEventListener("keydown", handleKeyDown); }, [handleKeyDown]);

  if (!activeFile) return <EmptyState icon="📝" title="No file open" hint="Select a file from the explorer, or open a folder to get started" />;

  return (
    <div className="flex h-full w-full flex-col" ref={editorContainerRef}>
      <TabBar tabs={openFiles} activePath={activePath} onSelect={onSelect} onClose={onClose} />
      <DiffActionBar
        isDiffMode={isDiffMode}
        onAccept={() => { if (!activeFile) return; if (activeComposerBlockId && onDiffResolved) onDiffResolved(activeComposerBlockId, "accepted"); clearSnapshot(activeFile.path); setIsDiffMode(false); }}
        onReject={() => { if (!activeFile) return; if (activeComposerBlockId && onDiffResolved) onDiffResolved(activeComposerBlockId, "rejected"); const o = getSnapshot(activeFile.path); if (o !== null) onChange(activeFile.path, o); clearSnapshot(activeFile.path); setIsDiffMode(false); }}
      />
      <div className="flex items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-3 py-1 dark:border-neutral-800 dark:bg-neutral-900">
        <button onClick={() => { if (!isDiffMode && activeFile) setSnapshot(activeFile.path, activeFile.content); setIsDiffMode(!isDiffMode); }}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${isDiffMode ? "bg-blue-600 text-white" : "bg-neutral-200 text-neutral-600 hover:bg-neutral-300 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"}`}>{isDiffMode ? "Diff Mode ON" : "Diff Mode OFF"}</button>
      </div>
      <div className="min-h-0 flex-1">
        {isDiffReview ? (
          <>
            <DiffActionBar
              isDiffMode={true}
              onAccept={handleDiffAccept}
              onReject={handleDiffReject}
              totalDiffs={pendingDiffs.length}
              activeDiffIndex={activeDiffIndex}
              onPrevDiff={() => setActiveDiffIndex((i: number) => Math.max(0, i - 1))}
              onNextDiff={() => setActiveDiffIndex((i: number) => Math.min(pendingDiffs.length - 1, i + 1))}
            />
            <DiffEditor
              original={diffOriginal}
              modified={currentDiff?.newCode ?? ""}
              language={diffLanguage}
              originalPath={currentDiff?.filePath ?? "original"}
              modifiedPath={`modified:${currentDiff?.filePath ?? ""}`}
            />
          </>
        ) : isDiffMode ? (
          <DiffEditor original={getSnapshot(activeFile.path) ?? activeFile.content} modified={activeFile.content} language={languageFromPath(activeFile.path)}
            originalPath={`original:${activeFile.path}`} modifiedPath={`modified:${activeFile.path}`} />
        ) : (
          <Editor path={activeFile.path} language={languageFromPath(activeFile.path)} value={activeFile.content}
            onChange={(v) => onChange(activeFile.path, v)} onSave={() => onSave(activeFile.path)} />
        )}
      </div>
      {prompt.isOpen && (
        <InlinePromptWidget
          x={prompt.x} y={prompt.y}
          initialValue={prompt.selectedText ? `refactor: ${prompt.selectedText}` : ""}
          status={prompt.status}
          error={prompt.error}
          onSubmit={async (v) => { submitInlinePrompt(v, activeFile?.path ?? ""); return null; }}
          onAccept={handleAccept}
          onReject={handleReject}
          onClose={() => closePrompt(null)}
        />
      )}
    </div>
  );
}