# NeuralForge Project State

This file changes often as work progresses. Permanent operating rules live
in `.clinerules`, not here — do not duplicate rules into this file.

Current repository:

C:\Users\saiah\NeuralForge

Current commit (about to be superseded by this session's commit):

9518afe — "checkpoint: baseline before provider routing migration"

Current branch:

master

**Correction to a prior version of this file:** an earlier snapshot claimed
HEAD was `22c5230` ("Sprint 10 release candidate") and described an
`agent/governance/planning/intelligence/bootstrap` requirement→task→
execute→evidence→promotion pipeline as the "Completed Systems." That
pipeline's module directories are still present on disk, but the actual git
history has moved well past that snapshot (Multi-Agent Orchestration Layer,
Semantic Code Search, Task Orchestrator IPC, Agent Workbench UI, a crate
rename to `neuralforge`/`neuralforge_lib`, version bump to 1.2.0). This file
was not kept in sync with those commits. Treat any claim here that isn't
corroborated by a fresh `git log` / `cargo test` run as suspect, per
`.clinerules`' own source-of-truth priority (repo files > tests > git
history > docs).

## Architecture

Verified from repository files at this checkpoint:

- **Desktop shell:** Tauri 2.11.3
- **Backend:** Rust, edition 2021, rust-version 1.77.2. Crate name
  `neuralforge` (lib name `neuralforge_lib`). Modules: `agent/`, `governance/`,
  `planning/`, `intelligence/`, `bootstrap/`, `database/`, `ai/`,
  `extensions/`, `filesystem/`, `terminal/`, `workspace/`, `services/`,
  `performance/`, `parsers/`, `hardware/`, `core/`.
- **Persistence:** single `index.db` per opened workspace (rusqlite,
  bundled SQLite). Provider configs and per-task-type model selection are
  stored in the generic `settings` key/value table (see AI Provider
  Architecture below) — no new schema/migration was introduced this session.
- **Frontend:** Next.js 16.2.10, React 19, TypeScript 5.5, Tailwind, Monaco,
  xterm. Panel-based UI (`components/*Panel.tsx`) driven from `app/page.tsx`;
  typed `invoke()` bindings in `lib/*.ts`.

## AI Provider Architecture (this session)

**Mission:** finish integrating the previously-uncommitted universal
provider system (`provider_registry.rs`, `openai_compatible.rs`,
`ProviderManager.tsx`) into the live chat pipeline as ONE registry / ONE
routing path — not a second parallel system next to the existing
Ollama-only path.

**Routing shape (implemented):**

```
Frontend (ChatPane, unchanged) → chat_with_model (unchanged public signature)
  → provider_router::resolve_provider_for_model(db, model)
      - exact match in a configured provider's `models` list → that provider
      - no match → default local Ollama provider (today's behavior, unchanged)
  → provider_router::adapter_kind_for(provider_type)
      - "ollama"                       → ai::chat_with_model_core (untouched:
                                          VRAM gate, "ollama" health key,
                                          existing log lines/tests all intact)
      - openai/openai_compatible/
        openrouter/deepseek/groq/
        together/fireworks/deepinfra/
        lmstudio/vllm/llamacpp/custom  → provider_router::stream_cloud_chat
                                          → providers::openai_compatible
                                          (ONE shared adapter for all of these
                                          — no per-company Rust files)
      - anthropic/gemini               → explicit "not yet implemented" error
                                          (no native adapter exists; the code
                                          refuses to silently mis-route these
                                          through the OpenAI-compatible client)
```

- **Files added:** `src-tauri/src/ai/provider_router.rs` (the router itself:
  adapter selection, health-key isolation per provider, and an honest
  keyword-based `TaskCapability` classifier + `select_provider_and_model_for_task`
  used for capability-driven model selection — documented in-file as a
  heuristic, not real benchmark data).
- **Files modified:** `ai/mod.rs` (`chat_with_model`/`chat_or_use_cache` now
  resolve and pass a `ProviderConfig`; Ollama's own code path is byte-for-byte
  the same function it always called), `provider_registry.rs` (exposed
  `load_providers`/`default_ollama_provider` for the router to reuse instead
  of duplicating provider-loading logic).
- **UI:** `ProviderManager.tsx` mounted inside `SettingsPanel.tsx` under a
  "Cloud & Custom Providers" section, additive to (not replacing) the
  existing Ollama/model/endpoint/effort controls. Provider cards now show
  capability badges (context length, coding/vision/tools/streaming) sourced
  from `ProviderConfig.capabilities`.
