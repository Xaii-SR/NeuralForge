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
  const { ghost, acceptGhost, dismissGhost } = useGhostText();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);

  // Accept ghost text on Tab, dismiss on Esc or any other key
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

  const handleMount: OnMount = (editor, monacoInstance) => {
    editorRef.current = editor;
    editor.focus();

    (window as any).monaco = monacoInstance;

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