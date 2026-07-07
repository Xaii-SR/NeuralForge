"use client";

import MonacoEditor, { OnMount } from "@monaco-editor/react";

export interface EditorProps {
  path: string;
  language: string;
  value: string;
  onChange: (value: string) => void;
}

export default function Editor({ path, language, value, onChange }: EditorProps) {
  const handleMount: OnMount = (editor) => {
    editor.focus();
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
