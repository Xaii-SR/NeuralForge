# NEURALFORGE — CLAUDE CODE ENGINEERING HANDOFF

**Document Type:** Final Production Handoff  
**Version:** 2.0 (replaces v1.0 audit from 2026-07-16)  
**Date:** 2026-07-23  
**Current Commit at Handoff:** `0c0d535d4aeae7c9395b56ea78bd2e3f5d317291`  
**Branch:** `master`  
**Repository:** `C:\Users\saiah\NeuralForge` — `https://github.com/Xaii-SR/NeuralForge.git`

---

## MANDATORY PRE-READ

This document is the single authoritative engineering reference for Claude Code. Every claim here was verified against the actual repository files, not against prior AI output or stale documentation. If any statement conflicts with `git log`, `cargo test`, or a file on disk, **the repository is correct and this document is stale**. Verify before acting.

**Operating Rules (from `.clinerules`):**

1. UNDERSTAND FIRST. MODIFY SECOND. VALIDATE ALWAYS.
2. Never silently edit files, create files, install dependencies, change configuration, or modify tests.
3. Never delete failing tests, weaken assertions, skip validation, or hide failures.
4. Never rebase, squash, or rewrite git history.
5. Before changing any file: explain CHANGE, WHY, FILES, RISK, EXPECTED RESULT.
6. After changes: provide FILES CHANGED, IMPLEMENTATION, VALIDATION, RESULT.
7. Frozen files must not be modified without explicit Level 5 approval (see §5 below).

---

## TABLE OF CONTENTS

