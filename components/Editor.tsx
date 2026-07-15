"use client";

import { useCallback, useEffect, useRef } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { useTheme } from "@/hooks/useTheme";
import { useGhostText } from "@/hooks/useGhostText";

export interface EditorProps {
  path: string;
  language: string;
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
}

export default function Editor({ path, language, value, onChange, onSave }: EditorProps) {
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const { theme } = useTheme();
  const { ghost, suggestion, triggerGhostText, acceptGhost, dismissGhost } = useGhostText();
  const ghostTextRef = useRef<string | null>(null);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

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
      freeInlineCompletions: (completions: any) => {},
    });

    // Trigger ghost-text completion on cursor idle after edits
    editor.onDidChangeCursorPosition((e: any) => {
      const model = editor.getModel();
      if (!model) return;
      const pos = e.position;
      const lineCount = model.getLineCount();
      const prefix = model.getValueInRange({
        startLineNumber: 1, startColumn: 1,
        endLineNumber: pos.lineNumber, endColumn: pos.column,
      });
      const suffix = model.getValueInRange({
        startLineNumber: pos.lineNumber, startColumn: pos.column,
        endLineNumber: lineCount, endColumn: model.getLineMaxColumn(lineCount),
      });
      triggerGhostText(prefix, suffix, path);
    });

    editor.addCommand(monacoInstance.KeyMod.CtrlCmd | monacoInstance.KeyCode.KeyS, () => {
      onSaveRef.current();
    });
  };

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
        minimap: { enabled: true },
        fontSize: 13,
        automaticLayout: true,
        scrollBeyondLastLine: false,
      }}
    />
  );
}