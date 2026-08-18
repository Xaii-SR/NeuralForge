"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import * as ai from "@/lib/ai";
import Spinner from "@/components/ui/Spinner";
import ErrorBanner from "@/components/ui/ErrorBanner";
import CopyButton from "@/components/ui/CopyButton";
import { getAppConfig } from "@/lib/store";
import { useEvent } from "@/hooks/useEvent";
import {
  AUTO_MODEL,
  buildStageMessages,
  choosePreferredModel,
  createWorkflowRoles,
  modelKey,
  type WorkflowStageResult,
  type WorkflowVariant,
} from "@/lib/agent-workflow";

interface TokenPayload {
  request_id: string;
  token: string;
  done: boolean;
}

interface AITeamPanelProps {
  variant?: WorkflowVariant;
  workspaceGeneration: number;
}

const STATUS_STYLE: Record<WorkflowStageResult["status"], string> = {
  waiting: "bg-neutral-100 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400",
  running: "bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300",
  complete: "bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-300",
  failed: "bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300",
};

function ModelSelect({
  roleId,
  selected,
  recommended,
  models,
  onChange,
}: {
  roleId: string;
  selected: string;
  recommended: ai.ChatModelDescriptor | undefined;
  models: ai.ChatModelDescriptor[];
  onChange: (value: string) => void;
}) {
  return (
    <select
      aria-label={`Model for ${roleId}`}
      value={selected}
      onChange={(event) => onChange(event.target.value)}
      className="w-full rounded border border-neutral-200 bg-white px-2 py-1 text-[11px] outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-800"
    >
      <option value={AUTO_MODEL}>
        Auto — {recommended ? `${recommended.provider_name}: ${recommended.display_name}` : "no available model"}
      </option>
      {models.map((model) => (
        <option key={modelKey(model)} value={modelKey(model)}>
          {model.is_local ? "Local · " : "Remote · "}{model.provider_name}: {model.display_name}
        </option>
      ))}
    </select>
  );
}

