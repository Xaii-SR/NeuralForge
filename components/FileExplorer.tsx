"use client";

import { useEffect, useState } from "react";
import * as fs from "@/lib/fs";

interface TreeNodeProps {
  entry: fs.FileEntry;
  depth: number;
  onFileClick: (path: string) => void;
}

function TreeNode({ entry, depth, onFileClick }: TreeNodeProps) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<fs.FileEntry[] | null>(null);

  async function toggle() {
    if (!entry.is_dir) {
      onFileClick(entry.path);
      return;
    }
    if (!expanded && children === null) {
      const loaded = await fs.readDir(entry.path);
      setChildren(loaded);
    }
    setExpanded((prev) => !prev);
  }

  return (
    <div>
      <div
        onClick={toggle}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
        className="cursor-pointer whitespace-nowrap py-0.5 text-sm text-neutral-300 hover:bg-neutral-800"
      >
        {entry.is_dir ? (expanded ? "▾ " : "▸ ") : "  "}
        {entry.name}
      </div>
      {expanded && children && (
        <div>
          {children.map((child) => (
            <TreeNode key={child.path} entry={child} depth={depth + 1} onFileClick={onFileClick} />
          ))}
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
  const [entries, setEntries] = useState<fs.FileEntry[]>([]);

  useEffect(() => {
    fs.readDir(workspaceRoot).then(setEntries);
  }, [workspaceRoot]);

  return (
    <div className="h-full overflow-y-auto bg-neutral-900 py-2">
      {entries.map((entry) => (
        <TreeNode key={entry.path} entry={entry} depth={0} onFileClick={onFileClick} />
      ))}
    </div>
  );
}
