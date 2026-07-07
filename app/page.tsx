"use client";

import EditorPane from "@/components/EditorPane";
import FileExplorer from "@/components/FileExplorer";
import Terminal from "@/components/Terminal";
import { useWorkspace } from "@/hooks/useWorkspace";

export default function Home() {
  const workspace = useWorkspace();

  return (
    <main className="flex h-screen w-screen flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-neutral-800 bg-neutral-900 px-3">
        <button
          onClick={workspace.openFolder}
          className="rounded bg-neutral-800 px-2 py-1 text-xs text-neutral-200 hover:bg-neutral-700"
        >
          Open Folder
        </button>
        {workspace.workspaceRoot && (
          <span className="truncate text-xs text-neutral-500">{workspace.workspaceRoot}</span>
        )}
      </div>
      <div className="flex min-h-0 flex-1">
        <div className="w-64 shrink-0 border-r border-neutral-800">
          {workspace.workspaceRoot ? (
            <FileExplorer workspaceRoot={workspace.workspaceRoot} onFileClick={workspace.openFile} />
          ) : (
            <div className="p-3 text-xs text-neutral-500">No folder open</div>
          )}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-[2]">
            <EditorPane
              openFiles={workspace.openFiles}
              activePath={workspace.activePath}
              onSelect={workspace.setActivePath}
              onClose={workspace.closeFile}
              onChange={workspace.updateContent}
              onSave={workspace.saveFile}
            />
          </div>
          <div className="h-56 shrink-0 border-t border-neutral-800">
            <Terminal />
          </div>
        </div>
      </div>
    </main>
  );
}
