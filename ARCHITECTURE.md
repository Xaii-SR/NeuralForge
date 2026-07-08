# Architecture

For developers. This describes how NeuralForge is actually built, not how it was originally planned — where reality diverged from `blueprint.md`, this document (and the underlying `.neuralforge/memory/` files) reflect what shipped.

## Stack

- **Frontend**: Next.js 16 (App Router, static export — `output: "export"`), TypeScript, Tailwind CSS. No SSR, API routes, middleware, or server actions; it's a static site loaded by Tauri via `frontendDist`.
- **Backend**: Tauri 2 / Rust. `main.rs` is initialization-only; `lib.rs` wires plugins, managed state, and the full command registry.
- **Storage**: SQLite (`rusqlite`, bundled with FTS5) — one database per workspace (`.neuralforge/index.db`) plus one global database for model benchmarks.
- **AI**: [Ollama](https://ollama.com) via its HTTP API (`localhost:11434`). No cloud provider has a working client yet — see "Provider system" below.

## Rust module layout

```
src-tauri/src/
  core/
    config.rs    - memory-folder scaffold + ensure_memory_scaffold()
    errors.rs    - AppError (serializes to a string for the frontend), AppResult<T>
    events.rs    - typed event-name constants + emit helpers
    logging.rs   - tracing/tracing-subscriber/tracing-appender, get_recent_logs, export_logs
    state.rs     - AppState { workspace_root }
  filesystem/    - read/write/create/delete/rename, all validated against the open workspace root
  terminal/      - portable-pty backed PTY sessions
  hardware/      - cpu.rs/memory.rs (sysinfo), gpu.rs (DXGI on Windows; empty elsewhere)
  database/      - per-workspace SQLite: indexing, FTS5 search, schema for cache/settings/agent tasks
  ai/
    providers/   - ProviderId registry + the one real client, ollama.rs
    health.rs    - per-provider latency window + failure-cooldown
    model_manager.rs - VRAM estimation from parameter size + quantization
    context.rs   - memory injection + prompt management
    cache.rs     - response cache (prompt+model hash -> response)
    benchmarks.rs - real TPS/latency measurement, global model_benchmarks.db
    router.rs    - preferences, cost estimation, model scoring, auto-selection
  agent/
    mod.rs       - task queue (JSON protocol: id/objective/agent/files/status/verification/rollback)
    planner.rs   - Simulation Mode: proposes changes via the LLM, no filesystem access
    executor.rs  - snapshot + apply + verify (real `cargo check` for .rs) + rollback
    memory.rs    - appends outcomes to .neuralforge/memory/agent_history.md
  extensions/
    manifest.rs  - extension.json schema
    loader.rs    - scans ~/.neuralforge/extensions/, spawns process-isolated plugins
    api.rs       - the mediated JSON-RPC API plugins can call (chat, file ops, search)
  bootstrap/
    selfanalyze.rs - Phase 7: reads a workspace's own memory docs + scans its source files
    suggest.rs      - asks the LLM to pick one file and one improvement (validated against the real file list)
    diff.rs         - small hand-rolled LCS line differ, renders a readable unified diff
    git.rs          - branch creation, commit, real test run (local only, never pushes)
    mod.rs          - propose_self_improvement / apply_self_improvement commands, PR-summary formatting
```

## Frontend layout

```
app/
  layout.tsx   - theme init script (avoids flash of wrong theme), global CSS
  page.tsx     - top-level shell: toolbar, explorer, editor, bottom tabs, chat sidebar
components/
  Editor.tsx, EditorPane.tsx, TabBar.tsx  - Monaco-based multi-tab editor
  FileExplorer.tsx   - lazy-loaded file tree
  Terminal.tsx       - xterm.js PTY client
  LogViewer.tsx       - polls get_recent_logs, exports
  ChatPane.tsx        - chat UI, auto-mode, context injection, caching indicator
  SettingsPanel.tsx   - preferences, benchmarking, cache control
  AgentPanel.tsx      - task creation, plan review, approve/reject
  ExtensionsPanel.tsx - extension manager
  BootstrapPanel.tsx  - Phase 7: analyze/propose, diff review, approve -> branch+test+PR summary
  ui/                 - Spinner, EmptyState, ErrorBanner (shared primitives)
hooks/
  useWorkspace.ts  - shared workspace/tab/file state
  useEvent.ts      - typed wrapper around Tauri's event listen()
  useTheme.ts      - light/dark theme, persisted to localStorage
lib/
  fs.ts, ai.ts, agent.ts, extensions.ts, bootstrap.ts  - typed IPC wrappers over the Rust commands
```

## Key invariants

- **Every filesystem-touching command validates the target path is canonicalized-within the open workspace root** before touching disk. This is the single most important safety property in the codebase and it's enforced independently in `filesystem::validate_within_workspace` (existing paths), `validate_new_path_in_workspace` (not-yet-existing paths — validates the parent instead, since `canonicalize` fails on paths that don't exist), and `agent::executor::apply_and_verify` (same discipline, separately implemented for the agent's write path).
- **`#[tauri::command]` async functions must return `Send` futures.** `rusqlite::Connection` is `Send` but not `Sync`, so a `&Connection` held across an `.await` breaks this. The fix used throughout: database reads happen before the async call, writes happen after, and the async core logic itself never touches a `Connection` — see `ai::chat_or_use_cache` for the canonical example.
- **Pure core, thin Tauri wrapper.** Every module with meaningful logic (filesystem validation, PTY spawn/chat streaming, cache lookup, model scoring, risk estimation, diff/verify/rollback) has that logic in a plain function taking owned values or `&Connection`/`&HealthRegistry` directly — never `State<T>` or `AppHandle`. The `#[tauri::command]` function is a thin shell that extracts state and calls the pure function. This is what makes the test suite possible without a live Tauri runtime (see below).

## AI provider system

`providers::ProviderId` enumerates all 11 providers named in the original blueprint (Ollama + 10 cloud providers). Only Ollama has a working HTTP client (`providers::ollama`). `providers::has_api_key()` always returns `false` — there is no credential storage anywhere in the codebase (matches the blueprint's "never store API keys outside OS keychain/encrypted SQLite," neither of which exists yet). Cloud provider pricing in `router::price_per_1k_tokens` has real numbers, but since no cloud provider is ever "configured," `router::select_model` only ever has Ollama in its candidate pool today. Wiring up a second provider means implementing its client (matching `providers::ollama`'s shape) and building real credential storage — the routing/scoring/cost logic already generically supports it.

## Context intelligence

Each workspace's `.neuralforge/index.db` holds an FTS5 (full-text search) index over chunked source files (`database::indexer`, 40-line windows with 5-line overlap, skips unchanged files via content hash). Queries are converted from natural language into an OR-of-terms FTS5 query with the porter stemmer enabled — FTS5's default MATCH syntax ANDs every bare word together, which fails on almost any real question. `ai::context::build_context_prompt` combines the top search matches with the workspace's `.neuralforge/memory/*.md` files into a single context block, sent as a system message ahead of the user's question.

**Vector/semantic search is not implemented.** The schema has a `chunks.embedding BLOB` column ready for it, but no embedding model was available in the development environment (the local Ollama instance wasn't running with `--embeddings` enabled, and restarting a service the developer had running for other purposes wasn't appropriate to do unilaterally). FTS5 keyword search is the real, working capability.

## The agent's safety model

The agent (`agent/` module) never writes to disk without going through this sequence:

1. **Plan** (`planner::plan_change`) — calls the LLM to propose a complete replacement file and computes a risk summary. This function has no filesystem write capability at all; it's not a permission check, it's an absent capability.
2. **Human approval** — the task sits in `awaiting_approval` until a human clicks Approve or Reject in the UI.
3. **Apply + verify** (`executor::apply_and_verify`) — only reachable after approval. Writes the proposed content, then verifies: real `cargo check` for `.rs` files under a Cargo project, an honest "no automated verification available" note for anything else (never a fabricated pass).
4. **Rollback on failure** — if verification fails, the original content is restored automatically. This is proven by a real test (`agent::executor::tests::broken_rust_change_is_automatically_rolled_back`) that plants a genuine syntax error in a temp Cargo project and asserts both that `cargo check` caught it and that the file was restored.

Only one agent type (Coder) is implemented; the blueprint's Tester/Security/Documentation agents are not built. A Supervisor dispatching across multiple agent types would be a natural extension of the existing task-queue/approval architecture.

## Extension system {#extension-system}

There is no dynamic native code loading and no claim of a security sandbox in the WASM/OS-container sense — that would need a real isolation technology this build doesn't include. Instead: an extension is a manifest (`extension.json`: name, version, author, entry point) plus an entry-point script. NeuralForge spawns it as a **separate child process** and communicates over a line-delimited JSON protocol on stdin/stdout. The plugin can *request* actions (send a chat message, read/write a file, run a terminal command) but never executes them directly — the host validates every request against the same workspace-boundary and approval rules used everywhere else before acting on the plugin's behalf. This is real process isolation with a mediated API surface, which is a meaningfully smaller attack surface than loading arbitrary native code into the host process, but it is **not** a substitute for not running untrusted extensions — a plugin process can still make its own network calls or spawn its own subprocesses outside the mediated API, since there's no OS-level sandboxing (seccomp, AppContainer, etc.) constraining the child process itself.

## Self-bootstrap (Phase 7)

`bootstrap/` lets NeuralForge open itself as a workspace (it's just a folder like any other from the app's perspective), read its own `.neuralforge/memory/architecture.md` and `decisions.md`, and generate improvement suggestions via the same chat/context pipeline used for any other project. Git operations (`bootstrap::git`) are strictly local: create a branch, apply a change, run tests. **Nothing pushes to a remote or opens a pull request automatically** — that's a deliberate line, not a missing feature. See [ROADMAP.md](ROADMAP.md).

## Testing philosophy

Nearly everything in this codebase is tested against real infrastructure, not mocks:

- Real SQLite databases (temp directories, cleaned up after).
- Real PTY spawn/write/read round trips.
- Real HTTP calls to a live local Ollama instance for chat, benchmarking, and agent planning (marked `#[ignore]` so the default `cargo test` run doesn't require Ollama to be running, but run explicitly during development).
- A real temporary Cargo project with a genuine syntax error to prove the agent's rollback mechanism actually works, not just that the code claims to check something.

`tauri::test`'s `MockRuntime` was tried once (for testing `chat_with_model`) and rejected — it crashed the test binary at process launch in this environment (`STATUS_ENTRYPOINT_NOT_FOUND`), confirmed unrelated to app correctness since the real app binary launched fine with identical code otherwise. The fix was structural, not a workaround: pull all meaningful logic out of `#[tauri::command]` functions into plain functions that take owned values instead of `State`/`AppHandle`, so tests never need a Tauri runtime at all.
