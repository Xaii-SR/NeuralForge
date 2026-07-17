# NEURAL FORGE COMPLETE PROJECT AUDIT

Version: 1.0 (this audit)
Date: 2026-07-16
Current Commit: `2d898f9` — "Complete universal AI provider routing integration"
Current Branch: `master`
Repository: `C:\Users\saiah\NeuralForge`

This document was produced by a read-only architectural audit. It is a
snapshot, not a promise — treat any claim here that a future `git log` or
`cargo test` run contradicts as stale, and trust the repository over this
file. That instruction is not decoration: a prior version of
`PROJECT_STATE.md` drifted several commits behind actual HEAD before this
audit, which is exactly the failure mode this document exists to prevent
from recurring silently.

---

## 1. PROJECT OVERVIEW

Neural Forge is a **local-first desktop AI IDE** built on Tauri, aiming for
Cursor-level capability: a code editor with an integrated, provider-agnostic
AI assistant, workspace-aware context retrieval, and (increasingly) a
multi-agent autonomous execution layer, all running as a native desktop app
with a local SQLite database per workspace — no required cloud backend.

**Primary goals**, as evidenced by the module structure and commit history:
- A real, working local-model coding assistant (Ollama) with zero
  configuration required.
- An extensible path to cloud/self-hosted model providers without
  fragmenting the codebase into one bespoke client per AI company.
- Governed autonomous code-change execution (requirement → plan → execute →
  verify → promote), with human approval gates, not silent auto-apply.
- An emerging multi-agent orchestration layer (Supervisor + specialized
  Research/Coding/Testing/Review agents) built on top of the above.

**Current maturity level:** actively developed, pre-1.0-stable in spirit
despite the `1.2.0` version number in `Cargo.toml`/`package.json` — several
core subsystems (multi-agent orchestration, task orchestrator, change
executor) are freshly landed (see §8) and contain known stub logic (see
§11). The Ollama chat path and the governance/promotion pipeline are the
most battle-tested pieces (largest, oldest test coverage).

**Production status:** not production-ready for non-technical end users
today. Concretely: API keys for cloud providers are stored in plain text
(§10), at least one core execution module (`change_executor.rs`) is a
literal no-op stub (§11), and several `#[ignore]`d integration tests require
a real local Ollama instance to prove real behavior, meaning CI-only runs
don't fully validate the AI path end to end.

---

## 2. TECHNOLOGY STACK

### Desktop Layer
- **Tauri:** 2.11.3 (CLI 2.11.4)
- **Rust:** edition 2021, `rust-version = "1.77.2"`
- **Crate name:** `neuralforge` (binary), lib name `neuralforge_lib`
- **Backend architecture:** modular Rust crate, ~30 top-level modules (see
  §7), Tauri commands as the sole frontend↔backend boundary (no separate
  HTTP server)

### Frontend
- **Next.js:** 16.2.10 (Turbopack)
- **React:** 19.0.0
- **TypeScript:** 5.5.2
- **Tailwind CSS:** 3.4.4
- **Monaco Editor:** via `@monaco-editor/react` 4.7.0
- **Terminal:** `@xterm/xterm` 6.0.0 + `@xterm/addon-fit`
- **State management:** no global store library (no Redux/Zustand/Jotai) —
  local `useState`/custom hooks per feature (`hooks/*.ts`), with
  `window.dispatchEvent`/`window.addEventListener("nf_settings_updated", …)`
  as an ad hoc cross-component signal for settings changes, and Tauri's
  native `listen()`/`emit()` event bus for backend→frontend streaming
  (`AI_RESPONSE_TOKEN`, `inline-stream`, `inline-diff-stream`, orchestrator
  state events, etc.)

### Backend
- **Async runtime:** Tokio 1.x (`rt-multi-thread`, `macros`, `process`
  features)
- **HTTP client:** `reqwest` 0.12 (`rustls-tls`, `stream`, `json`,
  `blocking` features) — used for both Ollama and OpenAI-compatible
  provider calls
- **IPC:** Tauri's `#[tauri::command]` + `invoke_handler!`/`generate_handler!`
  macro; **114 registered commands** in `lib.rs` at this commit
- **Streaming:** Tauri's native event system (`AppHandle::emit`, frontend
  `listen()`), not WebSockets/SSE-to-frontend — SSE parsing happens
  Rust-side against the actual provider, then re-emitted as Tauri events
- **Embeddings:** `fastembed` 5.17.2 (local embedding generation, no cloud
  call)
- **Other notable deps:** `scraper` + `html2md` (web doc fetching/cleanup),
  `sysinfo` (hardware detection for VRAM gating), `walkdir` + `regex` +
  `sha2` (workspace indexing), `specta` (Rust→TypeScript type generation)

### Database
- **SQLite** via `rusqlite` 0.32 (`bundled-full` — no system SQLite
  dependency)
- **One `index.db` per opened workspace** (`database::open_for_workspace`),
  not a single global database
- Schema is additive-only (`CREATE TABLE IF NOT EXISTS`, non-fatal
  `ALTER TABLE ADD COLUMN`), per `.clinerules`
- A generic `settings` key/value table is reused across features (AI
  preferences, provider registry, active-model-per-task-type) rather than
  one table per feature — see §4

### AI Infrastructure
- **Router:** two distinct routers exist, easy to confuse (see the callout
  in §4) — `ai::router` (Ollama model scoring/cost-estimate heuristics) and
  `ai::provider_router` (the newer, cross-provider adapter-selection layer)
