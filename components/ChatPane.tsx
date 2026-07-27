"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import { useEvent } from "@/hooks/useEvent";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";
import AutoResizeTextarea from "@/components/ui/AutoResizeTextarea";
import CopyButton from "@/components/ui/CopyButton";
import { getAppConfig } from "@/lib/store";
import { getModelConfig, setDefaultModel } from "@/lib/providers";

interface DisplayMessage { role: "user" | "assistant"; content: string; fromCache?: boolean; timestamp: number; }
interface TokenPayload {
  request_id: string;
  token: string;
  done: boolean;
  from_cache?: boolean;
  status?: "success" | "cancelled" | "error";
  error?: string | null;
}
type SessionState = "uninitialized" | "loading" | "ready" | "failed";

export interface ChatPaneProps {
  workspaceRoot: string | null;
  workspaceGeneration: number;
  selectedContext?: string | null;
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

export default function ChatPane({ workspaceRoot, workspaceGeneration, selectedContext, activeSessionId, sessionsReady, externalError, onDismissExternalError, onSendingChange }: ChatPaneProps) {
  const workspaceOpen = !!workspaceRoot;
  const connectedWorkspace = workspaceName(workspaceRoot);
  const [liveSelectedContext, setLiveSelectedContext] = useState<string | null>(selectedContext ?? null);
  const [models, setModels] = useState<ai.ChatModelDescriptor[]>([]);
  const [selectedModelKey, setSelectedModelKey] = useState("");
  const [modelsLoading, setModelsLoading] = useState(true);
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
  const workspaceGenerationRef = useRef(workspaceGeneration);
  useEffect(() => {
    if (workspaceGenerationRef.current === workspaceGeneration) return;
    workspaceGenerationRef.current = workspaceGeneration;
    activeRequestId.current = null;
    streamingContentRef.current = "";
    persistedRequestIds.current.clear();
    setSending(false);
    setMessages([]);
    setSessionState(workspaceRoot ? "loading" : "uninitialized");
  }, [workspaceGeneration, workspaceRoot]);
  useEffect(() => { setLiveSelectedContext(selectedContext ?? null); }, [selectedContext]);
  useEffect(() => {
    const onContextSelected = (event: Event) => setLiveSelectedContext((event as CustomEvent<string>).detail);
    window.addEventListener("nf_context_selected", onContextSelected);
    return () => window.removeEventListener("nf_context_selected", onContextSelected);
  }, []);

  async function handleIndex() { setIndexing(true); setIndexStatus(null); try { const s = await ai.indexWorkspace(); setIndexStatus(`Indexed ${s.files_indexed} files (${s.chunks_created} chunks)`); } catch (e) { setIndexStatus(`Index failed: ${e}`); } finally { setIndexing(false); } }

  useEffect(() => {
    let cancelled = false;
    async function loadModels() {
      setModelsLoading(true);
      try {
        const [available, assignment] = await Promise.all([
          ai.listChatModels(),
          getModelConfig("active_model_chat").catch(() => null),
        ]);
        if (cancelled) return;
        setModels(available);
        const assigned = assignment
          ? available.find(
              (model) =>
                model.provider_id === assignment.provider_id
                && model.model_id === assignment.model,
            )
          : null;
        const selected = assigned ?? available[0];
        setSelectedModelKey(
          selected ? `${selected.provider_id}\u0000${selected.model_id}` : "",
        );
      } catch (e) {
        if (!cancelled) {
          setModels([]);
          setSelectedModelKey("");
          setError(`Could not load configured chat models: ${e}`);
        }
      } finally {
        if (!cancelled) setModelsLoading(false);
      }
    }
    void loadModels();
    window.addEventListener("nf_settings_updated", loadModels);
    return () => {
      cancelled = true;
      window.removeEventListener("nf_settings_updated", loadModels);
    };
  }, [workspaceGeneration]);

  useEffect(() => { onSendingChange(sending); }, [sending, onSendingChange]);

  async function handleModelChange(key: string) {
    setSelectedModelKey(key);
    const selected = models.find(
      (model) => `${model.provider_id}\u0000${model.model_id}` === key,
    );
    if (!selected) return;
    try {
      await setDefaultModel(
        "active_model_chat",
        selected.provider_id,
        selected.provider_name,
        selected.model_id,
      );
    } catch (e) {
      setError(`Could not save the Chat model assignment: ${e}`);
    }
  }

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
        const generation = workspaceGeneration;
        const history = await ai.getSessionMessages(generation, activeSessionId);
        if (cancelled || workspaceGenerationRef.current !== generation) return;
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
  }, [activeSessionId, sessionsReady, workspaceOpen, workspaceGeneration]);

  useEvent<TokenPayload>("AI_RESPONSE_TOKEN", (payload) => {
    if (payload.request_id !== activeRequestId.current) return;
    if (payload.token) {
      streamingContentRef.current += payload.token;
      setMessages((prev) => { const n = [...prev]; const last = n[n.length - 1]; if (last && last.role === "assistant") n[n.length - 1] = { ...last, content: last.content + payload.token, fromCache: payload.from_cache }; else n.push({ role: "assistant", content: payload.token, fromCache: payload.from_cache, timestamp: Date.now() }); return n; });
    }
    if (payload.done) {
      setSending(false);
      const finishedRequestId = payload.request_id;
      const finalContent = streamingContentRef.current;
      activeRequestId.current = null;
      streamingContentRef.current = "";
      const sid = activeSessionIdRef.current;
      const generation = workspaceGenerationRef.current;
      if (payload.status === "error") setError(payload.error ?? "Generation failed.");
      if (payload.status === "cancelled") setError("Generation cancelled.");
      if (payload.status === "success" && sid && finalContent && !persistedRequestIds.current.has(finishedRequestId)) {
        persistedRequestIds.current.add(finishedRequestId);
        ai.appendSessionMessageWithMetadata(generation, sid, "assistant", finalContent, "complete").catch((e) => {
          console.error("Failed to persist assistant message", e);
          setError((prev) => prev ?? `Response wasn't saved: ${e}`);
        });
      }
    }
  });
  useEffect(() => { scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" }); }, [messages]);

  async function cancelGeneration() {
    const requestId = activeRequestId.current;
    if (!requestId) return;
    try {
      await ai.cancelAiRequest(requestId);
    } catch (error) {
      setError(`Could not cancel generation: ${error}`);
    }
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
    const generation = workspaceGeneration;
    {
      try {
        await ai.appendSessionMessage(generation, sid, "user", um.content, "complete");
        if (workspaceGenerationRef.current !== generation) return;
      } catch (e) {
        if (workspaceGenerationRef.current !== generation) return;
        console.error("Failed to persist user message", e);
        setError(`Message wasn't saved: ${e}`);
        setSending(false);
        activeRequestId.current = null;
        return;
      }
    }

    const selectedModel = models.find(
      (model) => `${model.provider_id}\u0000${model.model_id}` === selectedModelKey,
    );
    if (!selectedModel) {
      setError("No enabled chat-capable provider and model is configured");
      setSending(false);
      activeRequestId.current = null;
      return;
    }
    let cp: string | null = null;
    try {
      const contextQuery = liveSelectedContext ? `${um.content}\nSelected workspace context: ${liveSelectedContext}` : um.content;
      cp = await ai.getContextForQuery(contextQuery);
      if (workspaceGenerationRef.current !== generation) return;
    } catch { cp = null; }
    const out: ai.ChatMessage[] = [];
    if (cp) out.push({ role: "system", content: cp });
    out.push(...nm.map((m) => ({ role: m.role, content: m.content })));
    try {
      const appConfig = await getAppConfig();
      await ai.chatWithModel(
        rid,
        selectedModel.provider_id,
        selectedModel.model_id,
        out,
        generation,
        appConfig.shareWorkspaceContextWithCloud,
      );
    }
    catch (e) {
      if (activeRequestId.current === rid) {
        setError(String(e));
        setSending(false);
        activeRequestId.current = null;
      }
    }
  }

  if (modelsLoading) return <div className="flex h-full items-center justify-center gap-2 text-xs text-neutral-500"><Spinner size={12} />Loading configured providers...</div>;
  if (models.length === 0) return <EmptyState icon="🔌" title="No chat model configured" hint="Enable a chat-capable provider and add or discover at least one model in Settings." />;
  if (workspaceOpen && sessionState === "loading") return <div className="flex h-full items-center justify-center gap-2 text-xs text-neutral-500"><Spinner size={12} />Loading conversation...</div>;
  // Empty session state (v1.3.0 Phase 4B): reachable after deleting the
  // last session in a workspace. SessionTabs' "+ New" stays enabled here -
  // this is just the message pane telling the user there's nothing active
  // to send into yet.
  if (workspaceOpen && sessionsReady && !activeSessionId) return <EmptyState icon="💬" title="No active session" hint={'Click "+ New" above to start a conversation'} />;

  return (
    <div className="flex h-full flex-col bg-white dark:bg-neutral-900">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-neutral-200 px-2 dark:border-neutral-800">
        <select value={selectedModelKey} onChange={(e) => void handleModelChange(e.target.value)} className="rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
          {models.map((model) => (
            <option key={`${model.provider_id}:${model.model_id}`} value={`${model.provider_id}\u0000${model.model_id}`}>
              {model.provider_name}: {model.display_name}
            </option>
          ))}
        </select>
        {connectedWorkspace && (
          <div title={workspaceRoot ?? undefined} className="ml-auto max-w-[220px] truncate rounded bg-neutral-100 px-2 py-1 text-[10px] font-medium text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
            Workspace: {connectedWorkspace}
          </div>
        )}
        {liveSelectedContext && <div title={liveSelectedContext} className="max-w-[180px] truncate rounded bg-blue-50 px-2 py-1 text-[10px] font-medium text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">Context: {liveSelectedContext.split(/[\\/]/).pop()}</div>}
        {workspaceOpen && <button onClick={handleIndex} disabled={indexing} className="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-neutral-600 transition-colors hover:bg-neutral-100 disabled:opacity-60 dark:text-neutral-300 dark:hover:bg-neutral-800">{indexing && <Spinner size={10} />}{indexing ? "Indexing..." : "Index Workspace"}</button>}
      </div>
      {indexStatus && <div className="border-b border-neutral-200 px-2 py-1 text-[10px] text-neutral-500 dark:border-neutral-800 dark:text-neutral-500">{indexStatus}</div>}
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
          <button onClick={handleSend} disabled={!selectedModelKey || !activeSessionId} className="flex shrink-0 items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50">{sending && <Spinner size={10} />}Send</button>
        )}
      </div>
    </div>
  );
}