export default function AITeamPanel({ variant = "team", workspaceGeneration }: AITeamPanelProps) {
  const [task, setTask] = useState("");
  const [models, setModels] = useState<ai.ChatModelDescriptor[]>([]);
  const [modelLoading, setModelLoading] = useState(true);
  const [overrides, setOverrides] = useState<Record<string, string>>({});
  const [stages, setStages] = useState<WorkflowStageResult[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const outputsRef = useRef<Record<string, string>>({});
  const requestRolesRef = useRef<Record<string, string>>({});
  const activeRequestIdsRef = useRef(new Set<string>());
  const mountedRef = useRef(true);
  const generationRef = useRef(workspaceGeneration);
  const roles = useMemo(() => createWorkflowRoles(variant, task), [task, variant]);

  useEffect(() => {
    generationRef.current = workspaceGeneration;
  }, [workspaceGeneration]);

  useEffect(() => {
    let active = true;
    ai.listChatModels()
      .then((available) => { if (active) setModels(available); })
      .catch(() => { if (active) setModels([]); })
      .finally(() => { if (active) setModelLoading(false); });
    return () => { active = false; };
  }, []);

  // Closing the modal or changing its key must not leave local/cloud model
  // work running in the background. The backend treats cancellation as an
  // explicit terminal state and emits no successful result afterward.
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      for (const requestId of activeRequestIdsRef.current) {
        void ai.cancelAiRequest(requestId);
      }
      activeRequestIdsRef.current.clear();
    };
  }, []);

  useEvent<TokenPayload>("AI_RESPONSE_TOKEN", (payload) => {
    const roleId = requestRolesRef.current[payload.request_id];
    if (!roleId || !payload.token) return;
    const output = `${outputsRef.current[roleId] ?? ""}${payload.token}`;
    outputsRef.current[roleId] = output;
    setStages((previous) => previous.map((stage) => stage.role.id === roleId ? { ...stage, output } : stage));
  });

  function resolveModel(roleId: string) {
    const role = roles.find((candidate) => candidate.id === roleId);
    if (!role) return undefined;
    const override = overrides[roleId] ?? AUTO_MODEL;
    return override === AUTO_MODEL
      ? choosePreferredModel(role, models)
      : models.find((model) => modelKey(model) === override);
  }

  async function runWorkflow() {
    const objective = task.trim();
    if (!objective || running) return;
    if (!models.length) {
      setError("No enabled chat model is available. Configure an enabled local Ollama model or a provider model in Settings.");
      return;
    }
    const generation = workspaceGeneration;
    const initialStages = roles.map((role) => ({ role, status: "waiting" as const, output: "", model: resolveModel(role.id) }));
    outputsRef.current = {};
    requestRolesRef.current = {};
    setStages(initialStages);
    setError(null);
    setRunning(true);

    const finished: WorkflowStageResult[] = [];
    try {
      const config = await getAppConfig();
      for (const stage of initialStages) {
        const model = stage.model;
        if (!model) throw new Error(`No model could be selected for ${stage.role.title}.`);
        if (generationRef.current !== generation) return;
        setStages((previous) => previous.map((item) => item.role.id === stage.role.id ? { ...item, status: "running" } : item));
        const requestId = crypto.randomUUID();
        requestRolesRef.current[requestId] = stage.role.id;
        activeRequestIdsRef.current.add(requestId);
        try {
          await ai.chatWithModel(
            requestId,
            model.provider_id,
            model.model_id,
            buildStageMessages(variant, objective, stage.role, finished),
            generation,
            config.shareWorkspaceContextWithCloud,
          );
        } finally {
          activeRequestIdsRef.current.delete(requestId);
          delete requestRolesRef.current[requestId];
        }
        if (!mountedRef.current || generationRef.current !== generation) return;
        const output = outputsRef.current[stage.role.id]?.trim();
        if (!output) throw new Error(`${stage.role.title} completed without a response.`);
        const completed = { ...stage, status: "complete" as const, output };
        finished.push(completed);
        setStages((previous) => previous.map((item) => item.role.id === stage.role.id ? completed : item));
      }
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      if (mountedRef.current && generationRef.current === generation) {
        setError(`Workflow stopped: ${message}`);
        setStages((previous) => previous.map((stage) => stage.status === "running" ? { ...stage, status: "failed", error: message } : stage));
      }
    } finally {
      if (generationRef.current === generation) setRunning(false);
    }
  }

  const title = variant === "team" ? "🤝 AI TEAM" : "⚖️ AI Council";
  const description = variant === "team"
    ? "A bounded, sequential team: each specialist receives relevant prior evidence and the final lead produces one answer. Local models are preferred automatically; every role can be overridden."
    : "One bounded review pass: reviewer, critic, and final editor. It completes once and returns one final review; it does not become an ongoing chat.";
  const finalStage = stages.at(-1);

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto p-1">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-[11px] leading-4 text-neutral-500 dark:text-neutral-400">{description}</p>
      </div>
      <textarea
        value={task}
        onChange={(event) => setTask(event.target.value)}
        disabled={running}
        rows={variant === "team" ? 4 : 3}
        placeholder={variant === "team" ? "Describe the outcome, constraints, and context for your AI team…" : "What should the Council review?"}
        className="w-full shrink-0 resize-none rounded border border-neutral-200 bg-white px-3 py-2 text-sm outline-none focus:border-green-500 disabled:opacity-60 dark:border-neutral-700 dark:bg-neutral-800"
      />

      <div className="grid gap-2 md:grid-cols-2">
        {roles.map((role, index) => {
          const selected = overrides[role.id] ?? AUTO_MODEL;
          const recommended = choosePreferredModel(role, models);
          const stage = stages.find((item) => item.role.id === role.id);
          return (
            <div key={role.id} className="rounded border border-neutral-200 p-2.5 dark:border-neutral-800">
              <div className="flex items-start justify-between gap-2">
                <div><span className="mr-1 text-[10px] text-neutral-400">{index + 1}.</span><span className="text-xs font-semibold">{role.title}</span></div>
                {stage && <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${STATUS_STYLE[stage.status]}`}>{stage.status}</span>}
              </div>
              <p className="mt-1 min-h-8 text-[11px] leading-4 text-neutral-500 dark:text-neutral-400">{role.responsibility}</p>
              <label className="mt-2 block text-[10px] font-medium uppercase tracking-wide text-neutral-400">Model</label>
              <ModelSelect roleId={role.id} selected={selected} recommended={recommended} models={models} onChange={(value) => setOverrides((previous) => ({ ...previous, [role.id]: value }))} />
              {stage?.model && <div className="mt-1 text-[10px] text-neutral-400">Using {stage.model.is_local ? "local" : "remote"}: {stage.model.display_name}</div>}
              {stage?.output && <div className="group relative mt-2 max-h-40 overflow-y-auto rounded bg-neutral-50 p-2 text-[11px] leading-4 whitespace-pre-wrap dark:bg-neutral-800/60"><CopyButton text={stage.output} className="absolute right-1 top-1 opacity-0 group-hover:opacity-100" />{stage.output}</div>}
              {stage?.error && <div className="mt-2 text-[11px] text-red-600 dark:text-red-400">{stage.error}</div>}
            </div>
          );
        })}
      </div>

      {modelLoading && <div className="flex items-center gap-2 text-xs text-neutral-500"><Spinner size={12} /> Discovering enabled models…</div>}
      {error && <ErrorBanner message={error} onDismiss={() => setError(null)} onRetry={runWorkflow} />}

      <button
        onClick={runWorkflow}
        disabled={running || !task.trim() || modelLoading}
        className={`flex shrink-0 items-center justify-center gap-2 rounded px-4 py-2 text-sm font-semibold text-white transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${variant === "team" ? "bg-green-600 hover:bg-green-500" : "bg-red-600 hover:bg-red-500"}`}
      >
        {running && <Spinner size={14} />}
        {running ? (variant === "team" ? "AI TEAM is coordinating…" : "Council is reviewing…") : (variant === "team" ? "Run AI TEAM" : "Run one Council review")}
      </button>

      {finalStage?.status === "complete" && (
        <div className="rounded border border-green-200 bg-green-50 p-3 dark:border-green-900/50 dark:bg-green-950/20">
          <div className="mb-1 text-[10px] font-semibold uppercase tracking-wide text-green-700 dark:text-green-300">Final result</div>
          <div className="whitespace-pre-wrap text-xs leading-5 text-neutral-800 dark:text-neutral-200">{finalStage.output}</div>
        </div>
      )}
    </div>
  );
}
