"use client";

import Editor from "./Editor";
import TabBar from "./TabBar";
import EmptyState from "@/components/ui/EmptyState";
import { languageFromPath } from "@/lib/language";
import type { OpenFile } from "@/hooks/useWorkspace";

export interface EditorPaneProps {
  openFiles: OpenFile[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onChange: (path: string, value: string) => void;
  onSave: (path: string) => void;
}

export default function EditorPane({
  openFiles,
  activePath,
  onSelect,
  onClose,
  onChange,
  onSave,
}: EditorPaneProps) {
  const activeFile = openFiles.find((f) => f.path === activePath) ?? null;

  if (!activeFile) {
    return (
      <EmptyState
        icon="📝"
        title="No file open"
        hint="Select a file from the explorer, or open a folder to get started"
      />
    );
  }

  return (
    <div className="flex h-full w-full flex-col">
      <TabBar tabs={openFiles} activePath={activePath} onSelect={onSelect} onClose={onClose} />
      <div className="min-h-0 flex-1">
        <Editor
          path={activeFile.path}
          language={languageFromPath(activeFile.path)}
          value={activeFile.content}
          onChange={(v) => onChange(activeFile.path, v)}
          onSave={() => onSave(activeFile.path)}
        />
      </div>
    </div>
  );
}
