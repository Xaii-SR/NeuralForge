"use client";

import { DiffEditor as MonacoDiffEditor } from "@monaco-editor/react";

export interface DiffEditorProps {
  original: string;
  modified: string;
  language: string;
  originalPath?: string;
  modifiedPath?: string;
}

export default function DiffEditor({ original, modified, language, originalPath, modifiedPath }: DiffEditorProps) {
  return (
    <MonacoDiffEditor
      original={original}
      modified={modified}
      language={language}
      originalModelPath={originalPath}
      modifiedModelPath={modifiedPath}
      theme="vs-dark"
      options={{
        fontSize: 13,
        automaticLayout: true,
        minimap: { enabled: true },
        scrollBeyondLastLine: false,
        renderSideBySide: true,
        readOnly: false,
        originalEditable: false,
      }}
    />
  );
}