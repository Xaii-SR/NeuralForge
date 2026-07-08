# Usage Guide

## Opening a project

Click **Open Folder** in the top bar. This does three things immediately:

1. Sets the workspace root (all file operations, search, and the agent are scoped to this folder — nothing outside it is ever touched).
2. Creates `.neuralforge/memory/` with seven template files if they don't already exist (`architecture.md`, `decisions.md`, `coding_rules.md`, `project_rules.md`, `known_bugs.md`, `agent_history.md`, `current_state.md`). Fill these in — the AI reads them before answering questions.
3. Opens `.neuralforge/index.db`, the SQLite database backing search, caching, and agent task history for this workspace.

## Editing files

Click a file in the explorer to open it in a tab. **Ctrl+S** saves. Unsaved changes show a blue dot on the tab. Closing a tab discards in-memory edits for that file (not the file on disk — nothing is written until you save).

## Chat

The chat panel (right side) talks to a local Ollama model.

- **Auto mode** (default, blue toggle): picks a model automatically based on your Settings preferences (see below) and shows why: *"Selected X because [reason] — speed goal, free cost preference, local/free."*
- **Manual mode**: turn off Auto to pick a specific installed model from the dropdown.
- **Workspace context**: every question automatically pulls in your `.neuralforge/memory/*.md` files plus the top full-text search matches for your question from the indexed workspace. Click **Index Workspace** first (or after making significant changes) so search has something to find — it skips unchanged files on re-runs, so it's cheap to run often.
- **Caching**: identical questions to the same model return instantly from a local cache, tagged **from cache** in the message timestamp row. Clear the cache from Settings if you want fresh answers to a repeated question (e.g. after changing code the question depends on).

If Ollama isn't reachable, the panel shows a clear "Ollama not detected" state — install/start Ollama and reopen the app rather than seeing a cryptic error.

## Settings

Opened via the **Settings** button in the top bar.

- **Goal**: *Fast* ranks installed models by real benchmarked tokens/second (falling back to smallest parameter count if a model hasn't been benchmarked yet); *Best Quality* ranks by largest parameter count.
- **Cost preference**: *Free only*, *Cheap OK*, or *Quality first*. Since only Ollama (local, free) has a working connection today, this mostly affects future cloud-provider routing once one is configured — see [ARCHITECTURE.md](ARCHITECTURE.md).
- **Model Benchmarks**: click **Benchmark** next to any installed model to run a real short prompt and measure tokens/sec and latency. Results persist across restarts (global, not per-workspace) and feed directly into Auto mode's *Fast* ranking.
- **Response Cache**: **Clear Cache** wipes all cached responses for the current workspace.

## The Agent

The **Agent** tab runs a safety-gated coding agent: it proposes changes, but never applies anything without your approval.

1. Enter an **Objective** ("add error handling to the parse function") and a **File path** (relative to the workspace root, e.g. `src/utils.rs`).
2. Click **Plan Task** (or press Enter). The agent reads the file, asks the local model to propose a complete replacement, and computes a risk score (low/medium/high, based on how much of the file actually changes) — **nothing is written to disk at this stage.**
3. Review the **Proposed content** and risk assessment in the task detail pane.
4. **Approve** or **Reject**.
   - Approve: NeuralForge writes the file, then verifies it. For `.rs` files inside a Cargo project, this means running a real `cargo check`. Other file types are written but flagged as unverified rather than falsely marked as checked.
   - If verification fails, **the original file content is automatically restored** — you'll see a red "Rolled back" banner with the failure reason.
   - Reject: the task is marked rejected; nothing is written.

Every finished task (approved or rejected) is appended to `.neuralforge/memory/agent_history.md`, so your project memory reflects what the agent did even without querying the task list.

## Extensions

The **Extensions** tab manages process-isolated plugins (Python execution, web search, file diffing, and anything you add yourself). See [ARCHITECTURE.md](ARCHITECTURE.md#extension-system) for how the isolation model works and what a plugin manifest looks like.

## Terminal & Logs

The bottom panel has four tabs:

- **Terminal**: a real PTY — anything you could run in a normal shell.
- **Logs**: structured, timestamped events from every subsystem, auto-refreshing. **Export** saves the full log file for sharing/debugging.
- **Agent**: the agent task list and detail view described above.
- **Extensions**: the extension manager.

## Keyboard shortcuts

| Shortcut | Where | Action |
|---|---|---|
| `Ctrl+S` | Editor | Save the active file |
| `Enter` | Chat input | Send message |
| `Shift+Enter` | Chat input | Newline without sending |
| `Enter` | Agent objective/file fields | Plan task |
| `Escape` | Settings panel | Close |

## Light / dark mode

Toggle via the ☀/🌙 button in the top bar. Your choice persists across restarts (stored in browser local storage, not per-workspace). The terminal keeps a fixed dark color scheme regardless of app theme — this matches how most terminal emulators behave and avoids fighting ANSI color assumptions baked into shell output.
