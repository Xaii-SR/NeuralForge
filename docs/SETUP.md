# NeuralForge Setup Guide

NeuralForge is a local-first, governed autonomous engineering environment: a Tauri
desktop app (Rust backend + Next.js frontend) where an agent proposes code changes
that are validated, executed with automatic rollback, evidence-logged, and gated by
a promotion controller — all recorded in a hash-chained audit ledger.

## Prerequisites

| Tool | Version | Why |
|---|---|---|
| Rust (rustup) | stable, 1.75+ | Tauri backend, `cargo check`/`cargo test` verification of agent changes |
| Node.js | 18+ | Next.js frontend |
| npm | comes with Node | dependency install / build scripts |
| Ollama | latest, running on `localhost:11434` | local LLM inference for chat, planning, and self-bootstrap (optional for browsing the UI; required for any AI feature) |
| Git | any recent | required by the self-bootstrap flow (local branches only; NeuralForge never pushes) |

Windows additionally needs the Microsoft C++ Build Tools and WebView2 runtime
(standard Tauri prerequisites — see https://tauri.app/start/prerequisites/).

## First run

```bash
npm install          # frontend dependencies
npm run tauri dev    # builds the Rust backend and launches the app with hot reload
```

The first Rust build takes several minutes; later builds are incremental.

## First steps in the app

1. **Open Folder** (top-left) — everything is scoped to this workspace. A
   `.neuralforge/` directory is created inside it holding `index.db` (the single
   SQLite database: search index, tasks, requirements, ledger, evidence,
   promotions, worker profiles) and `memory/` (human-readable agent history).
2. **Settings** — pick model preferences (speed vs quality, cost preference).
   With Ollama running, installed models are detected automatically.
3. **Agent tab** — file edits require a *requirement* (title, intent, acceptance
   criteria); weak requirements are rejected by the validator before any model
   call. Proposed changes always wait for your approval, are verified with a real
   `cargo check` where applicable, and roll back automatically on failure.
4. **Governance tab** — browse the append-only audit ledger, filter by
   correlation ID (one ID threads requirement → tasks → evidence → verdicts),
   and run **Verify chain** to check the hash chain for tampering.
5. **Workers tab** — manage worker profiles and capabilities; reliability scores
   are *derived* from each worker's real verified outcomes, not hand-set.
6. On finished tasks the Agent tab shows the task report: confidence score with
   named factors, failure classification, evidence, promotion verdicts, and — for
   failed/rolled-back tasks — a bounded **Retry** action that prepares a new
   attempt which still requires your approval. Nothing ever executes unattended.

## Verifying an installation

```bash
cd src-tauri && cargo test   # full backend suite (some tests run real cargo builds; allow a few minutes)
npx tsc --noEmit             # frontend type check
npm run build                # frontend production build
```

All Rust tests should pass; a handful marked `#[ignore]` require a live Ollama
instance and are skipped by default.

## Troubleshooting

- **"no workspace open"** — every AI/agent/governance command needs a folder
  opened first; this error is expected until you do.
- **Ollama features failing** — confirm `ollama serve` is running and at least
  one model is pulled (`ollama pull llama3.2` for a small start).
- **`cargo check` verification always failing** — the agent verifies Rust edits
  inside *your workspace's* crate; the workspace must build before the agent can
  verify changes against it.
- **Hash chain reports tampering** — the ledger detected that `index.db` rows
  were edited or deleted outside the app. The data is still readable; the
  verification banner tells you the first broken sequence number.
