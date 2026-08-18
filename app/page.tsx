"use client";

import { useEffect, useRef, useState } from "react";
import EditorPane from "@/components/EditorPane";
import FileExplorer from "@/components/FileExplorer";
import Terminal from "@/components/Terminal";
import LogViewer from "@/components/LogViewer";
import SessionTabs from "@/components/SessionTabs";
import SettingsPanel from "@/components/SettingsPanel";
import AgentPanel from "@/components/AgentPanel";
import CouncilPanel from "@/components/CouncilPanel";
import AITeamPanel from "@/components/AITeamPanel";
import ExtensionsPanel from "@/components/ExtensionsPanel";
import GovernancePanel from "@/components/GovernancePanel";
import WorkersPanel from "@/components/WorkersPanel";
import PromptMaker from "@/components/PromptMaker";
import BootstrapManager from "@/components/BootstrapManager";
import UnsavedChangesDialog from "@/components/UnsavedChangesDialog";
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
  const layoutRootRef = useRef<HTMLDivElement | null>(null);
  const layout = usePanelLayout(layoutRootRef);
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [bottomTab, setBottomTab] = useState<"terminal" | "logs" | "agent" | "extensions" | "governance" | "workers">("terminal");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [promptMakerOpen, setPromptMakerOpen] = useState(false);
  const [councilOpen, setCouncilOpen] = useState(false);
  const [aiTeamOpen, setAiTeamOpen] = useState(false);
  const [selectedContext, setSelectedContext] = useState<string | null>(null);

  useEvent<FileChangedPayload>("FILE_CHANGED", (payload) => { setLastEvent(`${payload.kind}: ${payload.path}`); });

  // Mirrors PromptMaker's own internal Escape handling - CouncilPanel has
  // no onClose/Escape logic of its own (it's reused as-is, unmodified),
  // so the floating wrapper here owns it instead, scoped to councilOpen.
  useEffect(() => {
    if (!councilOpen && !aiTeamOpen) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      setCouncilOpen(false);
      setAiTeamOpen(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [aiTeamOpen, councilOpen]);

  const startSidebarDrag = (event: React.PointerEvent<HTMLDivElement>) => layout.startDrag("sidebar")(event);
  const startBottomDrag = (event: React.PointerEvent<HTMLDivElement>) => layout.startDrag("bottom")(event);
  const startChatDrag = (event: React.PointerEvent<HTMLDivElement>) => layout.startDrag("chat")(event);

  return (
    <main className="flex h-screen w-screen flex-col bg-white text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-3 dark:border-neutral-800 dark:bg-neutral-900">
        <button onClick={workspace.openFolder} className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">Open Folder</button>
        {workspace.workspaceRoot && <span className="truncate text-xs text-neutral-500 dark:text-neutral-500">{workspace.workspaceRoot}</span>}
        <div className="ml-auto flex items-center gap-1">
          <button onClick={() => setPromptMakerOpen(true)} className="mr-1 flex items-center gap-1.5 rounded bg-purple-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-purple-500"><span>🛠️</span><span>Prompt Maker</span></button>
          <button onClick={() => setAiTeamOpen(true)} className="mr-1 flex items-center gap-1.5 rounded bg-green-500 px-2.5 py-1 text-xs font-semibold text-white transition-colors hover:bg-green-400"><span>🤝</span><span>AI Team</span></button>
          <button onClick={() => setCouncilOpen(true)} className="mr-1 flex items-center gap-1.5 rounded bg-red-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-red-500"><span>⚖️</span><span>Council</span></button>
          <button onClick={toggleTheme} aria-label="Toggle theme" title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"} className="rounded px-2 py-1 text-xs text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">{theme === "dark" ? "☀" : "🌙"}</button>
          <button onClick={() => setSettingsOpen(true)} className="rounded px-2.5 py-1 text-xs font-medium text-neutral-700 transition-colors hover:bg-neutral-200 dark:text-neutral-200 dark:hover:bg-neutral-800">Settings</button>
        </div>
      </div>
      {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      {promptMakerOpen && <PromptMaker onClose={() => setPromptMakerOpen(false)} />}
      {aiTeamOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4 backdrop-blur-[1px]" onClick={() => setAiTeamOpen(false)}>
          <div onClick={(e) => e.stopPropagation()} className="flex max-h-[90vh] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
            <div className="mb-4 flex shrink-0 items-center justify-between">
              <h2 className="text-base font-semibold">🤝 AI Team</h2>
              <button onClick={() => setAiTeamOpen(false)} aria-label="Close AI Team" className="rounded px-1.5 py-0.5 text-neutral-400 hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
            </div>
            <div className="min-h-0 flex-1"><AITeamPanel key={workspace.workspaceGeneration} workspaceGeneration={workspace.workspaceGeneration} /></div>
            <div className="mt-4 flex shrink-0 justify-end border-t border-neutral-100 pt-3 dark:border-neutral-800">
              <button onClick={() => setAiTeamOpen(false)} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Close</button>
            </div>
          </div>
        </div>
      )}
      {councilOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={() => setCouncilOpen(false)}>
          <div onClick={(e) => e.stopPropagation()} className="max-h-[85vh] w-[640px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
            <div className="mb-5 flex items-center justify-between">
              <h2 className="text-base font-semibold">⚖️ AI Council</h2>
              <button onClick={() => setCouncilOpen(false)} className="rounded px-1.5 py-0.5 text-neutral-400 hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
            </div>
            <div className="h-[500px]">
              <CouncilPanel key={workspace.workspaceGeneration} workspaceGeneration={workspace.workspaceGeneration} />
            </div>
            <div className="mt-4 flex justify-end border-t border-neutral-100 pt-3 dark:border-neutral-800">
              <button onClick={() => setCouncilOpen(false)} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Close</button>
            </div>
          </div>
        </div>
      )}
      <div ref={layoutRootRef} className="flex min-h-0 flex-1 overflow-hidden">
        <div style={{ width: "var(--nf-sidebar-w, 256px)" }} className="shrink-0 overflow-hidden border-r border-neutral-200 dark:border-neutral-800">
          {workspace.workspaceRoot ? <FileExplorer key={workspace.workspaceRoot} workspaceRoot={workspace.workspaceRoot} onFileClick={workspace.openFile} onContextSelect={setSelectedContext} /> : <EmptyState icon="📁" title="No folder open" hint="Open a folder to browse and edit its files" />}
        </div>
        <ResizeHandle orientation="vertical" label="Resize file explorer" onPointerDown={startSidebarDrag} onDoubleClick={() => layout.resetPanel("sidebar")} onNudge={(d) => layout.nudgePanel("sidebar", d)} />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="min-h-0 min-w-0 flex-1 overflow-hidden"><EditorPane openFiles={workspace.openFiles} activePath={workspace.activePath} onSelect={workspace.setActivePath} onClose={workspace.closeFile} onChange={workspace.updateContent} onSave={workspace.saveFile} onExternalWrite={workspace.acceptExternalWrite} readOnly={workspace.editingLocked} /></div>
          <ResizeHandle orientation="horizontal" label="Resize bottom panel" onPointerDown={startBottomDrag} onDoubleClick={() => layout.resetPanel("bottom")} onNudge={(d) => layout.nudgePanel("bottom", d)} />
          <div style={{ height: "var(--nf-bottom-h, 288px)" }} className="flex min-w-0 shrink-0 flex-col overflow-hidden border-t border-neutral-200 dark:border-neutral-800">
            <div className="flex h-9 shrink-0 gap-1 border-b border-neutral-200 bg-neutral-50 px-2 dark:border-neutral-800 dark:bg-neutral-900">
              {(["terminal","logs","agent","extensions","governance","workers"] as const).map((t) => (
                <button key={t} onClick={() => setBottomTab(t)} className={`${TAB_BUTTON} ${bottomTab === t ? TAB_ACTIVE : TAB_INACTIVE}`}>{t === "terminal" ? "Terminal" : t === "logs" ? "Logs" : t === "agent" ? "Agent" : t === "extensions" ? "Extensions" : t === "governance" ? "Governance" : "Workers"}</button>
              ))}
            </div>
            <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
              {bottomTab === "terminal" && <div className="h-full min-w-0 overflow-hidden"><Terminal /></div>}
              {bottomTab === "logs" && <div className="h-full min-w-0 overflow-hidden"><LogViewer /></div>}
              {bottomTab === "agent" && <div className="h-full min-w-0 overflow-hidden"><AgentPanel key={workspace.workspaceGeneration} workspaceOpen={!!workspace.workspaceRoot} workspaceGeneration={workspace.workspaceGeneration} /></div>}
              {bottomTab === "extensions" && <div className="h-full min-w-0 overflow-hidden"><ExtensionsPanel /></div>}
              {bottomTab === "governance" && <div className="h-full min-w-0 overflow-hidden"><GovernancePanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
              {bottomTab === "workers" && <div className="h-full min-w-0 overflow-hidden"><WorkersPanel workspaceOpen={!!workspace.workspaceRoot} /></div>}
            </div>
          </div>
        </div>
        <ResizeHandle orientation="vertical" label="Resize chat panel" onPointerDown={startChatDrag} onDoubleClick={() => layout.resetPanel("chat")} onNudge={(d) => layout.nudgePanel("chat", -d)} />
        <div style={{ width: "var(--nf-chat-w, 380px)" }} className="min-w-0 shrink-0 overflow-hidden border-l border-neutral-200 dark:border-neutral-800">
          <SessionTabs key={workspace.workspaceGeneration} workspaceRoot={workspace.workspaceRoot} workspaceGeneration={workspace.workspaceGeneration} selectedContext={selectedContext} />
        </div>
      </div>
      <div className="flex h-6 shrink-0 items-center border-t border-neutral-200 bg-neutral-50 px-3 text-xs text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-500">{lastEvent ?? "Ready"}</div>
      <BootstrapManager />
      {workspace.pendingUnsaved && (
        <UnsavedChangesDialog pending={workspace.pendingUnsaved} onResolve={workspace.resolveUnsaved} />
      )}
    </main>
  );
}
