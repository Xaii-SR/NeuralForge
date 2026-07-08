"use client";

import { useRef } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";
import { useTheme } from "@/hooks/useTheme";

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

  const handleMount: OnMount = (editor, monacoInstance) => {
    editor.focus();
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
