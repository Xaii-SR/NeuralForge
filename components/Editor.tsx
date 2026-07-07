"use client";

import { useRef } from "react";
import MonacoEditor, { OnMount } from "@monaco-editor/react";

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

  const handleMount: OnMount = (editor, monaco) => {
    editor.focus();
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      onSaveRef.current();
    });
  };

  return (
    <MonacoEditor
      path={path}
      language={language}
      value={value}
      theme="vs-dark"
      onMount={handleMount}
      onChange={(v) => onChange(v ?? "")}
      options={{
        minimap: { enabled: true },
        fontSize: 13,
        automaticLayout: true,
        scrollBeyondLastLine: false,
      }}
    />
  );
}
