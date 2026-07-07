"use client";

import { useState } from "react";
import EditorPane from "@/components/EditorPane";
import FileExplorer from "@/components/FileExplorer";
import Terminal from "@/components/Terminal";
import LogViewer from "@/components/LogViewer";
import ChatPane from "@/components/ChatPane";
import { useWorkspace } from "@/hooks/useWorkspace";
import { useEvent } from "@/hooks/useEvent";

interface FileChangedPayload {
  path: string;
  kind: string;
}

export default function Home() {
  const workspace = useWorkspace();
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [bottomTab, setBottomTab] = useState<"terminal" | "logs">("terminal");

  useEvent<FileChangedPayload>("FILE_CHANGED", (payload) => {
    setLastEvent(`${payload.kind}: ${payload.path}`);
  });

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
          <div className="flex h-56 shrink-0 flex-col border-t border-neutral-800">
            <div className="flex h-7 shrink-0 gap-1 border-b border-neutral-800 bg-neutral-900 px-2">
              <button
                onClick={() => setBottomTab("terminal")}
                className={`px-2 text-xs ${
                  bottomTab === "terminal" ? "text-neutral-100" : "text-neutral-500"
                }`}
              >
                Terminal
              </button>
              <button
                onClick={() => setBottomTab("logs")}
                className={`px-2 text-xs ${
                  bottomTab === "logs" ? "text-neutral-100" : "text-neutral-500"
                }`}
              >
                Logs
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <div className={bottomTab === "terminal" ? "h-full" : "hidden"}>
                <Terminal />
              </div>
              <div className={bottomTab === "logs" ? "h-full" : "hidden"}>
                <LogViewer />
              </div>
            </div>
          </div>
        </div>
        <div className="w-80 shrink-0 border-l border-neutral-800">
          <ChatPane workspaceOpen={!!workspace.workspaceRoot} />
        </div>
      </div>
      <div className="flex h-6 shrink-0 items-center border-t border-neutral-800 bg-neutral-900 px-3 text-xs text-neutral-500">
        {lastEvent ?? "Ready"}
      </div>
    </main>
  );
}
