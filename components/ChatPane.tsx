"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import { useEvent } from "@/hooks/useEvent";
import Spinner from "@/components/ui/Spinner";
import EmptyState from "@/components/ui/EmptyState";
import ErrorBanner from "@/components/ui/ErrorBanner";

interface DisplayMessage {
  role: "user" | "assistant";
  content: string;
  fromCache?: boolean;
  timestamp: number;
}

interface TokenPayload {
  request_id: string;
  token: string;
  done: boolean;
  from_cache?: boolean;
}

export interface ChatPaneProps {
  workspaceOpen: boolean;
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export default function ChatPane({ workspaceOpen }: ChatPaneProps) {
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
  const activeRequestId = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  async function handleIndex() {
    setIndexing(true);
    setIndexStatus(null);
    try {
      const stats = await ai.indexWorkspace();
      setIndexStatus(`Indexed ${stats.files_indexed} files (${stats.chunks_created} chunks)`);
    } catch (e) {
      setIndexStatus(`Index failed: ${e}`);
    } finally {
      setIndexing(false);
    }
  }

  useEffect(() => {
    ai.ollamaHealthCheck().then(async (healthy) => {
      setOllamaAvailable(healthy);
      if (healthy) {
        const list = await ai.listModels();
        setModels(list);
        if (list.length > 0) setSelectedModel(list[0].name);
      }
    });
  }, []);

  useEvent<TokenPayload>("AI_RESPONSE_TOKEN", (payload) => {
    if (payload.request_id !== activeRequestId.current) return;
    setMessages((prev) => {
      const next = [...prev];
      const last = next[next.length - 1];
      if (last && last.role === "assistant") {
        next[next.length - 1] = { ...last, content: last.content + payload.token, fromCache: payload.from_cache };
      } else {
        next.push({ role: "assistant", content: payload.token, fromCache: payload.from_cache, timestamp: Date.now() });
      }
      return next;
    });
    if (payload.done) {
      setSending(false);
      activeRequestId.current = null;
    }
  });

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages]);

  async function handleSend() {
    if (!input.trim() || sending) return;
    setError(null);
    const requestId = crypto.randomUUID();
    activeRequestId.current = requestId;
    const userMessage: DisplayMessage = { role: "user", content: input, timestamp: Date.now() };
    const nextMessages = [...messages, userMessage];
    setMessages(nextMessages);
    setInput("");
    setSending(true);

    let modelToUse = selectedModel;
    if (autoMode) {
      try {
        const selection = await ai.autoSelectModel(userMessage.content);
        setAutoSelection(selection);
        modelToUse = selection.model;
      } catch (e) {
        setError(String(e));
        setSending(false);
        activeRequestId.current = null;
        return;
      }
    } else {
      setAutoSelection(null);
    }

    if (!modelToUse) {
      setError("No model available to chat with");
      setSending(false);
      activeRequestId.current = null;
      return;
    }

    // Best-effort workspace context injection: silently skipped if no
    // workspace is open or the index is empty (get_context_for_query
    // errors in that case, which we treat as "no context available").
    let contextPrompt: string | null = null;
    try {
      contextPrompt = await ai.getContextForQuery(userMessage.content);
    } catch {
      contextPrompt = null;
    }

    const outgoing: ai.ChatMessage[] = [];
    if (contextPrompt) {
      outgoing.push({ role: "system", content: contextPrompt });
    }
    outgoing.push(...nextMessages.map((m) => ({ role: m.role, content: m.content })));

    try {
      await ai.chatWithModel(requestId, modelToUse, outgoing);
    } catch (e) {
      setError(String(e));
      setSending(false);
      activeRequestId.current = null;
    }
  }

  if (ollamaAvailable === null) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-neutral-500">
        <Spinner size={12} />
        Checking Ollama...
      </div>
    );
  }

  if (!ollamaAvailable) {
    return (
      <EmptyState
        icon="🔌"
        title="Ollama not detected"
        hint="Install Ollama and make sure it's running at localhost:11434, then reopen NeuralForge."
      />
    );
  }

  return (
    <div className="flex h-full flex-col bg-white dark:bg-neutral-900">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-neutral-200 px-2 dark:border-neutral-800">
        <button
          onClick={() => setAutoMode((v) => !v)}
          className={`rounded px-2 py-1 text-xs font-medium transition-colors ${
            autoMode
              ? "bg-blue-600 text-white hover:bg-blue-500"
              : "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
          }`}
        >
          Auto
        </button>
        {!autoMode && (
          <select
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            className="rounded border border-neutral-200 bg-white px-2 py-1 text-xs text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
          >
            {models.map((m) => (
              <option key={m.name} value={m.name}>
                {m.name} ({m.parameter_size})
              </option>
            ))}
          </select>
        )}
        {workspaceOpen && (
          <button
            onClick={handleIndex}
            disabled={indexing}
            className="ml-auto flex items-center gap-1.5 rounded px-2 py-1 text-xs text-neutral-600 transition-colors hover:bg-neutral-100 disabled:opacity-60 dark:text-neutral-300 dark:hover:bg-neutral-800"
          >
            {indexing && <Spinner size={10} />}
            {indexing ? "Indexing..." : "Index Workspace"}
          </button>
        )}
      </div>
      {indexStatus && (
        <div className="border-b border-neutral-200 px-2 py-1 text-[10px] text-neutral-500 dark:border-neutral-800 dark:text-neutral-500">
          {indexStatus}
        </div>
      )}
      {autoMode && autoSelection && (
        <div className="border-b border-neutral-200 bg-neutral-50 px-2 py-1.5 text-[10px] text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-400">
          Selected <span className="font-medium text-neutral-800 dark:text-neutral-200">{autoSelection.model}</span> from{" "}
          {autoSelection.provider} because {autoSelection.reason}.{" "}
          {autoSelection.is_free ? (
            <span className="font-medium text-green-600 dark:text-green-400">Free</span>
          ) : (
            <span className="font-medium text-yellow-600 dark:text-yellow-400">~${autoSelection.estimated_cost_usd.toFixed(4)}</span>
          )}
        </div>
      )}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {messages.length === 0 && (
          <EmptyState icon="💬" title="Ask NeuralForge anything" hint="Questions about your code get workspace context automatically" />
        )}
        {messages.map((m, i) => {
          const isUser = m.role === "user";
          return (
            <div key={i} className={`mb-3 flex ${isUser ? "justify-end" : "justify-start"}`}>
              <div className={`max-w-[85%] ${isUser ? "items-end" : "items-start"} flex flex-col gap-1`}>
                <div
                  className={`rounded-lg px-3 py-2 text-sm leading-relaxed shadow-sm ${
                    isUser
                      ? "bg-blue-600 text-white"
                      : "bg-neutral-100 text-neutral-800 dark:bg-neutral-800 dark:text-neutral-100"
                  }`}
                >
                  <div className="whitespace-pre-wrap">{m.content || (sending && i === messages.length - 1 ? "…" : "")}</div>
                </div>
                <div className="flex items-center gap-1.5 px-1 text-[10px] text-neutral-400 dark:text-neutral-600">
                  <span>{formatTime(m.timestamp)}</span>
                  {m.fromCache && <span className="font-medium text-yellow-600 dark:text-yellow-500">from cache</span>}
                </div>
              </div>
            </div>
          );
        })}
        {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}
      </div>
      <div className="flex shrink-0 gap-2 border-t border-neutral-200 p-2 dark:border-neutral-800">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              handleSend();
            }
          }}
          placeholder="Ask a question..."
          className="min-w-0 flex-1 rounded border border-neutral-200 bg-white px-2.5 py-1.5 text-sm text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
        />
        <button
          onClick={handleSend}
          disabled={sending || (!autoMode && !selectedModel)}
          className="flex items-center gap-1.5 rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        >
          {sending && <Spinner size={10} />}
          {sending ? "Sending" : "Send"}
        </button>
      </div>
    </div>
  );
}
