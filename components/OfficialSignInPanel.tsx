"use client";

import { useEffect, useState } from "react";
import * as officialSignIn from "@/lib/officialSignin";

const FALLBACK_CLIENTS: officialSignIn.OfficialSignInClient[] = [
  {
    id: "chatgpt_codex",
    title: "ChatGPT / Codex",
    detail: "Opens the official ChatGPT/Codex Windows app. Its account remains client-bound and is not imported into NeuralForge.",
    available: true,
  },
  {
    id: "claude_code",
    title: "Claude / Claude Code",
    detail: "Starts Claude Code's official account sign-in in a separate terminal. NeuralForge does not read its credentials.",
    available: false,
  },
  {
    id: "github_cli",
    title: "GitHub",
    detail: "Starts GitHub CLI's official web sign-in. This does not configure a Copilot provider in NeuralForge.",
    available: false,
  },
];

/**
 * Offers the official clients' own account flows without treating a
 * subscription login as a reusable NeuralForge API credential.
 */
export default function OfficialSignInPanel() {
  const [clients, setClients] = useState(FALLBACK_CLIENTS);
  const [launching, setLaunching] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    void officialSignIn.listOfficialSignInClients()
      .then((result) => { if (mounted) setClients(result); })
      .catch(() => {
        // The boundary remains visible in browser-only development mode.
      });
    return () => { mounted = false; };
  }, []);

  async function handleSignIn(client: officialSignIn.OfficialSignInClient) {
    setLaunching(client.id);
    setStatus(null);
    try {
      await officialSignIn.startOfficialSignIn(client.id);
      setStatus(`${client.title} sign-in was opened in its official client.`);
    } catch (error) {
      setStatus(`Could not open ${client.title}: ${String(error)}`);
    } finally {
      setLaunching(null);
    }
  }

  return (
    <section className="mt-4 rounded border border-sky-200 bg-sky-50/80 p-3 dark:border-sky-900/60 dark:bg-sky-950/20">
      <div className="text-xs font-semibold text-sky-950 dark:text-sky-100">Official account sign-in</div>
      <p className="mt-1 text-[11px] leading-4 text-sky-950/80 dark:text-sky-100/80">
        Start each provider&apos;s official sign-in next to Add Provider. NeuralForge never imports browser sessions, cookies, OAuth tokens, or credentials from another AI client.
      </p>
      <div className="mt-3 space-y-2">
        {clients.map((client) => (
          <div key={client.id} className="flex items-center justify-between gap-3 rounded border border-sky-200/80 bg-white/70 p-2 dark:border-sky-900/60 dark:bg-neutral-900/60">
            <div className="min-w-0">
              <div className="text-xs font-medium text-neutral-800 dark:text-neutral-100">{client.title}</div>
              <div className="mt-0.5 text-[11px] leading-4 text-neutral-500 dark:text-neutral-400">{client.detail}</div>
            </div>
            <button
              type="button"
              disabled={!client.available || launching !== null}
              onClick={() => handleSignIn(client)}
              className="shrink-0 rounded bg-sky-600 px-2.5 py-1.5 text-[11px] font-medium text-white transition-colors hover:bg-sky-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {launching === client.id ? "Opening…" : client.available ? "Sign in" : "Not installed"}
            </button>
          </div>
        ))}
      </div>
      <p className="mt-3 text-[11px] leading-4 text-sky-950/80 dark:text-sky-100/80">
        These actions do not authorize direct NeuralForge cloud requests. Add Provider remains the explicit API-credential path. GitHub Copilot additionally requires a registered GitHub App/OAuth integration, which is not configured here.
      </p>
      {status && <p role="status" className="mt-2 text-[11px] text-sky-950 dark:text-sky-100">{status}</p>}
    </section>
  );
}
