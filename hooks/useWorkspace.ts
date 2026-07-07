"use client";

import { useCallback, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as fs from "@/lib/fs";

export interface OpenFile {
  path: string;
  content: string;
  isDirty: boolean;
}

export function useWorkspace() {
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [openFiles, setOpenFiles] = useState<OpenFile[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);

  const openFolder = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || typeof selected !== "string") return;
    const root = await fs.openWorkspace(selected);
    setWorkspaceRoot(root);
    setOpenFiles([]);
    setActivePath(null);
  }, []);

  const openFile = useCallback(
    async (path: string) => {
      const existing = openFiles.find((f) => f.path === path);
      if (existing) {
        setActivePath(path);
        return;
      }
      const content = await fs.readFile(path);
      setOpenFiles((prev) => [...prev, { path, content, isDirty: false }]);
      setActivePath(path);
    },
    [openFiles]
  );

  const closeFile = useCallback(
    (path: string) => {
      setOpenFiles((prev) => prev.filter((f) => f.path !== path));
      if (activePath === path) {
        const remaining = openFiles.filter((f) => f.path !== path);
        setActivePath(remaining[remaining.length - 1]?.path ?? null);
      }
    },
    [activePath, openFiles]
  );

  const updateContent = useCallback((path: string, content: string) => {
    setOpenFiles((prev) =>
      prev.map((f) => (f.path === path ? { ...f, content, isDirty: true } : f))
    );
  }, []);

  const saveFile = useCallback(
    async (path: string) => {
      const file = openFiles.find((f) => f.path === path);
      if (!file) return;
      await fs.writeFile(path, file.content);
      setOpenFiles((prev) =>
        prev.map((f) => (f.path === path ? { ...f, isDirty: false } : f))
      );
    },
    [openFiles]
  );

  return {
    workspaceRoot,
    openFiles,
    activePath,
    setActivePath,
    openFolder,
    openFile,
    closeFile,
    updateContent,
    saveFile,
  };
}
