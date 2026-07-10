"use client";

import { useState } from "react";
import EditorPane from "@/components/EditorPane";
import FileExplorer from "@/components/FileExplorer";
import Terminal from "@/components/Terminal";
import LogViewer from "@/components/LogViewer";
import ChatPane from "@/components/ChatPane";
import SettingsPanel from "@/components/SettingsPanel";
import AgentPanel from "@/components/AgentPanel";
import ExtensionsPanel from "@/components/ExtensionsPanel";
import BootstrapPanel from "@/components/BootstrapPanel";
import GovernancePanel from "@/components/GovernancePanel";
import WorkersPanel from "@/components/WorkersPanel";
import EmptyState from "@/components/ui/EmptyState";
import { useWorkspace } from "@/hooks/useWorkspace";
import { useEvent } from "@/hooks/useEvent";
import { useTheme } from "@/hooks/useTheme";

interface FileChangedPayload {
  path: string;
  kind: string;
}

const TAB_BUTTON = "px-3 py-1.5 text-xs font-medium transition-colors border-b-2";
const TAB_ACTIVE = "border-blue-500 text-neutral-900 dark:text-neutral-100";
const TAB_INACTIVE = "border-transparent text-neutral-500 hover:text-neutral-700 dark:text-neutral-500 dark:hover:text-neutral-300";

export default function Home() {
  const workspace = useWorkspace();
  const { theme, toggleTheme } = useTheme();
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [bottomTab, setBottomTab] = useState<"terminal" | "logs" | "agent" | "extensions" | "bootstrap" | "governance" | "workers">("terminal");
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEvent<FileChangedPayload>("FILE_CHANGED", (payload) => {
    setLastEvent(`${payload.kind}: ${payload.path}`);
  });

  return (
    <main className="flex h-screen w-screen flex-col bg-white text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-3 dark:border-neutral-800 dark:bg-neutral-900">
        <button
          onClick={workspace.openFolder}
          className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800"
        >
          Open Folder
        </button>
        {workspace.workspaceRoot && (
          <span className="truncate text-xs text-neutral-500 dark:text-neutral-500">{workspace.workspaceRoot}</span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={toggleTheme}
            aria-label="Toggle theme"
            title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
            className="rounded px-2 py-1 text-xs text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800"
          >
            {theme === "dark" ? "☀" : "🌙"}
          </button>
          <button
            onClick={() => setSettingsOpen(true)}
            className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800"
          >
            Settings
          </button>
        </div>
      </div>
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      <div className="flex min-h-0 flex-1">
        <div className="w-64 shrink-0 border-r border-neutral-200 dark:border-neutral-800">
          {workspace.workspaceRoot ? (
            <FileExplorer workspaceRoot={workspace.workspaceRoot} onFileClick={workspace.openFile} />
          ) : (
            <EmptyState icon="📁" title="No folder open" hint="Open a folder to browse and edit its files" />
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
          <div className="flex h-72 shrink-0 flex-col border-t border-neutral-200 dark:border-neutral-800">
            <div className="flex h-9 shrink-0 gap-1 border-b border-neutral-200 bg-neutral-50 px-2 dark:border-neutral-800 dark:bg-neutral-900">
              <button
                onClick={() => setBottomTab("terminal")}
                className={`${TAB_BUTTON} ${bottomTab === "terminal" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Terminal
              </button>
              <button
                onClick={() => setBottomTab("logs")}
                className={`${TAB_BUTTON} ${bottomTab === "logs" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Logs
              </button>
              <button
                onClick={() => setBottomTab("agent")}
                className={`${TAB_BUTTON} ${bottomTab === "agent" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Agent
              </button>
              <button
                onClick={() => setBottomTab("extensions")}
                className={`${TAB_BUTTON} ${bottomTab === "extensions" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Extensions
              </button>
              <button
                onClick={() => setBottomTab("bootstrap")}
                className={`${TAB_BUTTON} ${bottomTab === "bootstrap" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Bootstrap
              </button>
              <button
                onClick={() => setBottomTab("governance")}
                className={`${TAB_BUTTON} ${bottomTab === "governance" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Governance
              </button>
              <button
                onClick={() => setBottomTab("workers")}
                className={`${TAB_BUTTON} ${bottomTab === "workers" ? TAB_ACTIVE : TAB_INACTIVE}`}
              >
                Workers
              </button>
            </div>
            <div className="min-h-0 flex-1">
              <div className={bottomTab === "terminal" ? "h-full" : "hidden"}>
                <Terminal />
              </div>
              <div className={bottomTab === "logs" ? "h-full" : "hidden"}>
                <LogViewer />
              </div>
              <div className={bottomTab === "agent" ? "h-full" : "hidden"}>
                <AgentPanel workspaceOpen={!!workspace.workspaceRoot} />
              </div>
              <div className={bottomTab === "extensions" ? "h-full" : "hidden"}>
                <ExtensionsPanel />
              </div>
              <div className={bottomTab === "bootstrap" ? "h-full" : "hidden"}>
                <BootstrapPanel workspaceOpen={!!workspace.workspaceRoot} />
              </div>
              <div className={bottomTab === "governance" ? "h-full" : "hidden"}>
                <GovernancePanel workspaceOpen={!!workspace.workspaceRoot} />
              </div>
              <div className={bottomTab === "workers" ? "h-full" : "hidden"}>
                <WorkersPanel workspaceOpen={!!workspace.workspaceRoot} />
              </div>
            </div>
          </div>
        </div>
        <div className="w-80 shrink-0 border-l border-neutral-200 dark:border-neutral-800">
          <ChatPane workspaceOpen={!!workspace.workspaceRoot} />
        </div>
      </div>
      <div className="flex h-6 shrink-0 items-center border-t border-neutral-200 bg-neutral-50 px-3 text-xs text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-500">
        {lastEvent ?? "Ready"}
      </div>
    </main>
  );
}
