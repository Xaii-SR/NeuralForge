"use client";

import { useEffect, useRef } from "react";
import { DiffEditor as MonacoDiffEditor } from "@monaco-editor/react";
import { useSmartScroll } from "@/hooks/useSmartScroll";

export interface DiffEditorProps {
  original: string;
  modified: string;
  language: string;
  originalPath?: string;
  modifiedPath?: string;
}

export default function DiffEditor({ original, modified, language, originalPath, modifiedPath }: DiffEditorProps) {
  const modifiedEditorRef = useRef<any>(null);
  const { isAutoScrollLocked } = useSmartScroll(modifiedEditorRef.current);
  const prevModifiedLengthRef = useRef(modified.length);

  // Capture the modified editor instance when diff is mounted
  const handleMount = (diffEditor: any) => {
    modifiedEditorRef.current = diffEditor.getModifiedEditor?.() ?? diffEditor;
  };

  // Auto-scroll when modified text grows
  useEffect(() => {
    if (modified.length > prevModifiedLengthRef.current && isAutoScrollLocked) {
      const ed = modifiedEditorRef.current;
      if (ed?.getModel && ed.getModel()) {
        const lineCount = ed.getModel().getLineCount();
        ed.revealLine(lineCount, 1); // Smooth scroll
      }
    }
    prevModifiedLengthRef.current = modified.length;
  }, [modified, isAutoScrollLocked]);

  return (
    <MonacoDiffEditor
      original={original}
      modified={modified}
      language={language}
      originalModelPath={originalPath}
      modifiedModelPath={modifiedPath}
      theme="vs-dark"
      onMount={handleMount}
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
