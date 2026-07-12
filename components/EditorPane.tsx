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
  const { pendingDiffs, setPendingDiffs, activeDiffIndex, setActiveDiffIndex } = useComposer();
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;
  const editorContainerRef = useRef<HTMLDivElement | null>(null);
  const inlineDecorationsRef = useRef<string[]>([]);
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

  const handleAccept = useCallback(() => { clearDecorations(); acceptChanges(); }, [clearDecorations, acceptChanges]);
  const handleReject = useCallback(() => { clearDecorations(); rejectChanges(); }, [clearDecorations, rejectChanges]);

  useEffect(() => {
    const editor = (window as any).__neuralforge_editor;
    const monaco = (window as any).monaco;
    if (!editor || !monaco) return;
    if (prompt.status === "review" && prompt.originalText && prompt.streamedText) {
      const origLines = prompt.originalText.split("\n").length;
      const streamLines = prompt.streamedText.split("\n").length;
      const decorations: any[] = [];
      for (let i = 1; i <= origLines; i++)
        decorations.push({ range: new monaco.Range(i, 1, i, 1), options: { isWholeLine: true, className: "inline-diff-deleted" } });
      for (let i = origLines + 1; i <= origLines + streamLines; i++)
        decorations.push({ range: new monaco.Range(i, 1, i, 1), options: { isWholeLine: true, className: "inline-diff-inserted" } });
      inlineDecorationsRef.current = editor.deltaDecorations(inlineDecorationsRef.current, decorations);
    } else if (prompt.status !== "streaming" && inlineDecorationsRef.current.length > 0) {
      inlineDecorationsRef.current = editor.deltaDecorations(inlineDecorationsRef.current, []);
    }
  }, [prompt.status, prompt.originalText, prompt.streamedText]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      const c = editorContainerRef.current;
      if (!c || !activeFile) return;
      const r = c.getBoundingClientRect();
      openPrompt(r.left + 20, r.top + 60, activeFile.content.substring(0, 200), 1, { startLine: 1, endLine: 1 });
    }
  }, [activeFile, openPrompt]);
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
      <div className="flex items-center gap-2 border-b border-[#333] bg-[#252526] px-3 py-1">
        <button onClick={() => { if (!isDiffMode && activeFile) setSnapshot(activeFile.path, activeFile.content); setIsDiffMode(!isDiffMode); }}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${isDiffMode ? "bg-blue-600 text-white" : "bg-[#333] text-[#aaa] hover:bg-[#444]"}`}>{isDiffMode ? "Diff Mode ON" : "Diff Mode OFF"}</button>
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
          onSubmit={async (v) => { submitInlinePrompt(v, activeFile?.path ?? ""); return null; }}
          onAccept={handleAccept}
          onReject={handleReject}
          onClose={() => closePrompt(null)}
        />
      )}
    </div>
  );
}