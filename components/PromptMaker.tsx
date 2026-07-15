"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import { getAppConfig } from "@/lib/store";
import { useEvent } from "@/hooks/useEvent";

export interface PromptMakerProps { onClose: () => void; }

interface TokenPayload { request_id: string; token: string; done: boolean; from_cache?: boolean; }

const META_PROMPT_SYSTEM_INSTRUCTION =
  "You are an Elite 100% Precision System Prompt Architect. Given a target objective, generate a complete professional system prompt with clear Personas, strict constraints, and formatting parameters. Output only the system prompt template.";

export default function PromptMaker({ onClose }: PromptMakerProps) {
  const [userIntent, setUserIntent] = useState("");
  const [generatedPrompt, setGeneratedPrompt] = useState("");
  const [genState, setGenState] = useState<"idle" | "checking" | "generating" | "complete" | "error">("idle");
  const [generationError, setGenerationError] = useState("");
  const [effort, setEffort] = useState<"Light" | "Medium" | "High" | "Extra High">("High");
  const [copied, setCopied] = useState(false);
  const activeRequestId = useRef<string | null>(null);

  useEffect(() => { const saved = localStorage.getItem("nf_custom_prompt"); if (saved) { setGeneratedPrompt(saved); setGenState("complete"); } }, []);
  useEffect(() => {
    function syncEffort() { getAppConfig().then((c) => setEffort(c.effort)); }
    syncEffort();
    window.addEventListener("nf_settings_updated", syncEffort);
    return () => window.removeEventListener("nf_settings_updated", syncEffort);
  }, []);
  useEffect(() => { function onKeyDown(e: KeyboardEvent) { if (e.key === "Escape") onClose(); } window.addEventListener("keydown", onKeyDown); return () => window.removeEventListener("keydown", onKeyDown); }, [onClose]);

  useEvent<TokenPayload>("AI_RESPONSE_TOKEN", (payload) => {
    if (payload.request_id !== activeRequestId.current) return;
    setGenState((prev) => (prev === "generating" ? prev : "generating"));
    setGeneratedPrompt((prev) => (payload.from_cache ? payload.token : prev + payload.token));
    if (payload.done) {
      setGeneratedPrompt((finalText) => {
        localStorage.setItem("nf_custom_prompt", finalText);
        return finalText;
      });
      setGenState("complete");
      activeRequestId.current = null;
    }
  });

  async function handleGenerate() {
    if (!userIntent.trim() || genState === "generating" || genState === "checking") return;
    setGenState("checking");
    setGenerationError("");
    setGeneratedPrompt("");

    const config = await getAppConfig();
    if (config.provider !== "ollama") {
      setGenerationError(
        `Provider: ${config.provider}\nAction: Generate system prompt\nFailure reason: Only Ollama is currently wired to the local runtime.\nPossible fix: Switch the provider to Ollama in Settings.`
      );
      setGenState("error");
      return;
    }
    if (!config.model) {
      setGenerationError(
        `Provider: ollama\nAction: Generate system prompt\nFailure reason: No model is configured.\nPossible fix: Choose a model in Settings.`
      );
      setGenState("error");
      return;
    }

    const requestId = crypto.randomUUID();
    activeRequestId.current = requestId;
    setGenState("generating");

    try {
      await ai.chatWithModel(requestId, config.model, [
        { role: "system", content: META_PROMPT_SYSTEM_INSTRUCTION },
        { role: "user", content: `Target Objective to Engineer: ${userIntent}` },
      ]);
    } catch (err: any) {
      activeRequestId.current = null;
      setGenerationError(
        `Provider: ollama\nAction: Generate system prompt\nFailure reason: ${err?.message || String(err)}\nPossible fix: Verify Ollama is running at the endpoint configured in Settings.`
      );
      setGenState("error");
    }
  }

  async function handleCopy() {
    if (!generatedPrompt) return;
    try {
      await navigator.clipboard.writeText(generatedPrompt);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = generatedPrompt;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }

  const isRunning = genState === "checking" || genState === "generating";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()} className="max-h-[85vh] w-[600px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
        <div className="mb-5 flex items-center justify-between"><h2 className="text-base font-semibold">🛠️ AI Prompt Orchestration Studio</h2><button onClick={onClose} className="rounded px-1.5 py-0.5 text-neutral-400 hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button></div>

        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">What objective or role do you want to optimize this AI agent for?</label>
            <textarea value={userIntent} onChange={(e) => setUserIntent(e.target.value)} rows={3} placeholder="e.g., Make a drift physics tuner for Assetto Corsa" disabled={isRunning} className="w-full resize-none rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none transition-colors focus:border-blue-500 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>

          <div className="text-xs text-neutral-400">
            Effort: <span className="font-medium text-neutral-600 dark:text-neutral-300">{effort}</span>
            <span className="ml-1 text-neutral-400">(set in Settings)</span>
          </div>

          <button onClick={handleGenerate} disabled={isRunning || !userIntent.trim()} className="flex w-full items-center justify-center gap-2 rounded bg-purple-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-purple-500 disabled:opacity-50">
            {isRunning && <Spinner size={14} />}
            {genState === "checking" ? "Checking runtime..." : genState === "generating" ? "Generating prompt..." : "⚡ Generate System Prompt via Runtime"}
          </button>

          {generationError && (<div className="rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 whitespace-pre-wrap dark:border-red-800 dark:bg-red-900/30 dark:text-red-400">{generationError}</div>)}

          <div contentEditable={false} className="flex-1 bg-black/5 dark:bg-black/50 border border-neutral-200 dark:border-neutral-700 rounded p-4 text-neutral-700 dark:text-neutral-300 text-sm overflow-y-auto whitespace-pre-wrap select-text cursor-text max-h-64">
            {generatedPrompt || (isRunning ? "Generating..." : "Awaiting input (e.g., Optimize Assetto Corsa lua chase camera script...)")}
          </div>

          <button disabled={!generatedPrompt} onClick={handleCopy} className="w-full rounded bg-neutral-200 dark:bg-neutral-700 hover:bg-neutral-300 dark:hover:bg-neutral-600 text-neutral-700 dark:text-white py-2 px-6 font-semibold text-sm disabled:opacity-40 disabled:pointer-events-none transition-colors">
            {copied ? "✓ Copied!" : "📋 Copy Prompt"}
          </button>
        </div>

        <div className="mt-4 flex justify-end border-t border-neutral-100 pt-3 dark:border-neutral-800">
          <button onClick={onClose} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Close</button>
        </div>
      </div>
    </div>
  );
}
