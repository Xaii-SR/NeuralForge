"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import { useEvent } from "@/hooks/useEvent";

interface DisplayMessage {
  role: "user" | "assistant";
  content: string;
  fromCache?: boolean;
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
        next.push({ role: "assistant", content: payload.token, fromCache: payload.from_cache });
      }
      return next;
    });
    if (payload.done) {
      setSending(false);
      activeRequestId.current = null;
    }
  });

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages]);

  async function handleSend() {
    if (!input.trim() || sending) return;
    setError(null);
    const requestId = crypto.randomUUID();
    activeRequestId.current = requestId;
    const userMessage: DisplayMessage = { role: "user", content: input };
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
    return <div className="flex h-full items-center justify-center text-xs text-neutral-500">Checking Ollama...</div>;
  }

  if (!ollamaAvailable) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-center text-xs text-neutral-500">
        <div>Ollama not detected at localhost:11434</div>
        <div>Install and start Ollama to use local AI chat.</div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-neutral-800 px-2">
        <button
          onClick={() => setAutoMode((v) => !v)}
          className={`rounded px-2 py-0.5 text-xs ${autoMode ? "bg-blue-600 text-white" : "bg-neutral-800 text-neutral-300"}`}
        >
          Auto
        </button>
        {!autoMode && (
          <select
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            className="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-200"
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
            className="ml-auto rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-300 hover:bg-neutral-700 disabled:opacity-50"
          >
            {indexing ? "Indexing..." : "Index Workspace"}
          </button>
        )}
      </div>
      {indexStatus && (
        <div className="border-b border-neutral-800 px-2 py-1 text-[10px] text-neutral-500">{indexStatus}</div>
      )}
      {autoMode && autoSelection && (
        <div className="border-b border-neutral-800 bg-neutral-900 px-2 py-1 text-[10px] text-neutral-400">
          Selected <span className="text-neutral-200">{autoSelection.model}</span> from {autoSelection.provider}{" "}
          because {autoSelection.reason}.{" "}
          {autoSelection.is_free ? (
            <span className="text-green-400">Free</span>
          ) : (
            <span className="text-yellow-400">~${autoSelection.estimated_cost_usd.toFixed(4)}</span>
          )}
        </div>
      )}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {messages.map((m, i) => (
          <div key={i} className="mb-3">
            <div className="mb-1 text-[10px] uppercase text-neutral-500">
              {m.role}
              {m.fromCache && <span className="ml-2 normal-case text-yellow-400">from cache</span>}
            </div>
            <div className="whitespace-pre-wrap text-sm text-neutral-200">{m.content}</div>
          </div>
        ))}
        {error && <div className="text-sm text-red-400">{error}</div>}
      </div>
      <div className="flex shrink-0 gap-2 border-t border-neutral-800 p-2">
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
          className="min-w-0 flex-1 rounded bg-neutral-800 px-2 py-1 text-sm text-neutral-200 outline-none"
        />
        <button
          onClick={handleSend}
          disabled={sending || (!autoMode && !selectedModel)}
          className="rounded bg-neutral-700 px-3 py-1 text-xs text-neutral-200 hover:bg-neutral-600 disabled:opacity-50"
        >
          {sending ? "..." : "Send"}
        </button>
      </div>
    </div>
  );
}
