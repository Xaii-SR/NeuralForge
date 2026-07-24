"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import { useEvent } from "@/hooks/useEvent";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";
import AutoResizeTextarea from "@/components/ui/AutoResizeTextarea";
import CopyButton from "@/components/ui/CopyButton";

interface DisplayMessage { role: "user" | "assistant"; content: string; fromCache?: boolean; timestamp: number; }
interface TokenPayload { request_id: string; token: string; done: boolean; from_cache?: boolean; }
type SessionState = "uninitialized" | "loading" | "ready" | "failed";

export interface ChatPaneProps {
  workspaceRoot: string | null;
  // v1.3.0 Phase 4B: session selection now lives in SessionTabs, which is
  // this component's only caller. ChatPane consumes the active session id
  // and messages for it - it does not discover or create sessions itself.
  activeSessionId: string | null;
  // True once SessionTabs has finished its own init attempt (ready or
  // failed) - lets ChatPane tell "no session yet, still loading the tab
  // strip" apart from "tab strip settled and there's genuinely none".
  sessionsReady: boolean;
  externalError: string | null;
  onDismissExternalError: () => void;
  onSendingChange: (sending: boolean) => void;
}

function formatTime(ts: number): string { return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }); }

function workspaceName(workspaceRoot: string | null): string | null {
  if (!workspaceRoot) return null;
  return workspaceRoot.split(/[\\/]/).filter(Boolean).pop() ?? workspaceRoot;
}

