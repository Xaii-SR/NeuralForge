"use client";

import { useEffect, useState } from "react";

export interface PromptMakerProps { onClose: () => void; }

export default function PromptMaker({ onClose }: PromptMakerProps) {
  const [systemRole, setSystemRole] = useState("");
  const [constraints, setConstraints] = useState("");
  const [outputFormat, setOutputFormat] = useState("");

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) { if (e.key === "Escape") onClose(); }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()} className="max-h-[80vh] w-[500px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">🛠️ Prompt Maker</h2>
          <button onClick={onClose} className="rounded px-1.5 py-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
        </div>

        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">System Persona / Role</label>
            <textarea value={systemRole} onChange={(e) => setSystemRole(e.target.value)} rows={3} placeholder="You are an expert Rust developer..." className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">Custom Constraints</label>
            <textarea value={constraints} onChange={(e) => setConstraints(e.target.value)} rows={3} placeholder="Do not use unsafe blocks. Prefer async/await." className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">Output Formatting</label>
            <textarea value={outputFormat} onChange={(e) => setOutputFormat(e.target.value)} rows={3} placeholder={`Output code wrapped in <write_file path="..."> tags.`} className="w-full resize-none rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>
        </div>

        <div className="mt-5 flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800">
          <button onClick={onClose} className="rounded px-3 py-1.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Cancel</button>
          <button onClick={onClose} className="rounded bg-purple-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-purple-500">Save Prompt Template</button>
        </div>
      </div>
    </div>
  );
}