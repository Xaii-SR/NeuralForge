"use client";

import type {
  PendingUnsavedChanges,
  UnsavedAction,
} from "@/hooks/useWorkspace";

export interface UnsavedChangesDialogProps {
  pending: PendingUnsavedChanges;
  onResolve: (action: UnsavedAction) => void;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export default function UnsavedChangesDialog({
  pending,
  onResolve,
}: UnsavedChangesDialogProps) {
  const label =
    pending.reason === "switch_workspace"
      ? "switching workspaces"
      : pending.reason === "close_app"
        ? "closing NeuralForge"
        : "closing this file";

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="unsaved-title"
        className="w-full max-w-md rounded-lg border border-neutral-200 bg-white p-5 shadow-2xl dark:border-neutral-700 dark:bg-neutral-900"
      >
        <h2 id="unsaved-title" className="text-sm font-semibold text-neutral-900 dark:text-neutral-100">
          Save changes before {label}?
        </h2>
        <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">
          {pending.paths.length === 1
            ? fileName(pending.paths[0])
            : `${pending.paths.length} files have unsaved changes.`}
        </p>
        {pending.error && (
          <div className="mt-3 rounded border border-red-200 bg-red-50 p-2 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-300">
            {pending.error}
          </div>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => onResolve("cancel")}
            disabled={pending.saving}
            className="rounded px-3 py-1.5 text-xs font-medium text-neutral-600 hover:bg-neutral-100 disabled:opacity-50 dark:text-neutral-300 dark:hover:bg-neutral-800"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onResolve("discard")}
            disabled={pending.saving}
            className="rounded px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 disabled:opacity-50 dark:text-red-400 dark:hover:bg-red-950/40"
          >
            Discard
          </button>
          <button
            type="button"
            onClick={() => onResolve("save")}
            disabled={pending.saving}
            className="rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-500 disabled:opacity-50"
          >
            {pending.saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
