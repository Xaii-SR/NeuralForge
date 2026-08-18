"use client";

import { useEffect, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import { getAppConfig } from "@/lib/store";
import { useEvent } from "@/hooks/useEvent";
import { AUTO_MODEL, choosePreferredModel, createWorkflowRoles, modelKey } from "@/lib/agent-workflow";

export interface PromptMakerProps { onClose: () => void; }

interface TokenPayload { request_id: string; token: string; done: boolean; from_cache?: boolean; }

function loadSavedPrompt(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem("nf_custom_prompt") ?? "";
}

const PROMPT_TEMPLATES = {
  system: "Generate a complete professional system prompt with a clear role, operating constraints, input contract, output format, safety boundaries, and an explicit definition of done. Output only the prompt.",
  implementation: "Generate an implementation brief for an AI coding agent. Require it to inspect before editing, name the files it will change, keep scope bounded, run concrete validation, and report limitations. Output only the brief.",
  review: "Generate a rigorous code or design review prompt. Require evidence, prioritized findings, tests or verification evidence, explicit uncertainty, and no fabricated results. Output only the review prompt.",
} as const;

export default function PromptMaker({ onClose }: PromptMakerProps) {
  const [userIntent, setUserIntent] = useState("");
  const [initialPrompt] = useState(loadSavedPrompt);
  const [generatedPrompt, setGeneratedPrompt] = useState(initialPrompt);
  const [genState, setGenState] = useState<"idle" | "checking" | "generating" | "complete" | "error">(initialPrompt ? "complete" : "idle");
  const [generationError, setGenerationError] = useState("");
  const [effort, setEffort] = useState<"Light" | "Medium" | "High" | "Extra High">("High");
  const [template, setTemplate] = useState<keyof typeof PROMPT_TEMPLATES>("system");
  const [models, setModels] = useState<ai.ChatModelDescriptor[]>([]);
  const [modelChoice, setModelChoice] = useState(AUTO_MODEL);
  const [copied, setCopied] = useState(false);
  const activeRequestId = useRef<string | null>(null);

  useEffect(() => {
    function syncEffort() { getAppConfig().then((c) => setEffort(c.effort)); }
    syncEffort();
    window.addEventListener("nf_settings_updated", syncEffort);
    return () => window.removeEventListener("nf_settings_updated", syncEffort);
  }, []);
  useEffect(() => {
    let active = true;
    ai.listChatModels().then((available) => { if (active) setModels(available); }).catch(() => { if (active) setModels([]); });
    return () => { active = false; };
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

    const promptRole = createWorkflowRoles("team", "write a prompt")[0];
    const selected = modelChoice === AUTO_MODEL
      ? choosePreferredModel(promptRole, models)
      : models.find((model) => modelKey(model) === modelChoice);
    if (!selected) {
      setGenerationError(
        "Action: Generate system prompt\nFailure reason: No enabled chat model is configured.\nPossible fix: Configure and assign a Chat model in Settings."
      );
      setGenState("error");
      return;
    }

    const requestId = crypto.randomUUID();
    activeRequestId.current = requestId;
    setGenState("generating");

    try {
      await ai.chatWithModel(requestId, selected.provider_id, selected.model_id, [
        { role: "system", content: `You are an expert prompt architect. ${PROMPT_TEMPLATES[template]}` },
        { role: "user", content: `Target Objective to Engineer: ${userIntent}` },
      ]);
    } catch (err: any) {
      activeRequestId.current = null;
      setGenerationError(
        `Provider: ${selected.provider_name}\nAction: Generate system prompt\nFailure reason: ${err?.message || String(err)}\nPossible fix: Verify the selected provider in Settings.`
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
            <textarea value={userIntent} onChange={(e) => setUserIntent(e.target.value)} rows={3} placeholder="Example: Write a Python script to scrape a website" disabled={isRunning} className="w-full resize-none rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none transition-colors focus:border-blue-500 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200" />
          </div>

          <div className="text-xs text-neutral-400">
            Effort: <span className="font-medium text-neutral-600 dark:text-neutral-300">{effort}</span>
            <span className="ml-1 text-neutral-400">(set in Settings)</span>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <label className="text-xs text-neutral-500 dark:text-neutral-400">Prompt format
              <select value={template} onChange={(event) => setTemplate(event.target.value as keyof typeof PROMPT_TEMPLATES)} disabled={isRunning} className="mt-1 w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none focus:border-purple-500 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                <option value="system">System prompt</option><option value="implementation">Implementation brief</option><option value="review">Review prompt</option>
              </select>
            </label>
            <label className="text-xs text-neutral-500 dark:text-neutral-400">Generation model
              <select value={modelChoice} onChange={(event) => setModelChoice(event.target.value)} disabled={isRunning} className="mt-1 w-full rounded border border-neutral-200 bg-white px-2 py-1.5 text-xs text-neutral-800 outline-none focus:border-purple-500 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200">
                <option value={AUTO_MODEL}>Auto — prefer best local model</option>
                {models.map((model) => <option key={modelKey(model)} value={modelKey(model)}>{model.is_local ? "Local · " : "Remote · "}{model.provider_name}: {model.display_name}</option>)}
              </select>
            </label>
          </div>

          <button onClick={handleGenerate} disabled={isRunning || !userIntent.trim()} className="flex w-full items-center justify-center gap-2 rounded bg-purple-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-purple-500 disabled:opacity-50">
            {isRunning && <Spinner size={14} />}
            {genState === "checking" ? "Checking runtime..." : genState === "generating" ? "Generating prompt..." : "⚡ Generate System Prompt via Runtime"}
          </button>

          {generationError && (<div className="rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 whitespace-pre-wrap dark:border-red-800 dark:bg-red-900/30 dark:text-red-400">{generationError}</div>)}

          <div contentEditable={false} className="flex-1 bg-black/5 dark:bg-black/50 border border-neutral-200 dark:border-neutral-700 rounded p-4 text-neutral-700 dark:text-neutral-300 text-sm overflow-y-auto whitespace-pre-wrap select-text cursor-text max-h-64">
            {generatedPrompt || (isRunning ? "Generating..." : "Awaiting input (e.g., Design a secure local-first AI code-review workflow...)")}
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
