"use client";

import { useEffect, useState } from "react";
import * as fs from "@/lib/fs";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";

interface TreeNodeProps {
  entry: fs.FileEntry;
  depth: number;
  onFileClick: (path: string) => void;
}

function TreeNode({ entry, depth, onFileClick }: TreeNodeProps) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<fs.FileEntry[] | null>(null);
  const [loading, setLoading] = useState(false);

  async function toggle() {
    if (!entry.is_dir) {
      onFileClick(entry.path);
      return;
    }
    if (!expanded && children === null) {
      setLoading(true);
      try {
        const loaded = await fs.readDir(entry.path);
        setChildren(loaded);
      } finally {
        setLoading(false);
      }
    }
    setExpanded((prev) => !prev);
  }

  return (
    <div>
      <div
        onClick={toggle}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
        className="flex cursor-pointer items-center gap-1 whitespace-nowrap py-1 text-sm text-neutral-700 transition-colors hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
      >
        <span
          className={`inline-block w-3 text-[10px] text-neutral-400 transition-transform dark:text-neutral-500 ${
            entry.is_dir && expanded ? "rotate-90" : ""
          }`}
        >
          {entry.is_dir ? "▸" : ""}
        </span>
        <span className="truncate">{entry.name}</span>
        {loading && <Spinner size={10} />}
      </div>
      {expanded && children && (
        <div>
          {children.map((child) => (
            <TreeNode key={child.path} entry={child} depth={depth + 1} onFileClick={onFileClick} />
          ))}
          {children.length === 0 && (
            <div style={{ paddingLeft: `${(depth + 1) * 14 + 8}px` }} className="py-1 text-xs text-neutral-400 dark:text-neutral-600">
              Empty
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export interface FileExplorerProps {
  workspaceRoot: string;
  onFileClick: (path: string) => void;
}

export default function FileExplorer({ workspaceRoot, onFileClick }: FileExplorerProps) {
  const [entries, setEntries] = useState<fs.FileEntry[] | null>(null);

  useEffect(() => {
    setEntries(null);
    fs.readDir(workspaceRoot).then(setEntries);
  }, [workspaceRoot]);

  if (entries === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (entries.length === 0) {
    return <EmptyState icon="📂" title="This folder is empty" />;
  }

  return (
    <div className="h-full overflow-y-auto bg-white py-2 dark:bg-neutral-900">
      {entries.map((entry) => (
        <TreeNode key={entry.path} entry={entry} depth={0} onFileClick={onFileClick} />
      ))}
    </div>
  );
}