1. [Complete Current Architecture](#1-complete-current-architecture)
2. [Authoritative System Map — Active vs Legacy](#2-authoritative-system-map)
3. [Production Blockers](#3-production-blockers)
4. [Security Review](#4-security-review)
5. [Deployment Review](#5-deployment-review)
6. [Testing Status](#6-testing-status)
7. [Claude Code Final Polish Instructions](#7-claude-code-final-polish-instructions)
8. [Final Verification Checklist](#8-final-verification-checklist)

---

## 1. COMPLETE CURRENT ARCHITECTURE

### 1.1 Tauri Desktop Shell

NeuralForge is a **Tauri 2.11.3** desktop application running on Windows (11, primary target; macOS/Linux not validated). The Tauri configuration is at `src-tauri/tauri.conf.json`.

**Key Tauri Settings:**
- **Product name:** `neuralforge`
- **App version:** `1.4.0` (matches `Cargo.toml` and `package.json`)
- **Identifier:** `com.neuralforge.ide`
- **Window:** 800×600, resizable, not fullscreen by default
- **Security (CSP):** `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: asset: https://asset.localhost; font-src 'self' data:; worker-src 'self' blob:; connect-src 'self' ipc: http://ipc.localhost`
- **Frontend dev URL:** `http://localhost:3000` (Next.js dev server)
- **Before build:** `npm run build` (Next.js static export to `out/`)
- **Plugins:** `tauri-plugin-dialog` v2

**Tauri State (`.manage()`):** 8 managed state objects — `AppState`, `TerminalRegistry`, `HealthRegistry`, `DbState`, `ComposerSessionState`, `ProcessTracker`, `ApprovalRegistry`, `SandboxState`, `OrchestratorState`, `AgentCoreState`.

**IPC surface:** ~130 registered `#[tauri::command]` handlers in `lib.rs`'s `generate_handler![]` macro. The full list is at `src-tauri/src/lib.rs` lines 67–187. All commands are enumerated there — no hidden IPC endpoints exist.

**Streaming:** The backend uses Tauri's native `emit()` event system for token streaming (`AI_RESPONSE_TOKEN` events) — not WebSockets, not SSE-to-frontend. SSE is parsed Rust-side against the actual provider, then re-emitted as Tauri events. This is a proven, stable architecture that should not be restructured.

### 1.2 Rust Backend Structure

**Crate:** `neuralforge` (binary) / `neuralforge_lib` (library)  
**Edition:** 2021, **MSRV:** 1.77.2  
**Async runtime:** Tokio 1.x (`rt-multi-thread`, `macros`, `process`)

**Module tree** (from `src-tauri/src/lib.rs`, declaration order):

| Module | Purpose | Lines (approx) |
|---|---|---|
| `agent/` | Frozen governed execution pipeline (executor, planner, memory) | ~200+ |
| `agent_controller.rs` | Five-phase state machine (Idle→Analyzing→Planning→Executing→Observing→Verifying) | 139 |
| `agent_core/` | Newest agent layer — lifecycle transitions + **Council pass** | Multi-file |
| `agent_v2.rs` | Parallel agent with `ApprovalRegistry`, uses `intelligence::router` | ~150+ |
| `change_executor.rs` | **STUB** — patch generation/application/validation all no-ops | 96 |
| `context_retrieval.rs` | Ranked file discovery for agent context | ~100+ |
| `error_analyzer.rs` | Failure classification + retry suggestions | ~80+ |
| `knowledge_store.rs` | Persistent project memory for agents | ~100+ |
| `multi_agent.rs` | Supervisor + Research/Coding/Testing/Review agents | ~200+ |
| `task_orchestrator.rs` | Multi-step autonomous execution engine | ~150+ |
| `workspace_scanner.rs` | File tree scanning for agent context | ~80+ |
| `ai/` | **AI generation path** (see §1.6) | Multi-file |
| `bootstrap/` | Self-improvement via local git branch | Multi-file |
| `core/` | Errors, logging, build info, state | Multi-file |
| `governance/` | Requirements, ledger (hash-chained), evidence, promotions | Multi-file |
| `database/` | SQLite (indexer, search, resolver, sessions) | Multi-file |
| `extensions/` | Extension loader, manifest, API runtime | Multi-file |
| `filesystem/` | Workspace file read/write/dir ops | Multi-file |
| `hardware/` | GPU/VRAM detection via sysinfo + Windows DXGI | Single-file |
| `intelligence/` | Worker capability matching, reliability scoring (NOT AI model routing) | Multi-file |
| `parsers/` | Language parser registry | Multi-file |
| `planning/` | **Legacy** task DAG planning (dag.rs, planner.rs) | Multi-file |
| `planning_engine.rs` | **Active** task decomposition (`TaskPlan` struct) | Single-file |
| `performance/` | Arena memory management, viewport rendering, soak testing | Multi-file |
| `services/` | Workspace, scheduler, hash, watcher services | Multi-file |
| `terminal/` | PTY management (xterm frontend) | Multi-file |
| `terminal_executor/` | Sandboxed command execution | Multi-file |
| `workspace/` | Semantic search, local embeddings (fastembed) | Multi-file |
| `release_validation.rs` | `#[cfg(test)]` — release validation tests | Single-file |

**Key dependencies** (from `Cargo.toml`):
- `rusqlite` 0.32 with `bundled-full` (no system SQLite required)
- `reqwest` 0.12 with `rustls-tls`, `stream`, `json`, `blocking`
- `fastembed` 5.17.2 (local embedding generation)
- `keyring` 2 (OS credential store for API keys) — **new since v1.0 audit**
- `scraper` + `html2md` (web doc fetching)
- `sysinfo` 0.32 (hardware detection)
- `portable-pty` 0.9 (terminal)
- `specta` 1.0.5 (Rust→TypeScript type generation)
- `sha2` 0.10 (hash-chained ledger)
- `walkdir` 2 + `regex` 1.13 (workspace indexing)

### 1.3 Next.js Frontend Structure

**Framework:** Next.js 16.2.10 (Turbopack)  
**React:** 19.0.0  
**TypeScript:** 5.5.2  
**Styling:** Tailwind CSS 3.4.4  
**Editor:** Monaco Editor via `@monaco-editor/react` 4.7.0  
**Terminal:** `@xterm/xterm` 6.0.0 + `@xterm/addon-fit` 0.11.0  
**Tauri bridge:** `@tauri-apps/api` 2.11.1, `@tauri-apps/plugin-dialog` 2.7.1

**Frontend file structure:**
```
app/
  globals.css          — Tailwind + global styles
  layout.tsx           — Root layout
  page.tsx             — Main application shell

components/
  AgentPanel.tsx        — Agent task management UI
  AgentWorkbench.tsx    — Orchestrator lifecycle panel (analyze/plan/execute/verify phases)
  BootstrapManager.tsx  — Self-improvement branch UI
  BootstrapPanel.tsx    — Self-improvement panel
  ChatPane.tsx          — Main AI chat (most exercised AI UI surface)
  CouncilPanel.tsx      — AI Council panel
  Editor.tsx            — Monaco wrapper with ghost-text + diff
  EditorPane.tsx        — Editor panel container
  ExtensionsPanel.tsx   — Extension management
  FileExplorer.tsx      — File tree
  GovernancePanel.tsx   — Requirement/ledger/promotion UI
  LogViewer.tsx         — Structured log output
  PromptMaker.tsx       — Meta-prompt generator
  ProviderManager.tsx   — Cloud/custom provider CRUD UI
  SessionTabs.tsx       — Chat session tabs
  SettingsPanel.tsx     — Ollama defaults + provider manager host
  TabBar.tsx            — Tab bar component
  TaskReportView.tsx    — Task report drill-in
  Terminal.tsx          — xterm.js PTY terminal
  WorkersPanel.tsx      — Worker capability UI

  composer/             — Multi-file AI composer sub-components
  editor/               — Editor sub-components
  terminal/             — Terminal sub-components
  ui/                   — Shared UI primitives

hooks/
  useComposer.ts        — Composer state hook
  useDebounce.ts        — Debounce utility
  useEvent.ts           — Tauri event listener hook
  useGhostText.ts       — Ghost text completion hook
  useIndexer.ts         — Workspace indexer hook
  useInlineDiff.ts      — Inline diff display hook
  useInlinePrompt.ts    — Ctrl+K inline prompt hook
  useMentionMenu.ts     — @mention autocomplete hook
  usePanelLayout.ts     — Panel layout/resize hook
  useSmartScroll.ts     — Smart scroll behavior hook
  useTerminal.ts        — Terminal integration hook
  useTheme.ts           — Theme hook
  useVersionCache.ts    — Version cache hook
  useWorkspace.ts       — Workspace state hook

lib/
  agent.ts              — Agent IPC bindings
  ai.ts                 — AI chat IPC bindings (frozen contract)
  bootstrap.ts          — Bootstrap IPC bindings
  buildInfo.ts          — Build info bridge
  council.ts            — Council IPC bindings
  extensions.ts         — Extensions IPC bindings
  fs.ts                 — Filesystem IPC bindings
  governance.ts         — Governance IPC bindings
  language.ts           — Language detection
  orchestrator.ts       — Orchestrator IPC bindings
  providers.ts          — Provider CRUD IPC bindings
  store.ts              — Simple state store
  events/               — Event bus
  utils/                — Utilities
```

**State management pattern:** NeuralForge has **no global state library** (no Redux, Zustand, Jotai). Each component manages its own state via `useState` and custom hooks. Cross-component communication uses:
- `window.dispatchEvent`/`window.addEventListener("nf_settings_updated", …)` for settings
- Tauri's native `listen()`/`emit()` for backend→frontend streaming

This is intentional — do **not** introduce a state management library unless there is a concrete, proven need. The current pattern works.

### 1.4 React Component Architecture

**Component maturity tiers (verified this handoff):**

**PRODUCTION (battle-tested, do not restructure):**
- `ChatPane.tsx` — Most exercised AI surface. Stable contract: `chat_with_model(request_id, model, messages)` + `AI_RESPONSE_TOKEN` events.
- `Editor.tsx` / `EditorPane.tsx` — Monaco with ghost-text, diff decorations, Ctrl+K inline edit.
- `SettingsPanel.tsx` — Ollama defaults + ProviderManager host.
- `Terminal.tsx` — xterm.js PTY. Stable.
- `GovernancePanel.tsx` / `WorkersPanel.tsx` — Production per prior audit.
- `PromptMaker.tsx` — Routes through real chat pipeline.
- `FileExplorer.tsx` — File tree. Stable.
- `composer/` — Multi-file AI composer. Production.

**NEW (functional, less battle-tested):**
- `ProviderManager.tsx` — Provider CRUD UI. Rust side unit-tested. UI not yet verified end-to-end against real Tauri runtime with actual cloud providers. **Requires verification.**
- `AgentWorkbench.tsx` — Orchestrator lifecycle UI. Depends on `change_executor.rs` stub (§3). Will not produce real file changes.

**EXPERIMENTAL (partially wired):**
- `AgentPanel.tsx` — Agent task management. Partially wired.
- `CouncilPanel.tsx` — AI Council UI. **Requires verification** of what's actually functional.
- `BootstrapPanel.tsx` / `BootstrapManager.tsx` — Self-improvement. Scope is local-only, "never pushes" per design.

### 1.5 Monaco Editor Integration

Monaco Editor is wrapped in `components/Editor.tsx` and `EditorPane.tsx`. It provides:

- **Ghost-text completions:** FIM (Fill-in-the-Middle) via `ai::completion` module. Currently **Ollama-only** — calls `providers::ollama` directly, not through `provider_router`. This is a known gap, not a bug. Consistent with the session scope that only migrated the main chat path.
- **Inline diff decorations:** Shows AI-proposed changes as colored diffs within the editor.
- **Ctrl+K inline edit:** Streams AI edits inline via `ai::inline::stream_inline_edit`. Also **Ollama-only** — same gap as ghost-text.

**IPC commands for editor AI features:**
- `fetch_ghost_suggestion`
- `get_ghost_text_prediction`
- `get_prediction_with_fim`
- `store_prediction_result`
- `request_async_completion`
- `stream_inline_edit`

### 1.6 SQLite/Database Architecture

**Database:** One `index.db` per workspace, not a single global DB. Opened via `database::open_for_workspace()`.

**Schema** (additive-only, `CREATE TABLE IF NOT EXISTS`):

| Table | Purpose |
|---|---|
| `files` | Indexed workspace files (path, content_hash, indexed_at, size, language, line_count) |
| `chunks` | Code chunks with optional embedding BLOBs (FK → files) |
| `chunks_fts` | FTS5 virtual table for full-text search (porter + unicode61 tokenizer) |
| `settings` | Generic key/value store (AI prefs, provider configs, active model per task type) |
| `response_cache` | AI response cache keyed on `(prompt_hash, model)` |
| `sessions` | Chat sessions with messages |
| `agent_tasks` | Agent task records (objective, agent, status, file_path, risk, verification, rollback) |
| `task_dags` | Task DAG definitions (requirement_id, version, correlation_id) |
| `requirements` | Governance requirements (title, intent, acceptance_criteria, status) |
| `requirement_history` | Versioned requirement history (FK → requirements) |
| `ledger_entries` | Hash-chained governance ledger (event_type, payload, prev_hash, entry_hash) |
| `evidence` | Task evidence records with insertion sequence (FK-bounded) |
| `evidence_sequence` | Monotonic evidence sequence counter |
| `promotion_requests` | Promotion records (FK → evidence, status, timestamps) |
| `symbols` | Code symbols (name, kind, line range, visibility, signature, documentation) |
| `dependencies` | Code dependency graph (source→target file/symbol) |

**Concurrency:** `DbState` wraps the connection in `Mutex<Option<Connection>>`. Every command that accesses the DB locks this mutex. This is a known bottleneck under heavy parallel access but works correctly for the current single-user desktop model.

**Migration strategy:** Additive-only — `ALTER TABLE ADD COLUMN` for new columns. No destructive migrations. Per `.clinerules`, schema changes are non-fatal (if column already exists, skip). This means old databases remain compatible.

**Embeddings:** The `chunks.embedding` column stores BLOB data generated locally by `fastembed`. No cloud embedding API calls are made.

### 1.7 IPC Architecture

**Pattern:** All frontend↔backend communication goes through Tauri's `invoke()` / `listen()` / `emit()` system. There is:
- No separate HTTP server
- No WebSocket server
- No side-channel IPC

**Command registration:** All 130+ commands are listed in `lib.rs`'s `generate_handler![]`. To find any command's implementation:
1. Find the command name in `lib.rs`
2. The module path prefix tells you which file to open (e.g., `ai::chat_with_model` → `src-tauri/src/ai/mod.rs`)

**Streaming pattern:**
1. Frontend calls `invoke("chat_with_model", { requestId, model, messages })`
2. Backend spawns an async task, streams from the provider, and calls `app_handle.emit("AI_RESPONSE_TOKEN", payload)` per token
3. Frontend listens via `listen("AI_RESPONSE_TOKEN", callback)`

**Event types used for streaming:**
- `AI_RESPONSE_TOKEN` — chat token streaming
- `inline-stream` — Ctrl+K inline edit streaming
- `inline-diff-stream` — diff streaming
- Orchestrator state events (task lifecycle)

### 1.8 AI Provider Architecture

**Full path (chat):**
```
Frontend: invoke("chat_with_model", { requestId, model, messages })
    ↓
ai::mod.rs::chat_with_model
    ↓
ai::provider_router::resolve_provider_for_model(conn, model)
    ↓
ai::provider_router::adapter_kind_for(provider_type)
    ↓
Ollama branch → ai::chat_with_model_core (VRAM gate, health check)
OpenAI-compat branch → providers::openai_compatible (shared SSE client)
Unimplemented → explicit error
    ↓
Provider HTTP API
    ↓
Token streaming → app_handle.emit("AI_RESPONSE_TOKEN", payload)
```

**Three distinct "router" concepts (DO NOT CONFUSE):**

1. **`ai::provider_router`** (`src-tauri/src/ai/provider_router.rs`) — **THE ACTIVE AI ROUTER.** Cross-provider adapter dispatch layer. This is the one Claude should modify for any AI routing changes.

2. **`ai::router`** (`src-tauri/src/ai/router.rs`) — **LEGACY.** Ollama-only model scoring/cost heuristics (parameter-count estimates). Predates the provider system. Used by `auto_select_model`. Do not expand — eventually should be folded into `provider_router`.

3. **`intelligence::router`** (`src-tauri/src/intelligence/router.rs`) — **DIFFERENT CONCEPT.** Worker/agent capability matching for task assignment. Not related to AI model provider selection at all.

**Provider Registry** (`ai::provider_registry`):
- SQLite-backed CRUD for provider configs
- `ProviderConfig { id, name, provider_type, base_url, api_key, models, enabled, is_default, capabilities, created_at }`
- A default Ollama entry (`id = "default-ollama"`) always exists
- CRUD commands: `list_provider_configs`, `add_provider_config`, `update_provider_config`, `delete_provider_config`, `set_default_model`, `get_model_config`

**Credential Storage** (`ai::credential_store`):
- **FIXED since v1.0 audit.** API keys are now stored in the OS credential store (Windows Credential Manager / macOS Keychain / Linux libsecret) via the `keyring` crate
- `ProviderConfig.api_key` in SQLite still holds the field at the struct level but `store_api_key()`/`load_api_key()` delegate to the OS keychain
- Keyed by `provider_id` under service name `"neuralforge-provider-api-key"`
- Empty keys are no-ops (not written to keychain)
- Tests: 4 total (2 `#[ignore]` requiring real OS keychain, 2 unconditional)
- **Requires verification** that the migration from plain-text to keyring is complete — check whether `add_provider_config`/`update_provider_config` actually call `credential_store::store_api_key()`

**Adapters — four, all with real implementations:**

| Adapter | File | Lines | Scope | Status |
|---|---|---|---|---|
| Ollama | `ai/providers/ollama.rs` | ~200+ | Local Ollama only | **PRESERVE** — zero-config default |
| OpenAI-compatible | `ai/providers/openai_compatible.rs` | ~400+ | 12+ provider types | **Active** — universal compatibility layer |
| Anthropic | `ai/providers/anthropic.rs` | 273 | Anthropic Messages API | **Active** — native adapter, real implementation |
| Gemini | `ai/providers/gemini.rs` | 315 | Google Gemini API | **Active** — native adapter, real implementation |

`provider_router.rs` line 12 imports all four: `use crate::ai::providers::{anthropic, gemini, ollama, openai_compatible};`

**Provider types routed through OpenAI-compatible adapter:**
OpenAI, OpenRouter, DeepSeek, Groq, Together AI, Fireworks, DeepInfra, LM Studio, vLLM, llama.cpp, `openai_compatible`, `custom`, Mistral

**Known issue — Cohere:** Selecting `"cohere"` in the UI routes through the OpenAI-compatible adapter (catch-all arm). Cohere's real API is NOT OpenAI-shaped — requests will fail at runtime. **Requires Repository Verification** of current Cohere API docs. Fix: either implement native Cohere adapter or remove `"cohere"` from the frontend dropdown.

**Anthropic and Gemini adapters** are real, working implementations — NOT stubs. The v1.0 audit incorrectly stated they were "unimplemented." They are live and wired through `provider_router`. Handle them as production code.

**Not yet routed through provider_router (Ollama-only gap):**
- `ai::inline::stream_inline_edit` (Ctrl+K) — calls `providers::ollama` directly
- `ai::completion` (ghost-text/FIM) — calls `providers::ollama` directly

### 1.9 Agent Systems — Real Execution Chain

The agent execution flows through multiple layers. Here is the authoritative execution chain:

```
UI (AgentWorkbench.tsx / AgentPanel.tsx)
    ↓ invoke()
agent_v2.rs (start_agent_task / approve_agent_task / reject_agent_task)
    ↓ delegates to
task_orchestrator.rs (TaskOrchestrator — analyze → plan → execute → observe → verify)
    ↓ reads workspace context from
context_retrieval.rs (RankedFile discovery)
workspace_scanner.rs (file tree scanning)
    ↓ executes through
terminal_executor.rs (sandboxed command: cargo check, cargo test, etc.)
    ↓ failures analyzed by
error_analyzer.rs (DiagnosticFailure classification → retry suggestion)
    ↓ successful results stored in
knowledge_store.rs (persistent project memory)
    ↓ file changes attempted through
change_executor.rs (**STUB** — generates empty patches)
    ↓ governance recorded in
governance/ (requirements → ledger → evidence → promotion)
```

**System-by-system classification (see also §2):**

**`agent/` (executor.rs, planner.rs, memory.rs):**
- The original governed execution pipeline: task → execute → verify → rollback
- Tied into `governance::ledger` / `governance::promotion`
- **FROZEN** per `.clinerules` — do not modify

**`agent_controller.rs`:**
- Simple 5-phase state machine: `Idle → Analyzing → Planning → Executing → Observing → Verifying → Completed/Failed`
- Provides `AgentContext` and `AgentController::analyze/plan/execute/observe/verify`
- Used by `multi_agent.rs`
- **LEGACY** — superseded by `agent_v2.rs` + `task_orchestrator.rs`

**`agent_v2.rs` (ACTIVE):**
- Parallel agent with `AgentState` enum
- Uses `intelligence::router` for worker capability matching
- Manages `ApprovalRegistry` as Tauri state
- Registered commands: `start_agent_task`, `approve_agent_task`, `reject_agent_task`
- This is the **primary agent entry point** from the UI

**`task_orchestrator.rs` (ACTIVE):**
- The real execution engine: creates tasks, manages lifecycle state, runs analysis/planning/execution/observation/verification
- Recovery logic with configurable `max_recovery_attempts` (default: 3)
- Full test coverage (7 tests including `recovery_on_failure`, `max_recovery_fails`)
- This is where agent work actually happens

**`agent_core/` (NEWEST):**
- Lifecycle transitions (`agent_lifecycle_transition`) + Council pass (`run_council_pass`)
- Separate Tauri state: `AgentCoreState`
- Active development target for Council integration

**`multi_agent.rs` (ACTIVE):**
- Supervisor + Research/Coding/Testing/Review specialized agents
- Built on `agent_controller` (legacy) + `task_orchestrator` (active) + `knowledge_store`
- Highest-level composed agent layer

### 1.10 Planning Systems — Authoritative Classification

**`planning/` (dag.rs, planner.rs):**
- **LEGACY.** Task DAG planning with cycle/orphan detection, topological ordering, failure-blocks-dependents-not-siblings semantics
- Commands: `plan_requirement_dag`, `get_dag`, `get_dag_runnable_tasks`
- Still registered in `lib.rs` but superseded by `planning_engine.rs`

**`planning_engine.rs`:**
- **ACTIVE.** Newer task decomposition with `TaskPlan { task_description, objective, affected_files, subtasks, risks, verification, unknown_information, complexity }`
- Used by `task_orchestrator.rs` and `change_executor.rs`

### 1.11 Retrieval Systems

**`database/` (indexer.rs, search.rs, resolver.rs):**
- Workspace file indexing (walkdir → files table + chunks)
- FTS5 keyword search via `chunks_fts`
- File path resolution for `@mention` support
- **FROZEN** (`indexer.rs`, `resolver.rs`, `search.rs` per `.clinerules`)

**`workspace/` (embeddings.rs, search.rs):**
- `fastembed`-based local embedding generation
- Semantic codebase query via `query_codebase_semantic`
- `build_local_index`, `generate_local_embeddings`

**`context_retrieval.rs`:**
- Ranked file discovery for agent context
- Standalone module, used by agent systems

**`ai/context.rs`:**
- Chat-time context prompt building from workspace index
- Uses `database::search` results to construct enriched prompts

### 1.12 Extension Systems

**`extensions/` (mod.rs, api.rs, loader.rs, manifest.rs):**
- Extension discovery via directory scanning
- `ensure_bundled_extensions` for built-in extensions
- Enable/disable state persisted as JSON
- Extension runtime via `run_extension` (JSON-in/JSON-out API)
- Uninstall with path traversal protection (canonicalize checks)
- Commands: `list_extensions`, `set_extension_enabled`, `uninstall_extension`, `run_extension`

### 1.13 Global Application State

NeuralForge has three tiers of state persistence:

**Tier 1 — SQLite (per-workspace, durable):**
- `settings` key/value table — AI prefs, provider configs, active model per task type
- Chat sessions + messages (`sessions` table)
- Agent tasks, evidence, ledger entries
- All database state is per-workspace — closing a workspace persists everything

**Tier 2 — OS Credential Store (per-machine, durable):**
- `ai::credential_store` stores API keys in Windows Credential Manager / macOS Keychain / Linux libsecret
- Keys survive app restarts and workspace changes
- No plaintext keys in SQLite

**Tier 3 — In-memory (volatile):**
- Tauri managed state objects: `AppState`, `TerminalRegistry`, `HealthRegistry`, `DbState`, `ComposerSessionState`, `ProcessTracker`, `ApprovalRegistry`, `SandboxState`, `OrchestratorState`, `AgentCoreState`
- All lost on app restart — this is intentional for runtime caches (health cooldown state, active PTY sessions, in-flight composer processes)
- `HealthRegistry` cooldown state is the only state with user-visible consequences on loss — a degraded provider recovers immediately on restart since cooldown is wiped

**Startup crash risk — corrupted state:**
- SQLite corruption: `rusqlite` with `bundled-full` uses SQLite's built-in WAL recovery. Malformed DB would cause `open_for_workspace` to fail with a readable error, not a crash. The app would fail to open that workspace but the binary itself would not crash.
- Credential store corruption: `keyring::Entry::get_password()` returns `Err` on malformed entries; `load_api_key` returns empty string (graceful degradation).
- No startup-time state validation beyond `enforce_environment_gate()` (bootstrap checks). No automatic state repair/recovery exists — malformed settings JSON would require manual `index.db` editing.

**No dedicated migration was introduced for provider configs** — an intentional choice to avoid schema changes.

**Frontend state:** `lib/store.ts` provides a thin `localStorage` wrapper (`nf_app_config` key) for client-side AI config defaults (provider, endpoint, model, temperature, context, effort). This is separate from the Rust-side provider registry and acts as a local cache. An API key migration path exists: `migrateConfig()` moves old `apiKey` fields to `apiKeyRef` and backs up the original to `nf_api_key_backup` in localStorage. No `IndexedDB` or other structured frontend storage exists.

### 1.14 Workspace Systems

**`filesystem/`:**
- Core file operations: open_workspace, read_dir, read_file, write_file, create_file, create_dir, delete_path, rename_path
- Workspace opening is the entry point for DB initialization

**`workspace_scanner.rs`:**
- File tree scanning for agent context
- Used by `agent_controller`

**`services/`:**
- `workspace_service.rs` — workspace lifecycle management
- `scheduler_service.rs` — background task scheduling
- `hash_service.rs` — content hashing for file change detection
- `watcher_service.rs` — filesystem change watching

**`parsers/`:**
- Language parser registry for symbol extraction
- Used by the database indexer for code intelligence

---

## 2. AUTHORITATIVE SYSTEM MAP

### CRITICAL: Before modifying ANY file, read this section.

NeuralForge has accumulated overlapping systems. Claude MUST know which to modify and which to ignore. Modifying the wrong system wastes tokens and risks breaking frozen code.

### 2.1 Agent Systems

| System | Path | Status | Claude Should... |
|---|---|---|---|
| `agent/` | `src-tauri/src/agent/` | **FROZEN** (governed execution pipeline) | **NEVER MODIFY** — requires Level 5 approval |
| `agent_controller.rs` | `src-tauri/src/agent_controller.rs` | **LEGACY** (simple 5-phase state machine) | **DO NOT MODIFY** — superseded by `agent_v2` and `agent_core` |
| `agent_v2.rs` | `src-tauri/src/agent_v2.rs` | **ACTIVE** (worker-matching agent with approval flow) | **CAN MODIFY** — this is the active agent for task execution |
| `agent_core/` | `src-tauri/src/agent_core/` | **NEWEST** (lifecycle + Council pass) | **PREFERRED TARGET** for agent improvements |
| `multi_agent.rs` | `src-tauri/src/multi_agent.rs` | **ACTIVE** (Supervisor + 4 specialized agents) | **CAN MODIFY** — top-level orchestration layer |

**What Claude should NOT do:**
- Create a fifth agent system
- Modify `agent/executor.rs` or `agent/planner.rs`
- Spend time analyzing `agent_controller.rs` in depth — it's a dead end
- Try to "unify" all agent systems — they serve different purposes and explicit reconciliation is a separate Level 4 task

**What Claude SHOULD do:**
- Modify `agent_v2.rs` and `agent_core/` only when directly required to complete the documented `change_executor.rs` execution path or resolve verified execution bugs
- Extend `multi_agent.rs`'s specialized agents only when needed for the `change_executor.rs` execution path
- Wire `change_executor.rs` into the active agent path (see §3)
- Do not perform broad architectural rewrites, consolidation, modernization, or unprompted refactoring of these modules

### 2.2 Planning Systems

| System | Path | Status | Claude Should... |
|---|---|---|---|
| `planning/` | `src-tauri/src/planning/` | **LEGACY** (DAG planning) | **DO NOT MODIFY** — superseded |
| `planning_engine.rs` | `src-tauri/src/planning_engine.rs` | **ACTIVE** (TaskPlan decomposition) | **MODIFY THIS ONE** for any planning changes |

**Do not create a third planning system.**

### 2.3 AI Router Systems

| System | Path | Status | Claude Should... |
|---|---|---|---|
| `ai::provider_router` | `src-tauri/src/ai/provider_router.rs` | **ACTIVE** (cross-provider dispatch) | **MODIFY THIS ONE** for any AI routing changes |
| `ai::router` | `src-tauri/src/ai/router.rs` | **LEGACY** (Ollama-only scoring) | **DO NOT EXPAND** — eventually fold into provider_router |
| `intelligence::router` | `src-tauri/src/intelligence/router.rs` | **DIFFERENT CONCEPT** (worker matching) | Not related to AI routing at all |

### 2.4 Performance Systems

| System | Path | Status | Notes |
|---|---|---|---|
| `performance/` | `src-tauri/src/performance/` | **NEW** | arena_v5 (memory), viewport (rendering), soak_test (stress) — not present in v1.0 audit. **Requires verification** of what these do and whether they're wired. |

### 2.5 Bootstrap System

| System | Path | Status | Notes |
|---|---|---|---|
| `bootstrap/` | `src-tauri/src/bootstrap/` | **ACTIVE** | Self-improvement via local git branch. `enforce_environment_gate()` runs at startup. "Never pushes" per design. |

---

## 3. PRODUCTION BLOCKERS

### BLOCKER 1: `change_executor.rs` is a complete stub

**Location:** `src-tauri/src/change_executor.rs` (96 lines)  
**Current behavior:** Every function returns empty/no-op results:
- `ChangeGenerator::generate_patches()` → `Ok((vec![], vec![]))`
- `PatchApplier::apply()` → `Ok(vec![])`
- `PatchApplier::rollback()` → `Ok(false)`
- `PatchValidator::validate()` → `Ok(vec![])`
- `DiffGenerator::generate_diff()` → empty `UnifiedDiff`

**Why it matters:** The entire autonomous agent workflow (`AgentWorkbench.tsx` → `task_orchestrator` → `change_executor`) cannot produce real file changes. The planning, approval, and lifecycle tracking are all real and wired — but the final step that actually modifies code is a no-op.

**Root cause:** This was an intentional scaffold-first commit (`24bc677`, "Change Executor scaffold"). The types (`Patch`, `PatchOperation`, `UnifiedDiff`, `ApplyResult`) are well-designed — they just need real implementations.

**Recommended fix:** Implement `generate_patches` to produce real `Patch` objects from a `TaskPlan`. Start with simple file-level text replacement. Then implement `PatchApplier::apply` to write the changes to disk. Keep `rollback` as a secondary priority — it can use file backups.

**Risk level:** **CRITICAL** — blocks the core autonomous workflow end-to-end.

---

### BLOCKER 2: Ghost-text and inline-edit bypass the provider router

**Location:**
- `src-tauri/src/ai/completion.rs` — ghost-text/FIM pipeline
- `src-tauri/src/ai/inline.rs` — Ctrl+K inline edit

**Current behavior:** Both call `providers::ollama` directly instead of going through `ai::provider_router`. This means these features are **Ollama-only** regardless of what cloud providers the user has configured.

**Why it matters:** If a user configures a cloud provider (e.g., DeepSeek for coding), the main chat will use it but ghost-text completions and inline edits will silently fall back to Ollama. This creates an inconsistent experience.

**Recommended fix:** Refactor `completion.rs` and `inline.rs` to call `provider_router::resolve_provider_for_model` and `provider_router::stream_cloud_chat` (or equivalent) instead of calling `providers::ollama` directly. This was explicitly deferred during the Phase 1 provider routing work — it's the natural Phase 2.

**Risk level:** **HIGH** — inconsistent user experience, but the chat path (highest-traffic surface) already works correctly.

---

### BLOCKER 3: Cohere provider type routes through wrong adapter

**Location:**
- `src-tauri/src/ai/provider_router.rs` — `adapter_kind_for()` catch-all arm
- `components/ProviderManager.tsx` — provider type dropdown

**Current behavior:** Selecting `"cohere"` in the UI routes through the OpenAI-compatible adapter (catch-all in `adapter_kind_for`). Cohere's real API is NOT OpenAI-shaped — different request/response envelope, different streaming format. Requests will fail at runtime with confusing errors.

**Why it matters:** Users can configure a Cohere provider with a valid API key and it will quietly fail with HTTP errors, not a clean "unsupported" message like Anthropic/Gemini produce.

**Recommended fix:** Two options:
1. **Preferred (small scope):** Remove `"cohere"` from the frontend's provider type dropdown until a native adapter exists. This is a one-line fix and prevents the broken path.
2. **Larger scope:** Implement a native Cohere adapter if the Cohere API is genuinely different enough to require it. Verify against current Cohere API docs first.

**Risk level:** **MEDIUM** — only affects users who specifically try to configure Cohere.

---

### BLOCKER 4: Database concurrency under parallel access

**Location:** `src-tauri/src/database/mod.rs` — `DbState { conn: Mutex<Option<Connection>> }`  
**Current behavior:** A single `Mutex` guards the main database connection. Any command that touches the DB through this path blocks all other DB commands targeting the same connection.

**Mitigation already in place (verified v1.3.0+):** `filesystem::open_workspace` spawns background indexing on a **separate thread with its own `Connection`** to the same `index.db` file — see the `background_indexing_connection_does_not_lock_out_the_ui_connection` test in `filesystem/mod.rs`. This means the indexer no longer contends with chat/session DB operations through the shared `DbState` mutex.

**Current risk assessment:** The single `Mutex<Connection>` design is a **known limitation**, not a current production blocker. For a single-user desktop IDE, the mutex contention is acceptable. Multi-agent concurrent DB writes are not yet a real workload — the agent orchestrator is single-threaded and `change_executor.rs` is still a stub (§3, Blocker 1).

**Future concern:** If multiple agent workers begin performing concurrent DB writes (e.g., parallel evidence recording, simultaneous knowledge store updates), the single mutex becomes a bottleneck. At that point, switching to WAL mode with a connection pool (e.g., `r2d2-sqlite`) is the recommended solution.

**Risk level:** **LOW** — acceptable for current single-user workloads. Reclassify as MEDIUM when multi-agent concurrent writes become a real feature.

---

### BLOCKER 5: Migration validation not automated

**Location:** `src-tauri/src/database/mod.rs` — `ALTER TABLE ADD COLUMN` pattern  
**Current behavior:** Schema migration is additive and uses `IF NOT EXISTS` guards. However, there is no automated test that opens a database from N versions ago and verifies it still works after migration.

**Why it matters:** A migration that breaks backward compatibility would only be discovered when a user with an old workspace opens the app — not in CI.

**Recommended fix:** Add a migration test that checks out old DB fixtures and runs `open_for_workspace` against them, verifying all tables are queryable.

**Risk level:** **MEDIUM** — no known breakage, but the safety net is missing.

---

### BLOCKER 6: IPC validation is manual

**Location:** `src-tauri/src/lib.rs` — `generate_handler![]` macro  
**Current behavior:** Command signatures are not validated against frontend TypeScript types at compile time. `specta` is a dependency but types appear to be manually maintained in `lib/generated_types.d.ts`.

**Why it matters:** If a Rust command signature changes (e.g., parameter renamed) but the TypeScript call site isn't updated, the failure is a runtime `invoke()` error, not a compile-time error.

**Recommended fix:** Verify whether `specta` type export is working. If not, enable it and add a CI step that checks for type drift (`diff` on generated types vs committed types).

**Risk level:** **LOW** today (commands are stable), but increases with each new command.

---

### BLOCKER 7: Intermittent test flake (unresolved)

**Location:** Unknown — has surfaced in `completion::pipeline_hit` and `indexer::index_workspace_reindexes_after_file_change` in prior runs  
**Current behavior:** Roughly 1-in-3 full-suite runs show failures in unrelated tests that pass in isolation. Not reproduced in the v1.0 audit's verification run (291/291 clean). Still unresolved.

**Why it matters:** An uncharacterized flake erodes trust in the test suite. If CI starts failing intermittently, it trains developers to ignore test failures.

**Recommended fix:** Characterize the flake. Run `cargo test` in a loop (50+ iterations) and capture which test fails. Check for shared mutable state, file system contention, or DB locking between tests.

**Risk level:** **MEDIUM** — not currently blocking, but an uncharacterized concurrency bug could be a real issue.

---

## 4. SECURITY REVIEW

### 4.1 Credential Storage — RESOLVED

**Status:** ✅ **FIXED since v1.0 audit**

**What changed:** The `ai::credential_store` module (`src-tauri/src/ai/credential_store.rs`) now stores API keys in the OS credential store via the `keyring` crate (v2):
- Windows: Credential Manager
- macOS: Keychain
- Linux: libsecret

**Implementation:**
- `store_api_key(provider_id, api_key)` — writes to OS keychain under service `"neuralforge-provider-api-key"`
- `load_api_key(provider_id)` — reads from OS keychain, returns empty string on miss (not an error)
- `delete_api_key(provider_id)` — removes from OS keychain, no-op if absent

**What needs verification:**
- Confirm that `add_provider_config` and `update_provider_config` actually call `credential_store::store_api_key()` and do NOT persist the raw key in the `settings` table
- Confirm that `load_api_key()` is called when building the provider config for chat requests
- The `ProviderConfig` struct still has an `api_key` field — verify it's empty/redacted in SQLite

### 4.2 SQLite Security

**Database location:** One `index.db` per workspace, inside the workspace directory.  
**Schema:** No encryption at rest. The database file is readable by any process with filesystem access.  
**Risk:** If a workspace is on a shared filesystem, the `settings` table (even with API keys moved to keychain) still contains provider configuration metadata that reveals which services are configured.  
**Mitigation:** This is inherent to local-first desktop architecture. The credential migration to OS keychain resolves the highest-risk item (raw API keys). Remaining metadata exposure is low-severity.

### 4.3 IPC Attack Surface

**CSP:** `script-src 'self'` — no inline scripts, no eval. This is a strong CSP.  
**Tauri IPC:** All commands are explicitly registered in `generate_handler![]`. No dynamic command registration.  
**Frontend→Backend:** `invoke()` is the only bridge. The frontend cannot execute arbitrary Rust code.  
**Backend→Frontend:** `emit()` events are the only push channel. The frontend must explicitly `listen()` for each event type.  
**Risk assessment:** The IPC surface is well-constrained. The main risk is a command that accepts user-controlled paths (e.g., `read_file`, `write_file`) without path traversal validation. **Requires verification** that `filesystem/` commands canonicalize paths and reject paths outside the workspace root.

### 4.4 Filesystem Permissions

**Module:** `src-tauri/src/filesystem/`  
**Commands:** `open_workspace`, `read_dir`, `read_file`, `write_file`, `create_file`, `create_dir`, `delete_path`, `rename_path`  
**Risk:** These commands operate on the opened workspace directory. If path traversal is not validated in every command, an attacker could read/write arbitrary files on the system.  
**Requires verification:** Check each filesystem command for path canonicalization and workspace-root-bounded checks.

### 4.5 Terminal Sandbox

**Module:** `src-tauri/src/terminal_executor.rs`  
**Implementation:** `SandboxConfig` with:
- **Allowlist:** `cargo`, `npm`, `pnpm`, `node`, `rustc`, `git`, `echo`
- **Denylist:** `rm -rf /`, `rm -rf ~` (substring match — could be bypassed)
- **Max timeout:** 300 seconds
- **Requires approval:** `true` by default

**Commands:** `execute_sandboxed_command`, `allowlist_add`, `denylist_add`

**Risks:**
1. **Denylist is substring-based** — `rm -rf /` is blocked but `rm -rf --no-preserve-root /` or `find / -delete` would bypass it. The denylist is not a security boundary.
2. **Allowlist is configurable** — `allowlist_add` can add arbitrary commands. Depends on approval gate.
3. **Separate from interactive terminal** — `Terminal.tsx` uses `portable-pty` and is intentionally unrestricted. The sandbox only applies to agent-initiated commands.

**Assessment:** The sandbox is a convenience/safety layer, not a security boundary. Do not rely on it for sandboxing untrusted code.

### 4.6 Provider API Handling

**API keys in transit:** All provider HTTP calls use `reqwest` with `rustls-tls`. Keys are sent as `Authorization: Bearer <key>` headers over HTTPS (no plain HTTP option exists in the code).  
**Key exposure in logs:** **Requires verification** — check that `tracing` doesn't log API keys or `Authorization` headers.  
**Provider health checks:** `health_check()` pings provider endpoints. No keys are logged during health checks.  
**Provider model discovery:** `list_models()` calls provider `/v1/models` or `/api/tags`. Keys are sent in standard auth headers, not in URL parameters.

### 4.7 Secrets Management Summary

| Concern | Status | Action Needed |
|---|---|---|
| API keys in SQLite plaintext | ✅ Fixed (OS keychain) | Verify migration completeness |
| API keys in transit | ✅ HTTPS only (rustls) | Verify no plain HTTP codepath |
| API keys in logs | ⚠️ Requires verification | Audit tracing calls in provider code |
| Provider metadata exposure | ⚠️ Low risk | Acceptable for local-first desktop |
| Filesystem path traversal | ⚠️ Requires verification | Audit filesystem commands |
| Terminal sandbox bypass | ⚠️ Known limitations | Documented, not a security boundary |
| CSP | ✅ Strong (`script-src 'self'`) | Maintain |
| IPC surface | ✅ Explicit registration | Maintain |

---

## 5. DEPLOYMENT REVIEW

### 5.1 Tauri Bundling

**Configuration:** `src-tauri/tauri.conf.json` → `bundle` section  
**Bundle targets:** `"all"` — produces `.msi`/`.nsis` (Windows), `.dmg` (macOS), `.deb`/`.AppImage` (Linux)  
**Icons:** 32×32, 128×128, 128×128@2x, `.icns`, `.ico` — present in `src-tauri/icons/`  
**Windows specifics:**
- NSIS installer with custom icon
- WiX MSI with `en-US` language
- **No code signing configured** — installers will show "untrusted publisher" warnings
- **No auto-updater configured** — no `tauri-plugin-updater` in `Cargo.toml`

### 5.2 Installers

**NSIS:** Nullsoft Scriptable Install System — produces `.exe` installer  
**WiX:** Windows Installer XML — produces `.msi` installer  
**Status:** Both configured but **untested** — no evidence of actual installer generation or testing in this repository's history. **Requires verification.**

### 5.3 Updater

**Status:** ❌ **Not implemented.** No `tauri-plugin-updater` dependency. No update server URL configured. No update manifest generation.

**What's needed for production:**
1. Add `tauri-plugin-updater` to `Cargo.toml`
2. Configure update endpoints in `tauri.conf.json`
3. Set up update manifest hosting (GitHub Releases can serve this)
4. Implement update check UI in frontend

### 5.4 GitHub Releases

**CI:** `.github/workflows/release.yml`  
**Trigger:** Push of tag matching `v*`  
**Platform:** `windows-latest` only (single matrix entry)  
**Steps:**
1. Checkout
2. Setup Node 20
3. Install Rust stable
4. `npm install`
5. `tauri-apps/tauri-action@v1` with `GITHUB_TOKEN`

**Missing:**
- **No macOS or Linux builds** — matrix only has `windows-latest`
- **No code signing** — no certificate configuration
- **No notarization** (macOS would need this but macOS isn't in the matrix)
- **No release notes generation** — hardcoded body: "Automated release of Neural Forge"
- **No pre-release/draft logic** — hardcoded `releaseDraft: false`, `prerelease: false`

### 5.5 Versioning

**Current version:** `1.4.0` (agrees across `Cargo.toml`, `package.json`, `tauri.conf.json`)

**Git tags (verified via `git tag --list`):**
Tags exist for: `v1.0.0`, `v1.2.0`, `v1.3.0`, `v1.4.0`, `v5.0.0`–`v5.2.0`, plus untagged variants (`1.0.0`, `1.3.0`, `1.4.0`, `5.0.1`–`5.0.11`, `5.1.0`, `5.2.0`). Also present: `neuralforge-v1.2.0`. This is a chaotic tag history — versions skip from 1.4.0 to 5.0.0 then back to 1.x then to 5.x again. The v5.x tags predate the current `master` branch state. The canonical release lineage is `v1.0.0 → v1.2.0 → v1.3.0 → v1.4.0`.

**CAUTION:** Release workflow triggers on `v*` tags. Pushing any tag starting with `v` (including the stale v5.x series) will trigger a build. Only push `v1.x` tags matching the current `Cargo.toml`/`package.json`/`tauri.conf.json` version.

### 5.6 CI/CD

**Only workflow:** `.github/workflows/release.yml` — tag-triggered release only.  
**Missing CI checks:**
- No PR checks (no `cargo test`, `cargo clippy`, `npm run build`, `npx tsc --noEmit` on push/PR)
- No linting workflow
- No security scanning
- No dependency audit

**What should exist:**
```yaml
# Recommended: add a ci.yml workflow triggered on push/PR that runs:
# - cargo check
# - cargo test
# - cargo clippy -- -D warnings
# - npx tsc --noEmit
# - npm run build
```

### 5.7 Build Process

**Development:**
```bash
npm install          # Install frontend deps
cd src-tauri && cargo build  # Build Rust backend
npm run tauri dev    # Run with hot reload
```

**Production build:**
```bash
npm run build        # Next.js static export → out/
cd src-tauri && cargo build --release  # Release Rust build
npm run tauri build  # Bundle with Tauri
```

**Build artifacts:**
- `out/` — Next.js static export (gitignored)
- `src-tauri/target/release/` — Rust binary
- `src-tauri/target/release/bundle/` — Installers

### 5.8 Signing Requirements

**Windows:** Code signing certificate (EV or standard) needed for:
- `.exe` installer (NSIS)
- `.msi` installer (WiX)
- The actual `.exe` binary

**Without signing:** Windows SmartScreen will block the installer with "Windows protected your PC." Users must click "More info" → "Run anyway."

**macOS:** Notarization required for distribution outside the App Store. Currently not relevant (no macOS CI target).

**Linux:** No signing infrastructure required. AppImage/Flatpak have their own distribution channels.

---

## 6. TESTING STATUS

### 6.1 Test Count (v1.0 audit baseline — requires re-verification)

**Last verified at commit `2d898f9` (v1.0 audit):**
- `cargo test`: **291 passed, 0 failed, 9 ignored**
- `cargo check`: 0 errors, ~163 style warnings
- `npm run build`: Clean
- `npx tsc --noEmit`: Clean

**Current commit `0c0d535` — NOT YET RE-VERIFIED.** Claude MUST run the full test suite as its first action.

### 6.2 Ignored Tests (9 known from v1.0 audit)

All 9 ignored tests require a live local Ollama instance. These are integration tests meant for manual execution, not CI. They are correctly gated with `#[ignore]`.

**New ignored tests (since v1.0 audit):**
- `credential_store::store_then_load_round_trips_the_real_key` — requires real OS keychain
- `credential_store::loading_a_missing_key_returns_empty_string_not_an_error` — requires real OS keychain
- `credential_store::deleting_a_missing_key_does_not_panic` — requires real OS keychain

### 6.3 Missing Test Coverage

**No tests exist for (verified this handoff):**
- `change_executor.rs` — complete stub, 0 tests
- `ai::credential_store` — 4 tests exist, 2 unconditional
- `performance/` module — no test files found (soak_test.rs may contain tests — requires verification)
- `services/` module — **requires verification**
- End-to-end IPC validation — no integration test that calls a Tauri command and verifies the response through the full stack
- Frontend component tests — no Jest/Vitest/React Testing Library configuration exists

### 6.4 Build Status

**Frontend:** `npm run build` (Next.js static export) must succeed.
**Backend:** `cargo check` and `cargo build` must succeed.
**Type checking:** `npx tsc --noEmit` must pass.

**Known warnings (v1.0 audit, ~163):**
- Mostly camelCase Tauri command parameter names (Tauri convention, not a bug)
- One dead-code struct (`MODEL_FAILED` constant in `core::events.rs`)
- New `#[allow(dead_code)]` on `agent_core` module — intentional, doc-commented

### 6.5 E2E Verification Requirements

For a production release, these must be verified on real hardware:

1. **Fresh install:** Clean Windows machine, no Ollama, no prior NeuralForge → install → launch → verify Ollama download prompt works
2. **With Ollama:** Machine with Ollama + model → chat works, streaming works, ghost-text works
3. **With cloud provider:** Configure OpenRouter/DeepSeek → chat works through cloud, verify key stored in Credential Manager
4. **Agent workflow:** Create requirement → plan → execute → verify → verify change_executor produces real patches
5. **Session persistence:** Close and reopen → sessions, settings, provider configs survive
6. **Database migration:** Open workspace from v1.2.0 era → verify all tables work
7. **Installer:** Run NSIS/WiX installer → app launches, no SmartScreen block (if signed), uninstaller works

---

## 7. CLAUDE CODE FINAL POLISH INSTRUCTIONS

### DO NOT:

1. **Do not rebuild architecture.** NeuralForge's architecture works. The provider routing system, agent pipeline, governance ledger, and workspace intelligence are proven. Do not restructure them.

2. **Do not introduce a state management library.** No Redux, no Zustand, no Jotai. The current `useState` + custom hooks + Tauri events pattern is intentional and works.

3. **Do not create a new agent system.** There are already five. Modify `agent_v2.rs` and `agent_core/` only when directly required to complete the documented `change_executor.rs` execution path or resolve verified execution bugs. Do not perform broad architectural rewrites, consolidation, modernization, or unprompted refactoring of these modules.

4. **Do not create provider-specific adapter files.** `groq.rs`, `deepseek.rs`, `openrouter.rs`, etc. are forbidden. All OpenAI-shaped providers route through `openai_compatible.rs`. Only create a new adapter for genuinely incompatible protocols (Anthropic Messages API, Gemini generateContent).

5. **Do not modify frozen files** without explicit Level 5 approval:
   - `src-tauri/src/agent/executor.rs`
   - `src-tauri/src/agent/planner.rs`
   - `src-tauri/src/database/indexer.rs`
   - `src-tauri/src/database/search.rs`
   - `src-tauri/src/database/resolver.rs`
   - `src-tauri/src/extensions/`
   - `src-tauri/src/ai/providers/ollama.rs`
   - The Ollama request flow inside `ai::chat_with_model_core`

6. **Do not break public contracts:**
   - `chat_with_model(request_id, model, messages)` signature
   - `AI_RESPONSE_TOKEN` event shape (`request_id`, `token`, `done`, `from_cache`)
   - `lib/ai.ts` exported function signatures

7. **Do not introduce unnecessary dependencies.** Before adding any crate or npm package, verify there isn't already a dependency that solves the problem.

8. **Do not silently modify tests.** Never delete failing tests, weaken assertions, or skip validation.

9. **Do not rebase, squash, or rewrite git history.**

10. **Do not remove Ollama support.** Ollama is the zero-config, always-available default. It must remain.

### DO:

1. **Run the full test suite as your first action:**
   ```bash
   cargo check && cargo test
   npx tsc --noEmit && npm run build
   ```
   Before changing code: record baseline test/build results to distinguish pre-existing issues from introduced regressions. Do not modify source files prior to establishing this baseline.

2. **Fix verified issues only.** Start with the production blockers in §3. Do not go hunting for problems to solve.

3. **Priority order for fixes:**
   1. `change_executor.rs` — implement real patch generation and application
   2. Route `completion.rs` and `inline.rs` through `provider_router`
   3. Remove or fix Cohere provider type
   4. Add CI workflow (`.github/workflows/ci.yml`) only if missing and only after all production blockers (§3) are fully resolved and verified
   5. Add database migration test
   6. Enable `specta` type generation in CI
   7. Characterize the intermittent test flake

4. **Improve quality:**
   - Remove dead code (e.g., `MODEL_FAILED` constant)
   - Fix Clippy warnings where safe
   - Add doc comments to public APIs that lack them
   - Ensure error messages are actionable (not just "failed")

5. **Address technical debt — preserve working architecture:**
   - **`agent_controller.rs`:** This is a legacy five-phase state machine. `agent_v2.rs` and `task_orchestrator.rs` are the active execution path. Do NOT consolidate or unify — `agent_controller.rs` is used by `multi_agent.rs` and has working commands. Leave it alone.
   - **`planning/` DAG system:** Superseded by `planning_engine.rs`. Do NOT delete it — commands are registered and may be in use. Add `#[deprecated]` doc comments to the public functions if you want to signal intent, but do not remove code.
   - **`ai::router`:** Ollama-only scoring. Do NOT expand. Eventually it should be folded into `provider_router`, but only as a deliberate Level 3+ change, not a drive-by refactor.

6. **Verify every change:**
   - After each change: `cargo check` (Rust) or `npx tsc --noEmit` (TypeScript)
   - After each Rust change: `cargo test`
   - After each frontend change: `npm run build`

7. **Update documentation:**
   - If you fix a production blocker, update this handoff document's §3
   - If you add a command, note it
   - Keep `PROJECT_STATE.md` updated as the running session log

8. **Preserve working systems.** If a test passes, the system it covers is working. Do not "improve" working code unless there's a concrete, verifiable reason.

9. **Handle the keyring migration verification.** Confirm that `add_provider_config` and `update_provider_config` call `credential_store::store_api_key()` and that raw keys are not persisted in SQLite. If they are, this is a P0 fix.

10. **Add the missing CI checks.** Create `.github/workflows/ci.yml` that runs on every push and PR:
    - `cargo check`
    - `cargo test`
    - `cargo clippy -- -D warnings` (or at least `--lib`)
    - `npx tsc --noEmit`
    - `npm run build`

---

## 8. FINAL VERIFICATION CHECKLIST

Claude Code must complete every item on this checklist before declaring the project production-ready. Mark items as `[x]` when verified, `[ ]` when incomplete, `[?]` when requiring human verification.

### Build & Test

- [ ] `cargo check` — 0 errors
- [ ] `cargo test` — all tests pass, 0 failures
- [ ] `cargo clippy --lib` — 0 errors (warnings acceptable if pre-existing)
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `npm run build` — succeeds
- [ ] `npm run tauri build` — produces installers (Windows at minimum)

### Providers

- [ ] Ollama chat works (requires local Ollama instance)
- [ ] Ollama streaming works (token-by-token)
- [ ] Ollama model discovery works (`list_models`)
- [ ] At least one OpenAI-compatible provider works end-to-end (requires real API key)
- [ ] Provider CRUD (add/update/delete) works through UI
- [ ] Provider connection test works
- [ ] API keys stored in OS keychain, not plaintext SQLite
- [ ] Active model per task type (chat/agent/inline/ghost) correctly routes

### Agent

- [ ] Agent task creation works
- [ ] Agent planning phase completes
- [ ] `change_executor.rs` produces real patches (not empty stubs)
- [ ] Patch application modifies files correctly
- [ ] Agent workflow runs end-to-end (create → plan → execute → verify)
- [ ] Multi-agent orchestration works (Supervisor + specialized agents)
- [ ] Agent approval/rejection gates function
- [ ] Knowledge store persists across sessions

### Governance

- [ ] Requirement creation works
- [ ] Ledger entries are hash-chained correctly
- [ ] Ledger verification passes (`verify_ledger`)
- [ ] Evidence records are created and FK-referenced correctly
- [ ] Promotion requests are recorded and auditable

### Editor

- [ ] Monaco editor loads
- [ ] Ghost-text completions appear (requires Ollama)
- [ ] Ctrl+K inline edit works (requires Ollama)
- [ ] Diff decorations display correctly
- [ ] File explorer navigates workspace

### Terminal

- [ ] Interactive terminal spawns (PTY)
- [ ] Sandboxed command execution works
- [ ] Sandbox allowlist/denylist is enforced
- [ ] Timeout kills long-running commands

### Database

- [ ] Workspace opens and indexes files
- [ ] FTS5 search returns results
- [ ] Semantic search returns results (requires fastembed)
- [ ] @mention file resolution works
- [ ] Chat sessions persist and restore
- [ ] Old database migration works (open v1.2.0-era `index.db`)

### Extensions

- [ ] Extensions list loads
- [ ] Bundled extensions are present
- [ ] Enable/disable toggle works
- [ ] Uninstall works with path traversal protection
- [ ] Extension API (`run_extension`) works

### Packaging

- [ ] `npm run tauri build` succeeds on Windows
- [ ] NSIS installer launches and installs
- [ ] Installed application launches
- [ ] Uninstaller removes application
- [ ] WiX MSI installer works (if applicable)

### Security

- [ ] API keys are NOT in SQLite `settings` table
- [ ] API keys ARE in OS credential store (Credential Manager on Windows)
- [ ] CSP blocks inline scripts
- [ ] Filesystem commands reject paths outside workspace
- [ ] No API keys in log output
- [ ] Terminal sandbox deny list is functional

### CI/CD

- [ ] CI workflow runs on push/PR
- [ ] CI runs `cargo test`
- [ ] CI runs `npx tsc --noEmit`
- [ ] CI runs `npm run build`
- [ ] Release workflow triggers on `v*` tags
- [ ] Release workflow produces Windows installer artifact

### Updater

- [ ] Auto-updater is configured (if required for production)
- [ ] Update check works
- [ ] Update manifest is published

---

## DOCUMENT MAINTENANCE

This document was produced by a read-only audit of commit `0c0d535d4aeae7c9395b56ea78bd2e3f5d317291` on branch `master`.

**Update frequency:** This deep-audit file is not meant to be updated every session. Update it when:
- Architecture changes (new subsystem, removed subsystem)
- A production blocker is resolved or discovered
- The authoritative system map changes (active/legacy classification)

**Running log:** `PROJECT_STATE.md` is the fast-moving companion — update it after each nontrivial session.

**Source of truth priority** (from `.clinerules`):
1. Repository files
2. Automated tests
3. Git history
4. Documentation
5. Previous AI conversations

This document is #4. When in doubt, trust the code.

---

*End of NeuralForge Claude Code Engineering Handoff*