export default function ChatPane({ workspaceRoot, activeSessionId, sessionsReady, externalError, onDismissExternalError, onSendingChange }: ChatPaneProps) {
  const workspaceOpen = !!workspaceRoot;
  const connectedWorkspace = workspaceName(workspaceRoot);
  const [ollamaAvailable, setOllamaAvailable] = useState<boolean | null>(null);
  const [models, setModels] = useState<ai.OllamaModel[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>("");
  const [autoMode, setAutoMode] = useState(true);
  const [autoSelection, setAutoSelection] = useState<ai.AutoSelection | null>(null);
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [indexStatus, setIndexStatus] = useState<string | null>(null);
  const [sessionState, setSessionState] = useState<SessionState>("uninitialized");
  const activeRequestId = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Mirrors the activeSessionId prop into a ref: useEvent (see hooks/useEvent.ts)
  // subscribes once and keeps whatever closure it captured on that first
  // subscription, so anything read inside the AI_RESPONSE_TOKEN handler
  // below must come from a ref, never a prop/state value directly, or it
  // would keep targeting whichever session was active on first mount.
  const activeSessionIdRef = useRef<string | null>(activeSessionId);
  useEffect(() => { activeSessionIdRef.current = activeSessionId; }, [activeSessionId]);
  // Accumulates the current assistant response outside React state so the
  // exact final text is available synchronously when payload.done fires,
  // without depending on a possibly-stale `messages` closure.
  const streamingContentRef = useRef<string>("");
  // Ensures assistant-completion persistence happens exactly once per
  // request, even if AI_RESPONSE_TOKEN's done:true is (re)delivered more
  // than once for the same request_id.
  const persistedRequestIds = useRef<Set<string>>(new Set());

  async function handleIndex() { setIndexing(true); setIndexStatus(null); try { const s = await ai.indexWorkspace(); setIndexStatus(`Indexed ${s.files_indexed} files (${s.chunks_created} chunks)`); } catch (e) { setIndexStatus(`Index failed: ${e}`); } finally { setIndexing(false); } }

  useEffect(() => { ai.ollamaHealthCheck().then(async (healthy) => { setOllamaAvailable(healthy); if (healthy) { const l = await ai.listModels(); setModels(l); if (l.length > 0) setSelectedModel(l[0].name); } }); }, []);

  useEffect(() => { onSendingChange(sending); }, [sending, onSendingChange]);

  // Message loading: keyed on activeSessionId, which SessionTabs owns and
  // controls entirely (init, switch, create, delete-with-replacement all
  // funnel through the same prop). No independent session discovery here.
  useEffect(() => {
    persistedRequestIds.current.clear();
    if (!activeSessionId) {
      setMessages([]);
      setSessionState(sessionsReady ? "ready" : workspaceOpen ? "loading" : "uninitialized");
      return;
    }
    setSessionState("loading");
    let cancelled = false;
    (async () => {
      try {
        const history = await ai.getSessionMessages(activeSessionId);
        if (cancelled) return;
        setMessages(
          history.map((m) => ({
            role: m.role === "user" ? "user" : "assistant",
            content: m.content,
            timestamp: m.timestamp * 1000,
          }))
        );
        setSessionState("ready");
      } catch (e) {
        if (cancelled) return;
        setError(`Could not load saved conversations: ${e}`);
        setSessionState("failed");
      }
    })();
    return () => { cancelled = true; };
  }, [activeSessionId, sessionsReady, workspaceOpen]);

  useEvent<TokenPayload>("AI_RESPONSE_TOKEN", (payload) => {
    if (payload.request_id !== activeRequestId.current) return;
    streamingContentRef.current += payload.token;
    setMessages((prev) => { const n = [...prev]; const last = n[n.length - 1]; if (last && last.role === "assistant") n[n.length - 1] = { ...last, content: last.content + payload.token, fromCache: payload.from_cache }; else n.push({ role: "assistant", content: payload.token, fromCache: payload.from_cache, timestamp: Date.now() }); return n; });
    if (payload.done) {
      setSending(false);
      const finishedRequestId = payload.request_id;
      const finalContent = streamingContentRef.current;
      activeRequestId.current = null;
      streamingContentRef.current = "";
      const sid = activeSessionIdRef.current;
      if (sid && finalContent && !persistedRequestIds.current.has(finishedRequestId)) {
        persistedRequestIds.current.add(finishedRequestId);
        ai.appendSessionMessage(sid, "assistant", finalContent, "complete").catch((e) => {
          console.error("Failed to persist assistant message", e);
          setError((prev) => prev ?? `Response wasn't saved: ${e}`);
        });
      }
    }
  });
  useEffect(() => { scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" }); }, [messages]);

  function cancelGeneration() {
    // No persistence call here on purpose: an incomplete assistant
    // response must never be saved (see Phase 4A cancellation rules).
    activeRequestId.current = null;
    streamingContentRef.current = "";
    setSending(false);
    setError("Generation cancelled.");
  }

  async function handleSend() {
    if (!input.trim() || sending || !activeSessionId) return;
    setError(null);
    const rid = crypto.randomUUID();
    activeRequestId.current = rid;
    streamingContentRef.current = "";
    const um: DisplayMessage = { role: "user", content: input, timestamp: Date.now() };
    const nm = [...messages, um];
    setMessages(nm);
    setInput("");
    setSending(true);

    // Persist the user message before kicking off generation, so storage
    // ordering (USER then ASSISTANT) holds even though assistant
    // persistence happens later, asynchronously, on payload.done.
    // SessionTabs disables switching/deleting while sending is true, so
    // activeSessionId is guaranteed stable for the lifetime of this send -
    // see the streaming/session-switch limitation note near onSendingChange.
    const sid = activeSessionId;
    {
      try { await ai.appendSessionMessage(sid, "user", um.content, "complete"); }
      catch (e) { console.error("Failed to persist user message", e); setError(`Message wasn't saved: ${e}`); }
    }

    let mtu = selectedModel;
    if (autoMode) {
      try { const sel = await ai.autoSelectModel(um.content); setAutoSelection(sel); mtu = sel.model; }
      catch (e) { setError(String(e)); setSending(false); activeRequestId.current = null; return; }
    } else { setAutoSelection(null); }
    if (!mtu) { setError("No model available"); setSending(false); activeRequestId.current = null; return; }
    let cp: string | null = null;
    try { cp = await ai.getContextForQuery(um.content); } catch { cp = null; }
    const out: ai.ChatMessage[] = [];
    if (cp) out.push({ role: "system", content: cp });
    out.push(...nm.map((m) => ({ role: m.role, content: m.content })));
    try { await ai.chatWithModel(rid, mtu, out); }
    catch (e) { setError(String(e)); setSending(false); activeRequestId.current = null; }
  }

  if (ollamaAvailable === null) return <div className="flex h-full items-center justify-center gap-2 text-xs text-neutral-500"><Spinner size={12} />Checking Ollama...</div>;
  if (!ollamaAvailable) return <EmptyState icon="🔌" title="Ollama not detected" hint="Install Ollama and make sure it's running at localhost:11434, then reopen NeuralForge." />;
  if (workspaceOpen && sessionState === "loading") return <div className="flex h-full items-center justify-center gap-2 text-xs text-neutral-500"><Spinner size={12} />Loading conversation...</div>;
  // Empty session state (v1.3.0 Phase 4B): reachable after deleting the
  // last session in a workspace. SessionTabs' "+ New" stays enabled here -
  // this is just the message pane telling the user there's nothing active
  // to send into yet.
  if (workspaceOpen && sessionsReady && !activeSessionId) return <EmptyState icon="💬" title="No active session" hint={'Click "+ New" above to start a conversation'} />;

  return (
    <div className="flex h-full flex-col bg-white dark:bg-neutral-900">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-neutral-200 px-2 dark:border-neutral-800">
        <button onClick={() => { setAutoMode((v) => !v); }} className={`rounded px-2 py-1 text-xs font-medium transition-colors ${autoMode ? "bg-blue-600 text-white hover:bg-blue-500" : "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"}`}>Auto</button>
        {!autoMode && (
          <select value={selectedModel} onChange={(e) => setSelectedModel(e.target.value)} className="rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
            {models.map((m) => (<option key={m.name} value={m.name}>{m.name} ({m.parameter_size})</option>))}
          </select>
        )}
        {connectedWorkspace && (
          <div title={workspaceRoot ?? undefined} className="ml-auto max-w-[220px] truncate rounded bg-neutral-100 px-2 py-1 text-[10px] font-medium text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
            Workspace: {connectedWorkspace}
          </div>
        )}
        {workspaceOpen && <button onClick={handleIndex} disabled={indexing} className="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-neutral-600 transition-colors hover:bg-neutral-100 disabled:opacity-60 dark:text-neutral-300 dark:hover:bg-neutral-800">{indexing && <Spinner size={10} />}{indexing ? "Indexing..." : "Index Workspace"}</button>}
      </div>
      {indexStatus && <div className="border-b border-neutral-200 px-2 py-1 text-[10px] text-neutral-500 dark:border-neutral-800 dark:text-neutral-500">{indexStatus}</div>}
      {autoMode && autoSelection && (<div className="border-b border-neutral-200 bg-neutral-50 px-2 py-1.5 text-[10px] text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-400">Selected <span className="font-medium text-neutral-800 dark:text-neutral-200">{autoSelection.model}</span> from {autoSelection.provider} because {autoSelection.reason}. {autoSelection.is_free ? <span className="font-medium text-green-600 dark:text-green-400">Free</span> : <span className="font-medium text-yellow-600 dark:text-yellow-400">~${autoSelection.estimated_cost_usd.toFixed(4)}</span>}</div>)}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {messages.length === 0 && <EmptyState icon="💬" title="Ask NeuralForge anything" hint="Questions about your code get workspace context automatically" />}
        {messages.map((m, i) => { const iu = m.role === "user"; const isStreamingNow = sending && i === messages.length - 1; const complete = !iu && m.content && !isStreamingNow; return (<div key={i} className={`mb-3 flex ${iu ? "justify-end" : "justify-start"}`}><div className={`max-w-[85%] ${iu ? "items-end" : "items-start"} flex flex-col gap-1`}><div className={`group relative rounded-lg px-3 py-2 text-sm leading-relaxed shadow-sm ${iu ? "bg-blue-600 text-white" : "bg-neutral-100 text-neutral-800 dark:bg-neutral-800 dark:text-neutral-100"}`}><div className="whitespace-pre-wrap">{m.content || (isStreamingNow ? "…" : "")}</div>{complete && <CopyButton text={m.content} className="absolute right-1 top-1 opacity-0 group-hover:opacity-100" />}</div><div className="flex items-center gap-1.5 px-1 text-[10px] text-neutral-400 dark:text-neutral-600"><span>{formatTime(m.timestamp)}</span>{m.fromCache && <span className="font-medium text-yellow-600 dark:text-yellow-500">from cache</span>}</div></div></div>); })}
        {externalError && <ErrorBanner message={externalError} onDismiss={onDismissExternalError} />}
        {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
      </div>
      <div className="flex shrink-0 items-end gap-2 border-t border-neutral-200 p-2 dark:border-neutral-800">
        <AutoResizeTextarea value={input} onChange={(e) => setInput(e.target.value)} onSubmit={handleSend} placeholder="Ask a question... (Shift+Enter for a new line)" className="min-w-0 flex-1 resize-none rounded border border-neutral-200 bg-white px-2.5 py-1.5 text-sm text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
        {sending ? (
          <button onClick={cancelGeneration} className="flex shrink-0 items-center gap-1.5 rounded bg-red-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-red-500">⏹ Stop</button>
        ) : (
          <button onClick={handleSend} disabled={(!autoMode && !selectedModel) || !activeSessionId} className="flex shrink-0 items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50">{sending && <Spinner size={10} />}Send</button>
        )}
      </div>
    </div>
  );
}