- **Provider system:** `ai::provider_registry` (SQLite-backed CRUD) +
  `ai::provider_router` (dispatch) + `ai::providers::{ollama, openai_compatible}`
  (the two working HTTP adapters)
- **Streaming:** real token-by-token streaming from the actual provider
  (Ollama's NDJSON stream, or SSE for OpenAI-compatible), re-emitted via
  Tauri events — not simulated/chunked-after-the-fact
- **Model discovery:** `ollama::list_models()` (real `/api/tags` call) and
  `openai_compatible::list_models()` (real `/v1/models` call)
- **Caching:** `ai::cache` — response cache keyed on `(model, messages)`,
  SQLite-backed, used by `chat_with_model` to skip re-generation on
  identical repeated prompts

---

## 3. COMPLETE ARCHITECTURE MAP

```
Frontend (Next.js/React components, e.g. ChatPane.tsx, AgentWorkbench.tsx)
        │
        │  @tauri-apps/api  invoke() / listen()
        ▼
Tauri IPC boundary  (114 #[tauri::command] handlers, src-tauri/src/lib.rs)
        │
        ▼
Rust Backend  (src-tauri/src/, ~30 top-level modules)
        │
        ├──► Feature Modules (non-AI-generation concerns)
        │      database/ · governance/ · planning/ · intelligence/
        │      filesystem/ · terminal/ · workspace/ · extensions/
        │
        └──► AI Generation Path
                    │
                    ▼
             ai::chat_with_model  (stable public command signature)
                    │
                    ▼
             ai::provider_router::resolve_provider_for_model
                    │
                    ▼
             ai::provider_router::adapter_kind_for(provider_type)
                    │
          ┌─────────┼──────────────────────┐
          ▼         ▼                      ▼
     "ollama"   openai/openrouter/…   anthropic/gemini
          │         (12 provider_types)     │
          ▼         ▼                      ▼
   ai::chat_with_    providers::        (error: no native
   model_core        openai_compatible   adapter implemented)
   (VRAM gate,       (ONE shared SSE
   ollama health     client for all
   key, unchanged)   12 types)
          │         │
          ▼         ▼
      Ollama HTTP API      Actual cloud/self-hosted provider HTTP API
```

**Explanation of each layer:**
- **Frontend** never talks to an AI provider directly. It calls a stable,
  narrow Tauri command (`chat_with_model(request_id, model, messages)`) and
  listens for `AI_RESPONSE_TOKEN` events.
- **Tauri IPC** is the only frontend↔backend boundary — no localhost HTTP
  server, no separate process.
- **Rust Backend** splits into feature modules (workspace intelligence,
  governance, planning — largely independent of AI generation) and the AI
  generation path.
- **`provider_router`** (added this session, see PROJECT_STATE.md) is the
  single place that decides which HTTP adapter handles a request, resolved
  from the SQLite-backed provider registry — not hardcoded per caller.
- **Adapters**: exactly two working HTTP clients exist
  (`providers::ollama`, `providers::openai_compatible`); the OpenAI-compatible
  one is deliberately generic and serves 12 different provider_type values,
  per the adapter-reuse rule in `.clinerules`.

---

## 4. CURRENT AI ARCHITECTURE

### ⚠️ Naming collision to know about immediately

There are **two unrelated things called "router"** in this codebase:

1. **`ai::router`** (`src-tauri/src/ai/router.rs`) — Ollama-only model
   scoring (`score_models`, parameter-count heuristics for
   speed/quality goals) and cost estimation. Predates the provider system.
   Used by `auto_select_model`.
2. **`ai::provider_router`** (`src-tauri/src/ai/provider_router.rs`) — the
   cross-provider adapter dispatch layer described in §3. This is the
   "AI Router" referenced in the architecture diagram and in
   `.clinerules`'s adapter rules.

There is also **`intelligence::router`** (`src-tauri/src/intelligence/router.rs`)
which is a **third, entirely different concept**: worker/agent
capability-matching for the governance/task-DAG system, unrelated to AI
model provider selection. **Do not confuse these three.** A future agent
asked to "modify the AI router" should almost always mean
`ai::provider_router`.

### AI Router (`ai::provider_router`)

- **Location:** `src-tauri/src/ai/provider_router.rs`
- **Responsibilities:**
  - `resolve_provider_for_model(conn, model)` — looks up which configured
    provider owns a model id (exact match against each provider's `models`
    list); falls back to the default local Ollama provider on no match.
  - `adapter_kind_for(provider_type)` — maps a `provider_type` string to
    `Ollama | OpenAiCompatible | Unimplemented`.
  - `health_key_for(config)` — Ollama keeps its historical `"ollama"` health
    key; every other provider gets an isolated `"provider:{id}"` key so one
    degraded cloud provider never affects another provider's or Ollama's
    cooldown state.
  - `stream_cloud_chat(...)` — the actual dispatch + streaming call for
    every non-Ollama provider.
  - `classify_task(prompt)` / `select_provider_and_model_for_task(...)` —
    capability-based model selection (Coding / Fast / Reasoning), using
    provider-declared capabilities + context length as an explicitly
    documented **heuristic**, not real benchmark data.
- **Model selection:** by capability tags + context length, not hardcoded
  provider/model names (see routing_validation tests in
  `provider_router.rs`'s test module).
- **Cost estimation:** exists in `ai::router::estimate_cost` (Ollama-only
  today; a rough $/1K-token table per `ProviderId`), not yet ported into
  `provider_router`.
- **Context handling:** context length is provider-declared
  (`ProviderCapabilities.context_length`), not measured/enforced by
  `provider_router` itself.

### Provider Registry (`ai::provider_registry`)

- **Location:** `src-tauri/src/ai/provider_registry.rs`
- **Purpose:** the single persisted source of truth for "what providers
  exist and how are they configured."
- **Storage:** SQLite `settings` table (same generic key/value table used
  for AI preferences), keyed `provider_configs` for the provider list and
  `active_model_{chat,agent,inline,ghost}` for per-task-type default model
  selection. No dedicated schema/migration was introduced for this — an
  intentional choice to avoid a schema change for this feature.
- **Provider lifecycle:** `ProviderConfig { id, name, provider_type,
  base_url, api_key, models, enabled, is_default, capabilities, created_at }`.
  A default Ollama entry (`id = "default-ollama"`) always exists even with
  an empty `settings` row.
- **CRUD operations (all `#[tauri::command]`):** `list_provider_configs`,
  `add_provider_config`, `update_provider_config`, `delete_provider_config`
  (refuses to delete the last remaining provider), `set_default_model`,
  `get_model_config`.

### Provider Adapters

#### Ollama Adapter
- **Location:** `src-tauri/src/ai/providers/ollama.rs`
- **Implementation:** real HTTP client against `http://localhost:11434`
  (hardcoded base URL — this predates the multi-provider registry).
- **Streaming:** `chat_stream()` parses Ollama's newline-delimited JSON
  stream, calls back per-token, and returns real `ChatStats`
  (`eval_count`/`eval_duration_ns` → computed tokens/sec) sourced from
  Ollama's own final `done: true` chunk — not client-side approximated.
- **Model discovery:** `list_models()` — real `/api/tags` call, returns
  parameter size/quantization/context length per model, used for the VRAM
  gate.
- **Health checks:** `health_check()` — `/api/version` ping.
- **Why it is frozen:** `.clinerules`' Frozen Architecture Protection does
  not literally list `ollama.rs`, but this session's explicit mission
  ("Ollama behavior must remain unchanged... do not remove ollama.rs") and
  the fact that `ai::chat_with_model_core` (the function that calls it) has
  a live integration test pinned to its exact log lines/health key make it
  frozen in practice. Any change here should be treated as Level 3+.

#### OpenAI-Compatible Adapter
- **Location:** `src-tauri/src/ai/providers/openai_compatible.rs`
- **Purpose:** the **universal compatibility layer** for every provider
  whose API is shaped like OpenAI's `/v1/chat/completions` +
  `/v1/models` — one Rust struct (`OpenAiCompatibleProvider`), not one file
  per company.
- **Streaming architecture:** parses SSE (`data: {json}\n\n` blocks,
  `[DONE]` sentinel), reads `choices[0].delta.content` per chunk, tracks
  `usage.total_tokens` when the provider reports it.
- **Supported through this adapter (per `provider_router::adapter_kind_for`
  and this session's requirement):** OpenAI, OpenRouter, DeepSeek, Groq,
  Together AI, Fireworks, DeepInfra, LM Studio, vLLM, llama.cpp servers,
  the literal `"openai_compatible"` type, and user-defined `"custom"`
  providers. (The frontend's `ProviderManager.tsx` dropdown also lists
  `"mistral"` and `"cohere"` as selectable provider_types; both currently
  route through this same adapter by virtue of `adapter_kind_for`'s
  catch-all — Mistral's real API is largely OpenAI-shaped so this is
  likely fine, but **Cohere's real API is NOT OpenAI-shaped** and picking
  it in the UI today will silently produce a provider that fails at
  request time, not a clean "unsupported" error. This is a real, currently
  undocumented gap — flagged here for the first time.)
- **Why it's the universal compatibility layer:** explicit architectural
  rule from `.clinerules`/session mission — creating `groq.rs`,
  `deepseek.rs`, etc. is expressly forbidden unless a provider's wire
  protocol genuinely differs from OpenAI's chat-completions shape.

### Native Adapters

- **Gemini status:** not implemented. `adapter_kind_for("gemini")` returns
  `Unimplemented`; a chat request routed to a Gemini-typed provider fails
  with an explicit "does not have a native adapter yet" error rather than
  being silently misrouted through the OpenAI-compatible client.
- **Anthropic status:** same as Gemini — not implemented, fails loudly and
  explicitly.
- **Rule:** only create a separate native adapter when the existing
  OpenAI-compatible layer cannot represent the provider's actual protocol.
  Anthropic's Messages API and Gemini's `generateContent` API are the two
  cases in this codebase currently believed to require that (different
  request/response envelope, different streaming event format) — this has
  not been re-verified against their current API docs as part of this
  audit; treat as a reasonable assumption, not confirmed fact.

---

## 5. PROVIDER ARCHITECTURE RULES

### Frozen Systems

Must not be rewritten without Level 4/5 approval per `.clinerules`:

- `src-tauri/src/ai/providers/ollama.rs`
- The existing Ollama request flow inside `ai::chat_with_model_core`
  (VRAM gating via `model_manager::check`, the `"ollama"` health key,
  the `chat_completed`/`chat_failed` tracing log lines)
- `ai::provider_router`'s adapter-dispatch shape (Ollama vs.
  OpenAI-compatible vs. Unimplemented) — extend by adding new
  `provider_type` → `AdapterKind` mappings, not by restructuring the enum
- The existing frontend AI contracts: `chat_with_model(request_id, model,
  messages)`'s signature, the `AI_RESPONSE_TOKEN` event shape
  (`request_id`, `token`, `done`, `from_cache`), `lib/ai.ts`'s exported
  function signatures

Also explicitly protected per `.clinerules` (unrelated to AI, but binding
on any future change that touches them):

- `src-tauri/src/agent/executor.rs`
- `src-tauri/src/agent/planner.rs`
- `src-tauri/src/database/indexer.rs`
- `src-tauri/src/database/search.rs`
- `src-tauri/src/database/resolver.rs`
- `src-tauri/src/extensions/`

### Adapter Rule

**Never create**, for any of the following, a dedicated Rust file (e.g.
`groq.rs`, `deepseek.rs`, `openrouter.rs`, `together.rs`, `fireworks.rs`,
`lmstudio.rs`, `vllm.rs`, `llamacpp.rs`, `deepinfra.rs`): all of these are
`openai_compatible`-routed today via `adapter_kind_for`'s catch-all arm.
Adding a new one of these company names to the frontend's provider-type
list requires **zero backend code changes** — it already routes correctly.

Only add a new `AdapterKind` variant (and a new adapter file) when a
provider's request/response format is not expressible as OpenAI-compatible
chat completions. Anthropic and Gemini are the two currently believed to
qualify; verify against current API docs before implementing, and flag the
architectural impact for approval first per `.clinerules`.

---

## 6. FRONTEND ARCHITECTURE

### Main Components (from `components/`, 18 top-level `.tsx` files +
subfolders `composer/`, `editor/`, `terminal/`, `ui/`)

| Component | Role | IPC? | Maturity |
|---|---|---|---|
| `ChatPane.tsx` | Main chat UI, model auto-select, streaming display, Stop button | Yes (`chat_with_model`, `auto_select_model`, `ollama_health_check`, `list_models`) | Production — most exercised AI surface |
| `SettingsPanel.tsx` | Ollama defaults + mounts `ProviderManager` | Yes (`get_preferences`, `save_preferences`, `list_models`, `clear_response_cache`) | Production |
| `ProviderManager.tsx` | Cloud/custom provider CRUD, connection test, model discovery, per-task model assignment, capability badges | Yes (`lib/providers.ts` → provider_registry commands) | New this session — Rust side unit-tested; UI CRUD flow not yet verified against a real Tauri runtime (browser-preview `invoke()` limitation) |
| `AgentWorkbench.tsx` | Orchestrator task lifecycle UI (analyzing/planning/executing/observing/verifying phases) | Yes (`lib/orchestrator.ts` → `task_orchestrator` commands) | Newer, less battle-tested than ChatPane; depends on `change_executor.rs` which contains a known stub (§11) |
| `Editor.tsx` / `EditorPane.tsx` | Monaco editor, ghost-text (FIM completion), diff decorations, inline AI edit (Ctrl+K) | Yes (`stream_inline_edit`, ghost-text commands) | Production |
| `Terminal.tsx` | xterm-backed PTY terminal | Yes (`terminal` module + `terminal_executor` sandboxed variant) | Production |
| `GovernancePanel.tsx` / `WorkersPanel.tsx` | Requirement/ledger/promotion + worker capability UI | Yes | Production per PROJECT_STATE.md's prior audit, not re-verified this pass |
| `PromptMaker.tsx` | Meta-prompt generator, routes through the real chat pipeline (see prior session's consolidation) | Yes (`chat_with_model`) | Production |
| `BootstrapPanel.tsx` / `BootstrapManager.tsx` | Self-improvement-via-local-git-branch UI | Yes | Production; scope is local-only, "never pushes" per `.clinerules`-adjacent design note in `PROJECT_STATE.md` |
| `ExtensionsPanel.tsx` | Extension system UI | Yes | Present; not audited this pass |
| `composer/ComposerWindow.tsx` + friends | Multi-file AI composer with @mention context, terminal command blocks | Yes | Production per prior session's theming pass |

**Which are experimental:** `AgentWorkbench.tsx` and everything under the
`task_orchestrator`/`multi_agent`/`change_executor` stack it depends on —
newest code, and `change_executor.rs`'s patch-generation is a literal stub
(§11), meaning the orchestrator can plan and track task lifecycle state but
cannot yet actually produce real file diffs through that specific path.

**Which communicate through IPC:** effectively all of the above — this is
a Tauri app, there is no component that does meaningful work without an
`invoke()` call somewhere in its tree.

---

## 7. BACKEND MODULE MAP

### AI (`src-tauri/src/ai/`)

| File | Purpose | Depends on | Modification risk |
|---|---|---|---|
| `mod.rs` | Command surface: `chat_with_model`, `auto_select_model`, `list_models`, health/context/cache commands | Everything below | **High** — public command signatures are frozen contracts |
| `router.rs` | Ollama-only model scoring/cost heuristics (legacy, predates provider_router) | `providers::ollama` | Medium |
| `provider_router.rs` | Cross-provider adapter dispatch + capability-based task routing | `provider_registry`, `providers::{ollama, openai_compatible}` | **High** — the architectural chokepoint; changes here affect every provider |
| `provider_registry.rs` | SQLite-backed provider CRUD | `core::errors`, `rusqlite` | Medium-high — schema-adjacent (settings table encoding) |
| `providers/ollama.rs` | Ollama HTTP client | `reqwest` | **High** — frozen, see §5 |
| `providers/openai_compatible.rs` | Universal OpenAI-shaped HTTP client | `reqwest`, `futures_util` | High — shared by 12 provider types, bugs here are wide-blast-radius |
| `providers/mod.rs` | `ProviderId` enum, static provider metadata registry (separate concept from `provider_registry`'s dynamic configs — legacy, worth reconciling) | — | Low-medium |
| `health.rs` | `HealthRegistry` — generic per-key cooldown/failure tracking | — | Medium (widely depended on) |
| `cache.rs` | Response cache keyed on (model, messages) | `rusqlite` | Low |
| `context.rs` | Chat-time context prompt building from workspace index | `database::search` | Medium |
| `composer.rs` | Multi-file composer session state, terminal command block execution | `terminal` | Medium |
| `inline.rs` | Inline (Ctrl+K) edit streaming | `providers::ollama` (only — not yet routed through `provider_router`, a gap worth noting) | Medium |
| `completion.rs` | Ghost-text FIM completion pipeline | `providers::ollama` (same gap as above) | Medium |
| `autocomplete.rs`, `docs.rs`, `git.rs`, `web.rs`, `model_manager.rs` | Supporting features (ghost text scaffolding, doc caching, git status/diff for AI context, web search, VRAM estimation) | Various | Low-medium |

**Architecture note (new discovery this audit):** `inline.rs` (Ctrl+K
inline edit) and `completion.rs` (ghost text) both call
`providers::ollama` **directly**, not through `provider_router`. Only
`ai::mod.rs::chat_with_model` (the main chat path) was migrated to the
unified router this session. This means today, inline-edit and ghost-text
are still Ollama-only regardless of what cloud providers are configured —
consistent with the mission's explicit scope ("finish chat routing first"),
but a real gap to close in a future phase if inline-edit/ghost-text should
also support cloud providers.

### Agents (multiple locations — see naming note below)

- **`src-tauri/src/agent/`** (`executor.rs`, `planner.rs`, `memory.rs`) —
  the original, frozen (§5) governed execution pipeline: task → execute →
  verify → rollback, tied into `governance::ledger`/`promotion`.
- **`src-tauri/src/agent_controller.rs`** — a five-phase state machine
  (`Idle → Analyzing → Planning → Executing → Observing → Verifying →
  Completed/Failed`) — appears to be a newer, parallel agent concept from
  the `agent/` module above. **Naming risk:** `agent/` vs `agent_controller`
  vs `agent_v2.rs` are three different things; verify which one a task
  actually needs before assuming.
- **`src-tauri/src/agent_v2.rs`** — yet another agent implementation,
  imports `intelligence::router` (worker capability matching), has its own
  `AgentState` enum and `ApprovalRegistry` (managed Tauri state). Registered
  separately in `lib.rs`.
- **`src-tauri/src/multi_agent.rs`** — Supervisor + Research/Coding/Testing/
  Review specialized agents, built atop `agent_controller`,
  `task_orchestrator`, and `knowledge_store` (per its own doc comment).
  This is the newest, most composed layer.

**Modification risk:** High for `agent/executor.rs`/`planner.rs` (frozen).
Medium for the others — real but less battle-tested; changes should be
scoped narrowly and tested since three overlapping "agent" concepts already
exist and a fourth would compound the confusion.

### Planning

- **`src-tauri/src/planning/`** (`dag.rs`, `planner.rs`) — task DAG
  planning: cycle/orphan detection, topological execution ordering,
  failure-blocks-dependents-not-siblings semantics (per PROJECT_STATE.md's
  prior audit).
- **`src-tauri/src/planning_engine.rs`** — a **different**, newer planning
  concept: `TaskPlan { task_description, objective, affected_files,
  subtasks, risks, verification, unknown_information, complexity }`, used
  by `task_orchestrator.rs`. Not the same system as `planning/dag.rs`.

### Intelligence

- **`src-tauri/src/intelligence/`** (`context.rs`, `gateway.rs`,
  `matcher.rs`, `registry.rs`, `reliability.rs`, `router.rs`) — worker
  capability registry, capability matching for task assignment, retry/
  reliability scoring derived from real promotion verdicts. **Not related
  to AI model provider routing** despite the `router.rs` filename — see the
  naming-collision callout in §4.

### Database

- **`src-tauri/src/database/`** (`indexer.rs`, `resolver.rs`, `search.rs`,
  `mod.rs`) — workspace file indexing, keyword/semantic search, file-path
  resolution (`@mention` support). `mod.rs` defines the base schema
  (`files` table, etc.) and `DbState` (the managed `Mutex<Option<Connection>>`
  every command locks). **Frozen** (`indexer.rs`, `resolver.rs`, `search.rs`
  per `.clinerules`).

---

## 8. COMPLETED DEVELOPMENT HISTORY

Reconstructed from `git log` at this commit (chronological, oldest→newest of
what's shown; full history is longer):

| System | Purpose | Commit (short) | Status |
|---|---|---|---|
| Prompt/provider dropdown foundation | Early provider/model UI, before the current registry existed | `5297642`…`a2682ac` | Superseded by current provider_registry/provider_router |
| Workspace intelligence foundation | Scheduler, hash, parser registry, watcher | `300b8e5` (v5.2.0) | Present |
| Ghost text real FIM pipeline | Replaced hardcoded placeholder with real Ollama FIM completion | `0aa5a3e` | Complete, but Ollama-only (see §7 gap) |
| ChatPane Stop button | Stream cancellation | `1d856dc` | Complete |
| Dead code removal | Removed old runtime files, `benchmarks.rs`, `InlinePromptBar` | `686577c` | Complete |
| Agent Controller + Workspace Scanner | Foundation for autonomous agent architecture | `bce55ad` | Complete (foundation only) |
| Context Retrieval + Code Search | Ranked file discovery, DB-backed keyword search | `e685a60` | Complete |
| Planning Engine | Task decomposition, impact analysis, plan validation | `47202b3` | Complete |
| Change Executor scaffold | Patch model, DiffGenerator, PatchApplier, PatchValidator **types registered** | `24bc677` | **Scaffold only — `generate_patches` is a stub returning empty results (§11)** |
| Terminal Executor + Command Sandbox | Tokio streaming executor, allowlist/denylist, timeout | `432ff96` | Complete, 7 tests |
| Error Analyzer + Self-Healing Recovery Loop | Failure classification, retry suggestion | `7cdd7ec` | Complete |
| Task Orchestrator + Knowledge Store | Multi-step autonomous execution engine, persistent project memory | `3b2d5de` | Complete |
| Agent Workbench UI | Lifecycle panel, timeline, terminal, plan preview, approval controls | `22e37cc` | Complete (UI); depends on Change Executor's stub for real patch application |
| Task Orchestrator IPC Integration | Rust commands, Tauri state, real-time event sync | `e53d3cb` | Complete |
| Semantic Code Search + Multi-File Refactoring | Symbol-aware retrieval, cross-reference engine, repository graph, rename planner | `2ce90b5` | Complete per commit title, not independently re-verified this audit |
| Multi-Agent Orchestration Layer | Supervisor + Research/Coding/Testing/Review agents, inter-agent messaging, Knowledge Store persistence | `17c507b` | Complete per commit title, not independently re-verified this audit |
| Release prep, crate rename | Version bump to 1.2.0, crate renamed `app`→`neuralforge` | `7dca574` | Complete |
| **Universal AI Provider Routing** | Unified `provider_router`, `provider_registry`, `openai_compatible` adapter, mounted `ProviderManager` UI | `9518afe` (checkpoint) → `2d898f9` | **Complete** — this session's work; see PROJECT_STATE.md for full detail |

**Older history** (pre-`5297642`, referenced in earlier PROJECT_STATE.md
snapshots but not re-verified this audit): Requirement Intelligence,
Traceability Ledger + Evidence (SHA-256 hash-chained), Task DAG Planning,
Promotion Governance, Worker Intelligence, Autonomous Reliability Layer.
Module directories for all of these are present on disk.

---

## 9. CURRENT RELEASE STATE

**Current version:** Neural Forge v1.2.0 (`Cargo.toml` and `package.json`
agree)

**Current achievements** (verified this audit):
- Universal provider routing — one registry, one dispatch path, Ollama
  unchanged, 12 provider types routed through a single shared adapter
- Multi-agent orchestration foundation (Supervisor + 4 specialized agent
  roles) — present, commit-verified, not independently re-tested this audit
- Semantic code intelligence (search, cross-reference, refactoring) —
  present, commit-verified, not independently re-tested this audit
- Governed agent execution pipeline (requirement→task→execute→evidence→
  promotion) — present, module structure verified, not independently
  re-tested this audit
- Provider abstraction layer separating "what UI/chat calls" from "which
  HTTP client actually runs" — verified this session via 12 new unit tests

**Testing — latest verified this audit (commit `2d898f9`, clean working
tree):**

| Check | Result |
|---|---|
| `cargo check` | Clean — 0 errors, ~163 pre-existing style warnings (camelCase Tauri args, one dead-code struct) |
| `cargo test` | **291 passed, 0 failed, 9 ignored** (the 9 ignored require a live local Ollama instance and are meant to be run manually per `docs/SETUP.md`) |
| `cargo clippy --lib` | Not re-run in this specific audit pass; last run (prior session, same commit lineage) reported 0 errors |
| `npm run build` | Clean |
| `npx tsc --noEmit` | Clean |

**Known flake (documented in `PROJECT_STATE.md`, not observed in this
audit's run):** an intermittent parallel-test-isolation race that has
surfaced in unrelated tests (`completion::pipeline_hit`,
`indexer::index_workspace_reindexes_after_file_change`) in roughly 1-in-3
full-suite runs; both pass reliably in isolation. Confirmed pre-existing,
not caused by the provider-routing work. Not reproduced in this audit's
single verification run (291/291 clean).

---

## 10. SECURITY AUDIT

**Filesystem boundaries:** the `filesystem/` module and `terminal_executor.rs`'s
`SandboxConfig` (allowlist/denylist, timeout) suggest deliberate scoping of
what agent-initiated file/command operations can touch, but this audit did
not trace every code path to confirm no escape exists — treat as
plausible, not verified.

**Sandbox:** `terminal_executor.rs` implements a command sandbox
(allowlist/denylist-based) for the agent's terminal execution path,
separate from the interactive `Terminal.tsx` PTY (which is intentionally
unrestricted, matching normal terminal expectations).

**Provider security:**

**Known limitation — API keys stored insecurely.** Audited and documented
this session (see `provider_registry.rs`'s doc comment on `ProviderConfig`
and `PROJECT_STATE.md`). Specifically:
- `ProviderConfig.api_key` is stored as **plain-text JSON** inside the
  `settings` key/value table in each workspace's SQLite `index.db`.
- No OS keychain / credential-manager integration exists anywhere in the
  crate — `Cargo.toml` has no `keyring` (or equivalent) dependency.
- Any process or user able to read a workspace's `index.db` file can read
  every configured cloud provider's API key in plain text.
- **This is acceptable for the Ollama-only default path** (no key
  involved) but is a real, live exposure the moment a user adds any cloud
  provider credential.

**Migration required before production cloud deployment** (not
implemented, and intentionally not invented/solved by this audit or by the
session that added the provider system — flagged for a dedicated,
separately-approved change): move `api_key` storage to the OS credential
store (Windows Credential Manager / macOS Keychain / Linux libsecret, most
naturally via the `keyring` crate from Rust), with `ProviderConfig` storing
only a reference/id rather than the raw secret. This is correctly scoped
as its own Level 3+ change, not a drive-by fix bundled into unrelated work.

---

## 11. CURRENT LIMITATIONS

Documented honestly, including two newly-discovered items this audit did
not previously have on record:

- **Native Gemini adapter missing** — by design, fails loudly rather than
  silently misrouting (§4).
- **Native Anthropic adapter missing** — same as above.
- **Capability metadata is coarse** — provider-level (not per-model)
  capability flags and context length are used as proxies for
  coding/fast/reasoning task routing; no real per-model benchmark data
  exists yet. Documented as a heuristic in `provider_router.rs` itself.
- **Cloud provider adapter is unverified end-to-end against a real Tauri
  runtime** — the OpenAI-compatible adapter and provider CRUD flow are
  proven by Rust unit tests and by graceful-failure behavior in a
  browser-only preview (where `invoke()` cannot function at all), but not
  by an actual `npm run tauri dev` session against a real LM Studio/
  OpenRouter/etc. endpoint. Recommended as the top follow-up action in
  `PROJECT_STATE.md`.
- **API key security migration needed** — see §10, not remediated by
  design (scope discipline), only documented.
- **NEW — `change_executor.rs`'s `generate_patches` is a literal stub**:
  `pub fn generate_patches(_plan: &TaskPlan, _root: &Path) -> Result<(Vec<Patch>, Vec<String>), String> { Ok((vec![], vec![])) }`.
  It unconditionally returns empty patches and empty errors regardless of
  input. Any UI/orchestrator flow that depends on this function (the
  `AgentWorkbench.tsx` → `task_orchestrator` → `change_executor` path)
  cannot currently produce real file changes through that specific
  route, even though the surrounding state machine (planning, approval,
  lifecycle tracking) is real and wired. This was not previously flagged
  in `PROJECT_STATE.md` and should be treated as a known gap, not a
  regression — it appears to be an intentional scaffold-first commit
  (`24bc677`, "Change Executor scaffold") that hasn't been completed yet.
- **NEW — `inline.rs` (Ctrl+K inline edit) and `completion.rs` (ghost
  text) are not routed through `provider_router`** — both call
  `providers::ollama` directly, meaning they remain Ollama-only regardless
  of configured cloud providers. Consistent with this session's explicit
  scope (chat path only), but a real gap for a future phase.
- **NEW — Cohere provider_type has no real compatibility path** — the
  frontend lists `"cohere"` as a selectable provider type, but
  `adapter_kind_for`'s catch-all routes it through the OpenAI-compatible
  adapter, and Cohere's actual API is not OpenAI-shaped. Selecting it
  today will produce request failures at chat time, not a clean
  "unsupported" error the way Anthropic/Gemini correctly do. Either give
  Cohere a real native adapter or remove it from the UI's provider-type
  list until one exists.
- **Three-plus overlapping "agent" concepts** (`agent/`, `agent_controller.rs`,
  `agent_v2.rs`, `multi_agent.rs`) and **two overlapping "planning" concepts**
  (`planning/`, `planning_engine.rs`) exist side by side. Not necessarily
  wrong (they may represent genuine architectural evolution with intentional
  boundaries), but the naming makes it easy for a future agent to modify the
  wrong one. Recommend an explicit reconciliation/renaming pass or a
  written note on which is authoritative for new work.
- **`docs/governance/Sprint-Gate-Protocol-v1.0.md` remains an explicit
  placeholder** per its own text (carried forward from prior
  `PROJECT_STATE.md`, not re-verified this audit but the file still exists
  at the same path).

---

## 12. AI COUNCIL FUTURE ARCHITECTURE

Not yet implemented. Documented here as the binding architectural
constraint for whenever it is built, per this session's explicit mission
scoping ("Do NOT begin AI Council yet... provider architecture must be
completed first").

```
Frontend (Council UI — not yet built)
        │
        ▼
Council Service (not yet built — Tauri command surface)
        │
        ▼
Council Engine (not yet built — orchestration logic: which models to
        │        consult, how to reconcile disagreement, etc.)
        ▼
Existing Neural Forge Router  →  ai::provider_router  (THIS IS THE ONLY
        │                         ENTRY POINT — no new routing layer)
        ▼
Provider Adapter  (ollama / openai_compatible / future native adapters)
        │
        ▼
Provider  (whatever the user configured)
```

**AI Council MUST NEVER:**
- Call OpenAI, Gemini, Claude, or any other provider directly (no new
  `reqwest::Client` constructed inside Council code)
- Manage its own API keys (must read from `provider_registry`)
- Create its own provider clients (must call `provider_router::stream_cloud_chat`
  or an equivalent function added *to* `provider_router`, not bypass it)

Any future AI Council implementation task should begin by re-reading
`provider_router.rs` in full and designing Council's multi-model-consult
logic as a caller of `resolve_provider_for_model`/`stream_cloud_chat` (or
a new sibling function in the same file), not as a parallel system.

---

## 13. FUTURE ROADMAP

**Phase 1: Universal Provider System**
**STATUS: COMPLETE** (commit `2d898f9`, this audit's current HEAD)

**Phase 2: AI Council Foundation**
Not started. Must consume `ai::provider_router` per §12. No code exists for
this yet — no Council UI, service, or engine files were found anywhere in
`components/` or `src-tauri/src/`.

**Phase 3: Multi-Agent Intelligence**
Partially started — `multi_agent.rs` (Supervisor + Research/Coding/Testing/
Review agents) exists per commit `17c507b`, built on `agent_controller`,
`task_orchestrator`, `knowledge_store`. Blocked from being fully real
end-to-end by `change_executor.rs`'s stub (§11) for any workflow that needs
to actually produce file patches.

**Phase 4: Production Hardening**
Not started as a discrete phase. Individual hardening items are tracked
piecemeal in this document (§10 security migration, §11 limitations) and
in `PROJECT_STATE.md`'s "Next Recommended Actions."

---

## 14. DEVELOPMENT RULES FOR FUTURE AI AGENTS

**Before coding:**
- Read this file (`docs/architecture/NEURALFORGE_COMPLETE_PROJECT_AUDIT.md`)
- Read `.clinerules` in full
- Read `PROJECT_STATE.md` for the latest session-to-session state (this
  file is a deeper, less frequently updated architectural snapshot;
  `PROJECT_STATE.md` is the fast-moving companion)
- Run `git log --oneline -20` and `git status` yourself — do not trust
  either document's claimed commit/state without verifying, per the
  source-of-truth priority in `.clinerules` (repo files > tests > git
  history > docs > previous AI conversations)

**Never:**
- Rewrite `ai::provider_router`'s adapter-selection architecture (extend
  the `AdapterKind` mapping, don't restructure it)
- Duplicate systems — before writing a new "agent," "planner," or "router,"
  grep for the word first; this codebase already has multiple overlapping
  concepts per those names (§7, §11) and does not need a fourth
- Remove providers, including Ollama, without explicit approval
- Break the Ollama code path — `ai::chat_with_model_core` and
  `providers::ollama` are frozen (§5)
- Create provider-specific adapter files (`groq.rs`, etc.) for anything
  the OpenAI-compatible adapter already handles (§5)
- Trust a stale document over `git log`/`cargo test`/actual file contents

**Always:**
- Extend existing systems — the provider system, the governance pipeline,
  and the workspace indexer are all designed to be extended in place
- Preserve public Tauri command signatures and frontend contracts; change
  implementation behind the interface, not the interface itself
- Update `PROJECT_STATE.md` after any nontrivial change (this deep-audit
  file is not meant to be updated every session — treat it as a periodic
  re-audit artifact, not a running log)
- Run `cargo check && cargo test` (Rust) and `npx tsc --noEmit && npm run
  build` (frontend) before considering any change complete

---

## 15. FINAL ARCHITECTURE SUMMARY

Neural Forge is now a modular AI operating environment where the chat/
generation surface has a single, provider-agnostic routing chokepoint
(`ai::provider_router`) sitting between a stable, unchanged frontend
contract and a small set of real HTTP adapters — one for Ollama (frozen,
zero-config, always available) and one shared OpenAI-compatible client that
already covers a dozen cloud and self-hosted provider types without
per-company code duplication. Provider independence is real: adding a new
OpenAI-shaped provider to the product requires no backend code, only a UI
dropdown entry, because the adapter layer was deliberately built generic
from the start rather than accreting one client per company. Native,
protocol-incompatible providers (Anthropic, Gemini) are explicitly
unimplemented rather than silently mishandled — a real gap, but an honest
one.

Layered on top of (and mostly independent from) this AI-generation path is
a second, actively-evolving system: governed autonomous agent execution
(requirement → plan → execute → evidence → promotion, human-gated) and a
newer multi-agent orchestration layer (Supervisor + specialized agents)
that together represent Neural Forge's Cursor-parity ambitions beyond
single-turn chat. This layer is real but less mature than the AI-routing
core — it has more overlapping/legacy naming to navigate (§7, §11) and at
least one genuinely stubbed component (`change_executor.rs`) blocking true
end-to-end autonomous file changes through that specific path.

Future AI Council capability has an explicit, binding architectural
contract already written for it (§12) even though no code exists yet:
Council must be a caller of `provider_router`, never a parallel provider
client. That single rule — extend the router, never bypass it — is the
throughline for every future AI-facing feature in this codebase, and is
the most important thing for the next engineer (human or AI) to internalize
before writing a single line of new AI-integration code.

---

# VALIDATION BEFORE FINISHING

- File created successfully: `docs/architecture/NEURALFORGE_COMPLETE_PROJECT_AUDIT.md`
- No source files modified — verified via `git status --short` immediately
  before and after this audit (both clean except the new untracked audit
  file and its new parent directory).
- Git diff contains only the new documentation file and its new directory.
