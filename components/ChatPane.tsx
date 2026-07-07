"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import { useEvent } from "@/hooks/useEvent";

interface DisplayMessage {
  role: "user" | "assistant";
  content: string;
}

interface TokenPayload {
  request_id: string;
  token: string;
  done: boolean;
}

export default function ChatPane() {
  const [ollamaAvailable, setOllamaAvailable] = useState<boolean | null>(null);
  const [models, setModels] = useState<ai.OllamaModel[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>("");
  const [messages, setMessages] = useState<DisplayMessage[]>([]);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeRequestId = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

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
        next[next.length - 1] = { ...last, content: last.content + payload.token };
      } else {
        next.push({ role: "assistant", content: payload.token });
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
    if (!input.trim() || !selectedModel || sending) return;
    setError(null);
    const requestId = crypto.randomUUID();
    activeRequestId.current = requestId;
    const userMessage: DisplayMessage = { role: "user", content: input };
    const nextMessages = [...messages, userMessage];
    setMessages(nextMessages);
    setInput("");
    setSending(true);

    try {
      await ai.chatWithModel(
        requestId,
        selectedModel,
        nextMessages.map((m) => ({ role: m.role, content: m.content }))
      );
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
      </div>
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
        {messages.map((m, i) => (
          <div key={i} className="mb-3">
            <div className="mb-1 text-[10px] uppercase text-neutral-500">{m.role}</div>
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
          disabled={sending || !selectedModel}
          className="rounded bg-neutral-700 px-3 py-1 text-xs text-neutral-200 hover:bg-neutral-600 disabled:opacity-50"
        >
          {sending ? "..." : "Send"}
        </button>
      </div>
    </div>
  );
}
