"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as fs from "@/lib/fs";
import {
  acceptSavedReplacement,
  acknowledgeSavedRevision,
  createBuffer,
  dirtyBufferPaths,
  saveDirtySnapshotsForDecision,
  shouldAcceptWorkspaceResponse,
  updateBuffer,
  type WorkspaceBuffer,
} from "@/lib/workspace-buffers";

export type OpenFile = WorkspaceBuffer;
export type UnsavedAction = "save" | "discard" | "cancel";

export interface PendingUnsavedChanges {
  paths: string[];
  reason: "close_file" | "switch_workspace" | "close_app";
  saving: boolean;
  error: string | null;
}

export function useWorkspace() {
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [workspaceGeneration, setWorkspaceGeneration] = useState(0);
  const [openFiles, setOpenFiles] = useState<OpenFile[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [pendingUnsaved, setPendingUnsaved] = useState<PendingUnsavedChanges | null>(null);
  const [workspaceTransitioning, setWorkspaceTransitioning] = useState(false);
  const restoreAttempted = useRef(false);
  const openFilesRef = useRef<OpenFile[]>([]);
  const workspaceGenerationRef = useRef(0);
  const workspaceRequestRef = useRef(0);
  const workspaceTransitioningRef = useRef(false);
  const decisionSavingRef = useRef(false);
  const unsavedResolver = useRef<((approved: boolean) => void) | null>(null);

  useEffect(() => {
    openFilesRef.current = openFiles;
  }, [openFiles]);

  useEffect(() => {
    workspaceGenerationRef.current = workspaceGeneration;
  }, [workspaceGeneration]);

  const replaceOpenFiles = useCallback((next: OpenFile[]) => {
    openFilesRef.current = next;
    setOpenFiles(next);
  }, []);

  const mutateOpenFiles = useCallback((update: (current: OpenFile[]) => OpenFile[]) => {
    const next = update(openFilesRef.current);
    openFilesRef.current = next;
    setOpenFiles(next);
    return next;
  }, []);

  const persistSnapshot = useCallback(async (file: OpenFile) => {
    await fs.writeFile(file.path, file.content);
    mutateOpenFiles((current) =>
      acknowledgeSavedRevision(current, file.path, file.revision)
    );
  }, [mutateOpenFiles]);

  const requestUnsavedDecision = useCallback(
    (paths: string[], reason: PendingUnsavedChanges["reason"]): Promise<boolean> => {
      const dirty = openFilesRef.current.filter(
        (file) => paths.includes(file.path) && file.isDirty
      );
      if (dirty.length === 0) return Promise.resolve(true);
      if (unsavedResolver.current) return Promise.resolve(false);

      return new Promise<boolean>((resolve) => {
        unsavedResolver.current = resolve;
        setPendingUnsaved({
          paths: dirty.map((file) => file.path),
          reason,
          saving: false,
          error: null,
        });
      });
    },
    []
  );

  const resolveUnsaved = useCallback(
    async (action: UnsavedAction) => {
      const pending = pendingUnsaved;
      const resolve = unsavedResolver.current;
      if (!pending || !resolve) return;

      if (action === "cancel") {
        unsavedResolver.current = null;
        setPendingUnsaved(null);
        resolve(false);
        return;
      }

      if (action === "discard") {
        unsavedResolver.current = null;
        setPendingUnsaved(null);
        resolve(true);
        return;
      }

      setPendingUnsaved({ ...pending, saving: true, error: null });
      decisionSavingRef.current = true;
      try {
        const saved = await saveDirtySnapshotsForDecision(
          pending.paths,
          () => openFilesRef.current,
          persistSnapshot
        );
        if (!saved) {
          throw new Error("a file changed while it was being saved");
        }
        unsavedResolver.current = null;
        setPendingUnsaved(null);
        resolve(true);
        queueMicrotask(() => {
          decisionSavingRef.current = false;
        });
      } catch (error) {
        decisionSavingRef.current = false;
        setPendingUnsaved({
          ...pending,
          saving: false,
          error: `Could not save changes: ${error}`,
        });
      }
    },
    [pendingUnsaved, persistSnapshot]
  );

  useEffect(() => {
    if (restoreAttempted.current) return;
    restoreAttempted.current = true;
    (async () => {
      const request = ++workspaceRequestRef.current;
      try {
        const last = await fs.getLastWorkspace();
        if (!last || request !== workspaceRequestRef.current) return;
        const workspace = await fs.openWorkspace(last);
        if (!shouldAcceptWorkspaceResponse(
          request,
          workspaceRequestRef.current,
          workspace.generation,
          workspaceGenerationRef.current
        )) return;
        workspaceGenerationRef.current = workspace.generation;
        setWorkspaceRoot(workspace.root);
        setWorkspaceGeneration(workspace.generation);
      } catch {
        // A failed restore leaves the normal no-workspace state intact.
      }
    })();
  }, []);

  useEffect(() => () => {
    const resolve = unsavedResolver.current;
    unsavedResolver.current = null;
    resolve?.(false);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const appWindow = getCurrentWindow();
      const stop = await appWindow.onCloseRequested(async (event) => {
        const dirty = dirtyBufferPaths(openFilesRef.current);
        if (dirty.length === 0) return;
        event.preventDefault();
        const approved = await requestUnsavedDecision(dirty, "close_app");
        if (approved) await appWindow.destroy();
      });
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [requestUnsavedDecision]);

  useEffect(() => {
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      if (dirtyBufferPaths(openFilesRef.current).length === 0) return;
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, []);

  const openFolder = useCallback(async () => {
    if (workspaceTransitioningRef.current) return;
    const selected = await open({ directory: true, multiple: false });
    if (!selected || typeof selected !== "string") return;
    const request = ++workspaceRequestRef.current;
    workspaceTransitioningRef.current = true;
    setWorkspaceTransitioning(true);
    try {
      const approved = await requestUnsavedDecision(
        dirtyBufferPaths(openFilesRef.current),
        "switch_workspace"
      );
      if (!approved) return;

      const workspace = await fs.openWorkspace(selected);
      if (!shouldAcceptWorkspaceResponse(
        request,
        workspaceRequestRef.current,
        workspace.generation,
        workspaceGenerationRef.current
      )) return;
      workspaceGenerationRef.current = workspace.generation;
      setWorkspaceRoot(workspace.root);
      setWorkspaceGeneration(workspace.generation);
      replaceOpenFiles([]);
      setActivePath(null);
    } finally {
      if (request === workspaceRequestRef.current) {
        workspaceTransitioningRef.current = false;
        setWorkspaceTransitioning(false);
      }
    }
  }, [replaceOpenFiles, requestUnsavedDecision]);

  const openFile = useCallback(async (path: string) => {
    const existing = openFilesRef.current.find((file) => file.path === path);
    if (existing) {
      setActivePath(path);
      return;
    }

    const generation = workspaceGenerationRef.current;
    const content = await fs.readFile(path);
    if (generation !== workspaceGenerationRef.current) return;
    mutateOpenFiles((current) => {
      if (current.some((file) => file.path === path)) return current;
      return [...current, createBuffer(path, content)];
    });
    setActivePath(path);
  }, [mutateOpenFiles]);

  const closeFile = useCallback(
    async (path: string) => {
      const approved = await requestUnsavedDecision([path], "close_file");
      if (!approved) return;
      const remaining = openFilesRef.current.filter((file) => file.path !== path);
      replaceOpenFiles(remaining);
      setActivePath((current) =>
        current === path ? remaining[remaining.length - 1]?.path ?? null : current
      );
    },
    [replaceOpenFiles, requestUnsavedDecision]
  );

  const updateContent = useCallback((path: string, content: string) => {
    if (decisionSavingRef.current || workspaceTransitioningRef.current) return;
    mutateOpenFiles((current) => updateBuffer(current, path, content));
  }, [mutateOpenFiles]);

  const saveFile = useCallback(
    async (path: string) => {
      const file = openFilesRef.current.find((candidate) => candidate.path === path);
      if (!file) return;
      await persistSnapshot(file);
    },
    [persistSnapshot]
  );

  const acceptExternalWrite = useCallback(
    (path: string, expectedContent: string, content: string) => {
      mutateOpenFiles((current) =>
        acceptSavedReplacement(current, path, expectedContent, content)
      );
    },
    [mutateOpenFiles]
  );

  return {
    workspaceRoot,
    workspaceGeneration,
    openFiles,
    activePath,
    pendingUnsaved,
    editingLocked: workspaceTransitioning || pendingUnsaved?.saving === true,
    setActivePath,
    openFolder,
    openFile,
    closeFile,
    updateContent,
    saveFile,
    acceptExternalWrite,
    resolveUnsaved,
  };
}
