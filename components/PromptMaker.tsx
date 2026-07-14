"use client";

import { useEffect, useState } from "react";
import Spinner from "@/components/ui/Spinner";

export interface PromptMakerProps { onClose: () => void; }

export default function PromptMaker({ onClose }: PromptMakerProps) {
  const [userIntent, setUserIntent] = useState("");
  const [generatedPrompt, setGeneratedPrompt] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const [generationError, setGenerationError] = useState("");

  useEffect(() => {
    const saved = localStorage.getItem("nf_custom_prompt");
    if (saved) setGeneratedPrompt(saved);
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) { if (e.key === "Escape") onClose(); }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  async function handleGenerate() {
    if (!userIntent.trim() || isGenerating) return;
    setIsGenerating(true);
    setGenerationError("");

    const systemPrompt = `You are a Meta-Prompt Engineer. Based on the user's objective, generate a beautifully structured AI system prompt containing:
1. A System Persona / Role definition.
2. Strict Edge-Case Constraints the model must follow.
3. Explicit Output Formatting rules.

Output ONLY the final prompt template. Do not include markdown fences or explanatory text.`;

    try {
      const res = await fetch("http://127.0.0.1:11434/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "deepseek-coder:latest",
          prompt: `${systemPrompt}\n\nUser Objective: ${userIntent}\n`,
          stream: false,
        }),
      });

      if (!res.ok) {
        const errText = await res.text().catch(() => `HTTP ${res.status}`);
        throw new Error(errText);
      }

      const data = await res.json();
      const output = (data.response || "").trim();
      if (!output) throw new Error("Model returned empty response");

      setGeneratedPrompt(output);
      localStorage.setItem("nf_custom_prompt", output);
    } catch (error: any) {
      console.error("Inference link broken:", error);
      setGenerationError(
        "OLLAMA BACKEND OFFLINE: Local hardware pipeline is unreachable at port 11434.\n\n" +
        "🔧 FIX PROTOCOL:\n" +
        "1. Ensure the Ollama icon is active in your Windows System Tray.\n" +
        "2. If active but rejected, terminate Ollama and run this command in CMD to bypass local CORS headers:\n" +
        "   set OLLAMA_ORIGINS=* && ollama serve\n" +
        "3. Ensure your local hardware has the baseline model pulled ('ollama pull qwen2.5-coder:7b' or your mapped alternative)."
      );
    } finally {
      setIsGenerating(false);
    }
  }

  function handleSave() {
    localStorage.setItem("nf_custom_prompt", generatedPrompt);
    onClose();
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-[1px]" onClick={onClose}>
      <div onClick={(e) => e.stopPropagation()} className="max-h-[85vh] w-[600px] overflow-y-auto rounded-lg border border-neutral-200 bg-white p-5 text-sm text-neutral-800 shadow-2xl dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200">
        <div className="mb-5 flex items-center justify-between">
          <h2 className="text-base font-semibold">🛠️ AI Prompt Orchestration Studio</h2>
          <button onClick={onClose} className="rounded px-1.5 py-0.5 text-neutral-400 transition-colors hover:bg-neutral-100 dark:text-neutral-500 dark:hover:bg-neutral-800">✕</button>
        </div>

        <div className="space-y-4">
          {/* User Intent Input */}
          <div>
            <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">What objective or role do you want to optimize this AI agent for?</label>
            <textarea
              value={userIntent}
              onChange={(e) => setUserIntent(e.target.value)}
              rows={3}
              placeholder="e.g., Make a drift physics tuner for Assetto Corsa"
              className="w-full resize-none rounded border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
            />
          </div>

          {/* Generate Button */}
          <button
            onClick={handleGenerate}
            disabled={isGenerating || !userIntent.trim()}
            className="flex w-full items-center justify-center gap-2 rounded bg-purple-600 px-4 py-2.5 text-sm font-semibold text-white transition-colors hover:bg-purple-500 disabled:opacity-50"
          >
            {isGenerating && <Spinner size={14} />}
            {isGenerating ? "Generating via Local AI..." : "⚡ Generate System Prompt via Local AI"}
          </button>

          {generationError && (
            <div className="rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-800 dark:bg-red-900/30 dark:text-red-400">
              Error: {generationError}
            </div>
          )}

          {/* Generated Prompt */}
          {generatedPrompt && (
            <div>
              <label className="mb-1 block text-xs font-medium uppercase tracking-wide text-neutral-400">Final System Prompt (Editable)</label>
              <textarea
                value={generatedPrompt}
                onChange={(e) => setGeneratedPrompt(e.target.value)}
                rows={12}
                className="w-full resize-none rounded border border-neutral-200 bg-white px-3 py-2 font-mono text-xs text-neutral-800 outline-none transition-colors focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200"
              />
            </div>
          )}
        </div>

        <div className="mt-5 flex justify-end gap-2 border-t border-neutral-100 pt-4 dark:border-neutral-800">
          <button onClick={onClose} className="rounded px-4 py-1.5 text-xs font-medium text-neutral-500 transition-colors hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-800">Cancel</button>
          <button onClick={handleSave} className="rounded bg-purple-600 px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-purple-500">Save Prompt Template</button>
        </div>
      </div>
    </div>
  );
}