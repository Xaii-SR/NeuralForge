"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import ChatPane from "@/components/ChatPane";

export interface SessionTabsProps { workspaceRoot: string | null; }

const TAB_BUTTON = "group flex shrink-0 items-center gap-1 rounded-t px-2.5 py-1 text-xs font-medium transition-colors border-b-2 max-w-[140px]";
const TAB_ACTIVE = "border-blue-500 bg-neutral-100 text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100";
const TAB_INACTIVE = "border-transparent text-neutral-500 hover:text-neutral-700 hover:bg-neutral-100 dark:text-neutral-500 dark:hover:text-neutral-300 dark:hover:bg-neutral-800";

type TabsState = "uninitialized" | "loading" | "ready" | "failed";

/**
 * Owns session list/selection state (v1.3.0 Phase 4B). This is the single
 * authoritative source of "which session is active" - ChatPane is a pure
 * consumer of the activeSessionId it's handed here, it does not discover
 * or create sessions itself (that logic moved here from ChatPane's old
 * Phase 4A init effect, it was not duplicated).
 */
export default function SessionTabs({ workspaceRoot }: SessionTabsProps) {
  const [sessions, setSessions] = useState<ai.Session[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [tabsState, setTabsState] = useState<TabsState>("uninitialized");
  const [error, setError] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  // Generation-in-progress flag ChatPane reports up, purely so switching/
  // creating/deleting can be disabled while a response is streaming -
  // see the "streaming during session switch" limitation documented below.
  const [sending, setSending] = useState(false);

  // Same StrictMode/re-render duplicate-init guard used by Phase 4A's
  // original effect in ChatPane - claimed synchronously before any await.
  const initializedForWorkspace = useRef<string | null>(null);

  useEffect(() => {
    if (!workspaceRoot) {
      initializedForWorkspace.current = null;
      setSessions([]);
      setActiveSessionId(null);
      setTabsState("uninitialized");
      return;
    }
    if (initializedForWorkspace.current === workspaceRoot) return;
    initializedForWorkspace.current = workspaceRoot;
    setSessions([]);
    setActiveSessionId(null);
    setTabsState("loading");

    (async () => {
      try {
        const list = await ai.listSessions();
        const session = list[0] ?? (await ai.createSession("New Chat"));
        setSessions(list[0] ? list : [session]);
        setActiveSessionId(session.id);
        setTabsState("ready");
      } catch (e) {
        setError(`Could not load saved conversations: ${e}`);
        setTabsState("failed");
      }
    })();
  }, [workspaceRoot]);

  async function handleCreate() {
    if (sending) return;
    try {
      const session = await ai.createSession("New Chat");
      setSessions((prev) => [session, ...prev]);
      setActiveSessionId(session.id);
    } catch (e) {
      setError(`Couldn't create a new session: ${e}`);
    }
  }

  function handleSelect(id: string) {
    if (sending || id === activeSessionId) return;
    setActiveSessionId(id);
  }

  function startRename(session: ai.Session) {
    setRenamingId(session.id);
    setRenameValue(session.title);
  }

  async function commitRename(session: ai.Session) {
    const title = renameValue.trim();
    setRenamingId(null);
    if (!title || title === session.title) return;
    try {
      await ai.updateSessionMetadata(session.id, title, session.last_message_preview ?? "");
      setSessions((prev) => prev.map((s) => (s.id === session.id ? { ...s, title } : s)));
    } catch (e) {
      // Original name stays visible since we never optimistically changed it.
      setError(`Couldn't rename session: ${e}`);
    }
  }

  async function handleDelete(session: ai.Session) {
    if (sending) return;
    const prevSessions = sessions;
    try {
      await ai.deleteSession(session.id);
      const remaining = prevSessions.filter((s) => s.id !== session.id);
      setSessions(remaining);
      if (activeSessionId === session.id) {
        setActiveSessionId(remaining[0]?.id ?? null);
      }
    } catch (e) {
      // Backend delete failed - leave existing UI state exactly as it was.
      setError(`Couldn't delete session: ${e}`);
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-b border-neutral-200 bg-neutral-50 px-2 dark:border-neutral-800 dark:bg-neutral-900">
        {sessions.map((s) => (
          <div key={s.id} className={`${TAB_BUTTON} ${s.id === activeSessionId ? TAB_ACTIVE : TAB_INACTIVE}`}>
            {renamingId === s.id ? (
              <input
                autoFocus
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onFocus={(e) => e.target.select()}
                onBlur={() => commitRename(s)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitRename(s);
                  if (e.key === "Escape") setRenamingId(null);
                }}
                className="w-24 min-w-0 rounded border border-blue-400 bg-white px-1 text-xs text-neutral-800 outline-none dark:bg-neutral-800 dark:text-neutral-100"
              />
            ) : (
              <button
                onClick={() => handleSelect(s.id)}
                onDoubleClick={() => startRename(s)}
                title={s.title}
                className="min-w-0 truncate"
              >
                {s.title}
              </button>
            )}
            <button
              onClick={() => startRename(s)}
              disabled={sending}
              title="Rename chat"
              aria-label={`Rename ${s.title}`}
              className="shrink-0 rounded px-1 text-neutral-400 opacity-0 transition-opacity hover:bg-neutral-200 hover:text-blue-500 group-hover:opacity-100 disabled:opacity-0 dark:hover:bg-neutral-700"
            >
              Edit
            </button>
            <button
              onClick={() => handleDelete(s)}
              disabled={sending}
              title="Delete session"
              aria-label={`Delete ${s.title}`}
              className="shrink-0 rounded px-1 text-neutral-400 opacity-0 transition-opacity hover:bg-neutral-200 hover:text-red-500 group-hover:opacity-100 disabled:opacity-0 dark:hover:bg-neutral-700"
            >
              ×
            </button>
          </div>
        ))}
        <button
          onClick={handleCreate}
          disabled={sending || !workspaceRoot}
          title="New chat"
          aria-label="New chat"
          className="ml-auto shrink-0 rounded px-2 py-1 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-200 hover:text-neutral-700 disabled:opacity-50 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-200"
        >
          + New
        </button>
      </div>
      <div className="min-h-0 flex-1">
        <ChatPane
          workspaceRoot={workspaceRoot}
          activeSessionId={activeSessionId}
          sessionsReady={tabsState === "ready" || tabsState === "failed"}
          externalError={error}
          onDismissExternalError={() => setError(null)}
          onSendingChange={setSending}
        />
      </div>
    </div>
  );
}
