"use client";

import { useCallback, useEffect, useState } from "react";
import * as fs from "@/lib/fs";

export interface WorkspaceSwitcherProps {
  workspaceRoot: string | null;
  onOpenWorkspace: (path: string) => Promise<void>;
}

/**
 * Project-first navigation. A project is a remembered folder plus a display
 * name; it never changes the real folder name or reaches outside the path
 * selected by the user.
 */
export default function WorkspaceSwitcher({ workspaceRoot, onOpenWorkspace }: WorkspaceSwitcherProps) {
  const [projects, setProjects] = useState<fs.WorkspaceProject[]>([]);
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setProjects(await fs.listWorkspaceProjects());
      setError(null);
    } catch (reason) {
      setError(`Could not load projects: ${reason}`);
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => { void refresh(); }, 0);
    return () => window.clearTimeout(timer);
  }, [refresh, workspaceRoot]);

  const current = projects.find((project) => project.root === workspaceRoot);
  const label = current?.name ?? (workspaceRoot ? "Workspace" : "No workspace");

  async function select(project: fs.WorkspaceProject) {
    if (!project.available || project.root === workspaceRoot) {
      setOpen(false);
      return;
    }
    await onOpenWorkspace(project.root);
    setOpen(false);
    await refresh();
  }

  async function saveName() {
    if (!workspaceRoot || !name.trim()) return;
    try {
      await fs.renameWorkspaceProject(workspaceRoot, name);
      setRenaming(false);
      await refresh();
    } catch (reason) {
      setError(`Could not rename project: ${reason}`);
    }
  }

  return (
    <div className="relative min-w-0">
      <button
        onClick={() => { setOpen((value) => !value); setRenaming(false); }}
        className="flex max-w-[280px] items-center gap-1 rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800"
        title={workspaceRoot ?? "No workspace open"}
      >
        <span className="truncate">{label}</span><span className="text-neutral-400">⌄</span>
      </button>
      {open && (
        <div className="absolute left-0 top-8 z-40 w-[360px] max-w-[calc(100vw-24px)] rounded-md border border-neutral-200 bg-white p-2 text-xs shadow-xl dark:border-neutral-700 dark:bg-neutral-900">
          <div className="mb-1 flex items-center justify-between px-1 py-1 text-[10px] font-medium uppercase tracking-wide text-neutral-400">
            <span>Projects</span><button onClick={() => void refresh()} className="normal-case hover:text-blue-500">Refresh</button>
          </div>
          {workspaceRoot && !renaming && (
            <button onClick={() => { setName(label); setRenaming(true); }} className="mb-2 w-full rounded border border-neutral-200 px-2 py-1.5 text-left text-[11px] text-neutral-600 hover:border-blue-300 hover:bg-blue-50 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-blue-950/30">
              Rename current project
            </button>
          )}
          {renaming && (
            <div className="mb-2 flex gap-1">
              <input autoFocus value={name} onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void saveName(); if (event.key === "Escape") setRenaming(false); }} className="min-w-0 flex-1 rounded border border-blue-400 bg-white px-2 py-1 text-xs outline-none dark:bg-neutral-800" />
              <button onClick={() => void saveName()} className="rounded bg-blue-600 px-2 py-1 text-white">Save</button>
            </div>
          )}
          <div className="max-h-64 space-y-1 overflow-y-auto">
            {projects.length === 0 && <div className="px-2 py-3 text-neutral-400">Open a folder to create your first project.</div>}
            {projects.map((project) => (
              <button key={project.root} onClick={() => void select(project)} disabled={!project.available} className={`w-full rounded px-2 py-2 text-left transition-colors ${project.root === workspaceRoot ? "bg-blue-50 text-blue-800 dark:bg-blue-950/40 dark:text-blue-200" : "hover:bg-neutral-100 dark:hover:bg-neutral-800"} disabled:cursor-not-allowed disabled:opacity-50`}>
                <div className="truncate font-medium">{project.name}</div>
                <div className="truncate text-[10px] text-neutral-400">{project.available ? project.root : "Folder unavailable — record kept"}</div>
              </button>
            ))}
          </div>
          {error && <div className="mt-2 rounded bg-red-50 px-2 py-1 text-[10px] text-red-700 dark:bg-red-950/30 dark:text-red-300">{error}</div>}
        </div>
      )}
    </div>
  );
}