- **Ollama behavior:** unchanged by design and by test — same function,
  same health key, same VRAM gate, same log lines, same passing test suite.

**Known limitations, disclosed not hidden:**
- `ProviderConfig.api_key` is stored as plain-text JSON in the `settings`
  table (audited this session, documented in a doc comment in
  `provider_registry.rs`). No OS keychain integration exists in the crate
  yet (`Cargo.toml` has no `keyring` dependency). Flagged as a required
  follow-up before shipping cloud-provider API keys to non-technical users;
  deliberately not implemented this session (would be its own Level 3+
  change, out of scope for "finish the routing integration").
- Capability metadata (context length, coding/vision/tools/streaming) is
  provider-level, not per-model. There's no per-model speed/cost/reasoning
  score yet — `select_provider_and_model_for_task`'s heuristic uses
  provider-declared capabilities and context length as proxies, which is
  honest but coarse.
- Anthropic and Gemini native adapters are not implemented — any provider
  configured with those types will fail with a clear error at chat time,
  by design, rather than silently routing through the OpenAI-compatible
  client (which would produce wrong requests against their real APIs).
- Full interactive verification (add provider → test connection → discover
  models → chat routes through it) could only be proven through Rust unit
  tests plus browser-level UI/error-path checks this session — the actual
  `invoke()` calls require a real Tauri webview, which this environment's
  browser preview doesn't provide. Backend logic is unit-tested directly
  (12 new tests in `provider_router.rs`, including the exact task-routing
  examples requested: coding → coding-capable model, "summarize" → smallest
  context, "architecture design" → largest context).

## Completed Systems

(Carried forward from repository evidence prior to this session; not
independently re-verified in this pass beyond what's needed for the
provider-routing work — see the correction note above about trusting this
list.)

- Requirement Intelligence, Traceability Ledger + Evidence, Task DAG
  Planning, Promotion Governance, Worker Intelligence, Autonomous
  Reliability Layer — module directories present, not re-audited this
  session.
- Multi-Agent Orchestration Layer, Semantic Code Search + Multi-File
  Refactoring, Task Orchestrator IPC Integration, Agent Workbench UI — per
  git log commit titles; not independently re-audited this session.

## Current Issues

- **Pre-existing intermittent test flake (not caused by this session's
  changes):** running the full `cargo test` suite in parallel occasionally
  fails one unrelated test — observed `ai::completion::tests::pipeline_hit`
  in one run and `database::indexer::tests::index_workspace_reindexes_after_file_change`
  in another. Both pass reliably in isolation
  (`cargo test <module>:: -- --test-threads=1`). Confirmed NOT introduced by
  this session: reverting to the pre-session checkpoint commit and running
  the full suite there passed 279/279 clean, and re-running with this
  session's changes applied passed clean in 2 of 3 runs with a different,
  unrelated test failing the third time — consistent with a pre-existing
  shared-state/timing race across parallel test threads (same class of issue
  as the Windows temp-dir collision noted as "resolved" in an earlier
  version of this file — may be a recurrence or a similar-but-different
  race). Not investigated further this session (out of scope for the
  provider-routing mission); recommend test-infra hardening as a follow-up.
- **Warnings:** ~160 pre-existing `snake_case`/dead-code style warnings
  across the crate (Tauri command args using camelCase for JS-side call
  compatibility, one unused `WorkspaceService` struct). None introduced or
  touched this session; `cargo clippy --lib` reports zero errors.

## Next Recommended Actions

1. Real Tauri-runtime verification of the provider CRUD → chat routing flow
   (add a real OpenAI-compatible endpoint, e.g. a local LM Studio instance,
   confirm chat actually streams through it) — could not be done from this
   environment's browser-only preview.
2. Secure credential storage migration for `ProviderConfig.api_key` (OS
   keychain via the `keyring` crate) before any cloud provider ships to
   non-technical users.
3. Per-model capability/cost/speed metadata (currently provider-level only)
   if capability-based routing needs to get more precise than the current
   heuristic.
4. Investigate the intermittent parallel-test flake noted above.
5. AI Council — explicitly deferred by this session's mission until the
   provider routing foundation (this work) was complete. Any AI Council
   work must consume `provider_router`, not talk to providers directly, per
   `.clinerules`' adapter-reuse rule.
