"use client";

import { useState } from "react";
import Editor from "./Editor";
import DiffEditor from "@/components/editor/DiffEditor";
import TabBar from "./TabBar";
import EmptyState from "@/components/ui/EmptyState";
import { languageFromPath } from "@/lib/language";
import { useVersionCache } from "@/hooks/useVersionCache";
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
  const [isDiffMode, setIsDiffMode] = useState(false);
  const { setSnapshot, getSnapshot } = useVersionCache();
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
      {/* Diff mode toggle bar */}
      <div className="flex items-center gap-2 border-b border-[#333] bg-[#252526] px-3 py-1">
        <button
          onClick={() => {
            if (!isDiffMode && activeFile) {
              setSnapshot(activeFile.path, activeFile.content);
            }
            setIsDiffMode(!isDiffMode);
          }}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${isDiffMode ? "bg-blue-600 text-white" : "bg-[#333] text-[#aaa] hover:bg-[#444]"}`}
        >
          {isDiffMode ? "Diff Mode ON" : "Diff Mode OFF"}
        </button>
      </div>
      <div className="min-h-0 flex-1">
        {isDiffMode ? (
          <DiffEditor
            original={getSnapshot(activeFile.path) ?? activeFile.content}
            modified={activeFile.content}
            language={languageFromPath(activeFile.path)}
            originalPath={`original:${activeFile.path}`}
            modifiedPath={`modified:${activeFile.path}`}
          />
        ) : (
          <Editor
            path={activeFile.path}
            language={languageFromPath(activeFile.path)}
            value={activeFile.content}
            onChange={(v) => onChange(activeFile.path, v)}
            onSave={() => onSave(activeFile.path)}
          />
        )}
      </div>
    </div>
  );
}
