"use client";

import { useCallback, useEffect, useRef } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { useTheme } from "@/hooks/useTheme";
import { useGhostText, GhostTextState } from "@/hooks/useGhostText";

export interface EditorProps {
  path: string;
  language: string;
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  readOnly?: boolean;
  workspaceGeneration?: number;
}

export default function Editor({ path, language, value, onChange, onSave, readOnly = false, workspaceGeneration = 0 }: EditorProps) {
  const onSaveRef = useRef(onSave);
  const { theme } = useTheme();
  const ghostText = useGhostText();
  const { ghost, suggestion, triggerGhostText, acceptGhost, dismissGhost } = ghostText;
  const setGhostRef = useRef(ghostText.setGhost || (() => {}));
  const ghostTextRef = useRef<string | null>(null);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  // Accept ghost text on Tab, dismiss on Esc or printable char
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!ghost.active || !editorRef.current) return;

    if (e.key === "Tab") {
      e.preventDefault();
      const insertion = acceptGhost();
      if (!insertion) return;
      const editor = editorRef.current;
      const position = editor.getPosition();
      if (!position) return;
      editor.executeEdits("ghost-text", [
        { range: new (window as any).monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column), text: insertion, forceMoveMarkers: true },
      ]);
      editor.setPosition({ lineNumber: position.lineNumber, column: position.column + insertion.length });
      editor.focus();
    } else if (e.key === "Escape" || e.key.length === 1) {
      dismissGhost();
    }
  }, [ghost.active, acceptGhost, dismissGhost]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // Sync suggestion state → ref for Monaco provider
  useEffect(() => { ghostTextRef.current = suggestion; }, [suggestion]);

  // Sync ghost setter into ref for use in Monaco provider closure
  useEffect(() => { setGhostRef.current = ghostText.setGhost || (() => {}); }, [ghostText.setGhost]);

  const handleMount: OnMount = (editor, monacoInstance) => {
    editorRef.current = editor;
    (window as any).monaco = monacoInstance;
    (window as any).__neuralforge_editor = editor;
    editor.focus();

    // Register InlineCompletionsProvider for ghost text
    monacoInstance.languages.registerInlineCompletionsProvider?.("*", {
      provideInlineCompletions: (model: any, position: any) => {
        const text = ghostTextRef.current;
        if (!text || text.length === 0) return { items: [] };
        return {
          items: [{ insertText: text, range: new monacoInstance.Range(position.lineNumber, position.column, position.lineNumber, position.column) }],
        };
      },
      // Invalidate completions when cursor moves — prevents stale ghost text
      // from persisting after cursor position changes during generation.
      freeInlineCompletions: (completions: any) => {
        setGhostRef.current({ text: "", requestId: null, active: false, filePath: null, workspaceGeneration: 0 });
      },
    });

    // Trigger ghost-text completion on cursor idle after edits.
    // The backend (`extract_prediction_window`) slices its own prefix/suffix
    // window from full file content plus a 0-indexed cursor line, so the
    // full content and real cursor position must be sent, not a pre-sliced
    // prefix/suffix pair.
    editor.onDidChangeCursorPosition((e: any) => {
      const model = editor.getModel();
      if (!model) return;
      const pos = e.position;
      const content = model.getValue();
      triggerGhostText(content, pos.lineNumber - 1, pos.column - 1, path, workspaceGeneration);
    });

    editor.addCommand(monacoInstance.KeyMod.CtrlCmd | monacoInstance.KeyCode.KeyS, () => {
      onSaveRef.current();
    });
  };

  // Invalidate ghost text when workspace generation changes (workspace switch)
  // or when the file path changes (file switch).
  useEffect(() => {
    if (!editorRef.current) return;
    // Clear any pending completions for the old generation/file
    editorRef.current.trigger("neuralforge-ghost", "inlineCommit", () => {});
  }, [path, workspaceGeneration]);

  return (
    <MonacoEditor
      path={path}
      language={language}
      value={value}
      theme={theme === "dark" ? "vs-dark" : "light"}
      onMount={handleMount}
      onChange={(v) => onChange(v ?? "")}
      loading={<div className="h-full w-full bg-white dark:bg-[#1e1e1e]" />}
      options={{
        readOnly,
        minimap: { enabled: true },
        fontSize: 13,
        automaticLayout: true,
        scrollBeyondLastLine: false,
      }}
    />
  );
}
