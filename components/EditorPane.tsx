"use client";

import { useState } from "react";
import Editor from "./Editor";
import TabBar, { Tab } from "./TabBar";
import { languageFromPath } from "@/lib/language";

interface OpenFile extends Tab {
  content: string;
}

export interface EditorPaneProps {
  initialFiles?: { path: string; content: string }[];
}

export default function EditorPane({ initialFiles = [] }: EditorPaneProps) {
  const [files, setFiles] = useState<OpenFile[]>(
    initialFiles.map((f) => ({ path: f.path, content: f.content, isDirty: false }))
  );
  const [activePath, setActivePath] = useState<string | null>(
    initialFiles[0]?.path ?? null
  );

  const activeFile = files.find((f) => f.path === activePath) ?? null;

  function handleChange(path: string, value: string) {
    setFiles((prev) =>
      prev.map((f) => (f.path === path ? { ...f, content: value, isDirty: true } : f))
    );
  }

  function handleClose(path: string) {
    setFiles((prev) => prev.filter((f) => f.path !== path));
    if (activePath === path) {
      const remaining = files.filter((f) => f.path !== path);
      setActivePath(remaining[remaining.length - 1]?.path ?? null);
    }
  }

  if (!activeFile) {
    return (
      <div className="flex h-full w-full items-center justify-center text-sm text-neutral-500">
        No file open
      </div>
    );
  }

  return (
    <div className="flex h-full w-full flex-col">
      <TabBar
        tabs={files}
        activePath={activePath}
        onSelect={setActivePath}
        onClose={handleClose}
      />
      <div className="min-h-0 flex-1">
        <Editor
          path={activeFile.path}
          language={languageFromPath(activeFile.path)}
          value={activeFile.content}
          onChange={(v) => handleChange(activeFile.path, v)}
        />
      </div>
    </div>
  );
}
