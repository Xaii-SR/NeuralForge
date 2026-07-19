"use client";

import { useState } from "react";
import EditorPane from "@/components/EditorPane";
import FileExplorer from "@/components/FileExplorer";
import Terminal from "@/components/Terminal";
import LogViewer from "@/components/LogViewer";
import ChatPane from "@/components/ChatPane";
import SettingsPanel from "@/components/SettingsPanel";
import AgentPanel from "@/components/AgentPanel";
import AgentWorkbench from "@/components/AgentWorkbench";
import CouncilPanel from "@/components/CouncilPanel";
import ExtensionsPanel from "@/components/ExtensionsPanel";
import BootstrapPanel from "@/components/BootstrapPanel";
import GovernancePanel from "@/components/GovernancePanel";
import WorkersPanel from "@/components/WorkersPanel";
import PromptMaker from "@/components/PromptMaker";
import BootstrapManager from "@/components/BootstrapManager";
import EmptyState from "@/components/ui/EmptyState";
import ResizeHandle from "@/components/ui/ResizeHandle";
import { useWorkspace } from "@/hooks/useWorkspace";
import { useEvent } from "@/hooks/useEvent";
import { useTheme } from "@/hooks/useTheme";
import { usePanelLayout } from "@/hooks/usePanelLayout";

interface FileChangedPayload { path: string; kind: string; }

const TAB_BUTTON = "px-3 py-1.5 text-xs font-medium transition-colors border-b-2";
const TAB_ACTIVE = "border-blue-500 text-neutral-900 dark:text-neutral-100";
const TAB_INACTIVE = "border-transparent text-neutral-500 hover:text-neutral-700 dark:text-neutral-500 dark:hover:text-neutral-300";

export default function Home() {
  const workspace = useWorkspace();
  const { theme, toggleTheme } = useTheme();
  const layout = usePanelLayout();
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [bottomTab, setBottomTab] = useState<"terminal" | "logs" | "agent" | "workbench" | "council" | "extensions" | "bootstrap" | "governance" | "workers">("terminal");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [promptMakerOpen, setPromptMakerOpen] = useState(false);

  useEvent<FileChangedPayload>("FILE_CHANGED", (payload) => { setLastEvent(`${payload.kind}: ${payload.path}`); });

  return (
    <main className="flex h-screen w-screen flex-col bg-white text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-3 dark:border-neutral-800 dark:bg-neutral-900">
        <button onClick={workspace.openFolder} className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">Open Folder</button>
        {workspace.workspaceRoot && <span className="truncate text-xs text-neutral-500 dark:text-neutral-500">{workspace.workspaceRoot}</span>}
        <div className="ml-auto flex items-center gap-1">
          <button onClick={() => setPromptMakerOpen(true)} className="mr-1 flex items-center gap-1.5 rounded bg-purple-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-purple-500"><span>🛠️</span><span>Prompt Maker</span></button>
          <button onClick={toggleTheme} aria-label="Toggle theme" title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"} className="rounded px-2 py-1 text-xs text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">{theme === "dark" ? "☀" : "🌙"}</button>
          <button onClick={() => setSettingsOpen(true)} className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">Settings</button>
        </div>
      </div>
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      {promptMakerOpen && <PromptMaker onClose={() => setPromptMakerOpen(false)} />}
      <div ref={layout.rootRef} className="flex min-h-0 flex-1">
        <div style={{ width: "var(--nf-sidebar-w, 256px)" }} className="shrink-0 border-r border-neutral-200 dark:border-neutral-800">
          {workspace.workspaceRoot ? <FileExplorer workspaceRoot={workspace.workspaceRoot} onFileClick={workspace.openFile} /> : <EmptyState icon="📁" title="No folder open" hint="Open a folder to browse and edit its files" />}
        </div>
        <ResizeHandle orientation="vertical" label="Resize file explorer" onPointerDown={layout.startDrag("sidebar")} onDoubleClick={() => layout.resetPanel("sidebar")} onNudge={(d) => layout.nudgePanel("sidebar", d)} />
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1"><EditorPane openFiles={workspace.openFiles} activePath={workspace.activePath} onSelect={workspace.setActivePath} onClose={workspace.closeFile} onChange={workspace.updateContent} onSave={workspace.saveFile} /></div>
          <ResizeHandle orientation="horizontal" label="Resize bottom panel" onPointerDown={layout.startDrag("bottom")} onDoubleClick={() => layout.resetPanel("bottom")} onNudge={(d) => layout.nudgePanel("bottom", d)} />
          <div style={{ height: "var(--nf-bottom-h, 288px)" }} className="flex shrink-0 flex-col border-t border-neutral-200 dark:border-neutral-800">
            <div className="flex h-9 shrink-0 gap-1 border-b border-neutral-200 bg-neutral-50 px-2 dark:border-neutral-800 dark:bg-neutral-900">
              {(["terminal","logs","agent","workbench","council","extensions","bootstrap","governance","workers"] as const).map((t) => (
                <button key={t} onClick={() => setBottomTab(t)} className={`${TAB_BUTTON} ${bottomTab === t ? TAB_ACTIVE : TAB_INACTIVE}`}>{t === "terminal" ? "Terminal" : t === "logs" ? "Logs" : t === "agent" ? "Agent" : t === "workbench" ? "Workbench" : t === "council" ? "Council" : t === "extensions" ? "Extensions" : t === "bootstrap" ? "Bootstrap" : t === "governance" ? "Governance" : "Workers"}</button>
              ))}
            </div>
            <div className="min-h-0 flex-1">
              {bottomTab === "terminal" && <div className="h-full"><Terminal /></div>}
              {bottomTab === "logs" && <div className="h-full"><LogViewer /></div>}
              {bottomTab === "agent" && <div className="h-full"><AgentPanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
              {bottomTab === "workbench" && <div className="h-full"><AgentWorkbench /></div>}
              {bottomTab === "council" && <div className="h-full"><CouncilPanel /></div>}
              {bottomTab === "extensions" && <div className="h-full"><ExtensionsPanel /></div>}
              {bottomTab === "bootstrap" && <div className="h-full"><BootstrapPanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
              {bottomTab === "governance" && <div className="h-full"><GovernancePanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
              {bottomTab === "workers" && <div className="h-full"><WorkersPanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
            </div>
          </div>
        </div>
        <ResizeHandle orientation="vertical" label="Resize chat panel" onPointerDown={layout.startDrag("chat")} onDoubleClick={() => layout.resetPanel("chat")} onNudge={(d) => layout.nudgePanel("chat", -d)} />
        <div style={{ width: "var(--nf-chat-w, 320px)" }} className="shrink-0 border-l border-neutral-200 dark:border-neutral-800">
          <ChatPane workspaceOpen={!!workspace.workspaceRoot} />
        </div>
      </div>
      <div className="flex h-6 shrink-0 items-center border-t border-neutral-200 bg-neutral-50 px-3 text-xs text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-500">{lastEvent ?? "Ready"}</div>
      <BootstrapManager />
    </main>
  );
}