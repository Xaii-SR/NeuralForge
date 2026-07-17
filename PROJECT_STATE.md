# NeuralForge Project State

This file changes often as work progresses. Permanent operating rules live
in `.clinerules`, not here — do not duplicate rules into this file.

Current repository:

C:\Users\saiah\NeuralForge

Current commit (about to be superseded by this session's commit):

ee66607 — "fix: correct architecture audit file extensions to .md"

Current branch:

master

## Agent Architecture (this session — Phase 2)

**Mission:** two full architecture audits (`docs/architecture/
NEURALFORGE_COMPLETE_PROJECT_AUDIT.md`,
`docs/architecture/NEURALFORGE_AGENT_ARCHITECTURE_AUDIT.md`) found that
`agent_v2.rs` — Neural Forge's one real, end-to-end autonomous coding
agent (real AI calls, real file writes with rollback, real `cargo check`
verification, real human-approval gate, wired to `AgentPanel.tsx`) — had
its own independent, duplicate Ollama HTTP client
(`intelligence::gateway::OllamaGateway`) instead of using
`ai::provider_router`. This session removed that duplication.

**What changed:**
- `ai::provider_router` gained `generate_for_task(providers, health, task,
  system_prompt, user_prompt) -> AppResult<String>` — a single-shot,
  non-streaming chat entry point for callers (like `agent_v2`) that just
  need a complete response string. It picks a real model by
  `TaskCapability` (Coding/Fast/Reasoning), preferring a configured
  capability-matching non-Ollama provider, falling back to local Ollama
  with a model chosen via the existing `ai::router::score_models`
  heuristic (never a hardcoded name).
- `agent_v2.rs`'s three AI call sites (architect/planner, coder, reviewer)
  now call this instead of `intelligence::router::route_through_gateway`/
  `route_with_system`. Every other line of `agent_v2.rs` — the
  `ApprovalRegistry` HITL gate, `FileExecutor::safe_write`/`rollback`,
  `WorkspaceVerifier::verify_cargo_with_stderr`, the retry loop, the
  `PayloadParser` — is byte-for-byte unchanged (diff is exactly the import
  lines + 3 call-site substitutions + a small `generate()` wiring helper).
- `intelligence::router.rs` and `intelligence::gateway.rs` deleted
  entirely — confirmed via crate-wide grep to have had exactly one
  external caller (`agent_v2.rs`) before this change, and zero after.
  `intelligence::mod.rs` no longer declares either submodule.
- `AgentPanel.tsx` and every other frontend file: **untouched**. The
  `start_agent_task`/`approve_agent_task`/`reject_agent_task` command
  signatures and `invoke()` call sites are identical — the new
  `HealthRegistry`/`DbState` dependencies are Tauri-managed state
  extracted via `app_handle.state::<T>()` inside the command body, which
  is invisible to the frontend contract.

**Verified this session:** `cargo check`/`cargo test` (291 passed, 0
failed, 10 ignored — one new ignored test added)/`cargo clippy --lib` all
clean. Added and ran (opt-in, `--ignored`) a real integration test,
`generate_for_task_falls_back_to_real_ollama_with_no_hardcoded_model`,
against this machine's actual running Ollama instance — passed, proving
the exact function `agent_v2` now calls produces a real generation with no
hardcoded model. `npm run build`/`npx tsc --noEmit` clean (no frontend
files changed, as expected).

**Not verified this session (environment constraint, not a code gap):**
the full `AgentPanel.tsx` UI flow (type a task → approve → watch it write
files → verify) end-to-end inside the actual Tauri desktop app. This
requires a real Tauri webview; this environment's browser-only preview
cannot invoke Tauri commands, and interactive screen access to drive the
native app window was not available this session. The Rust-level proof
above (real Ollama generation through the new code path, plus a diff
confirming zero changes to the file-write/approval/rollback logic) is the
strongest verification obtainable without it. Recommend a manual
`npm run tauri dev` pass through `AgentPanel.tsx` before treating this as
fully proven in the running app.

**Still open (not addressed this session, correctly out of scope for
"migrate agent_v2"):**
- `ai::inline` (Ctrl+K inline edit) and `ai::completion` (ghost text) still
  call `providers::ollama` directly rather than through
  `ai::provider_router` — a pre-existing gap documented in the agent
  architecture audit, unrelated to `agent_v2`.
- `agent_v2`'s workspace root is still hardcoded to `"."` rather than the
  actual open workspace path (`AgentRunner::process_task`'s
  `FileExecutor::new(".")`/`WorkspaceVerifier::verify_cargo_with_stderr(Path::new("."))`)
  — a correctness risk documented in the agent audit, not an AI-routing
  issue, so left untouched per this phase's scope (AI communication layer
  only).
- `agent_v2`'s reviewer response is still discarded/advisory-only (logged,
  never gates the write) — same reasoning, out of scope for this phase.
- `task_orchestrator`/`AgentWorkbench.tsx`/`multi_agent.rs` remain fully
  inert (see the agent architecture audit) — untouched, as instructed
  ("do not rewrite the agent system").
- Cloud-provider "provider switching" for `agent_v2` specifically has not
  been exercised end-to-end with a real configured cloud provider this
  session (only the Ollama fallback path was live-tested, since that's the
  zero-configuration default and no cloud provider is currently configured
  in this environment's database). The code path
  (`select_provider_and_model_for_task` → `stream_cloud_chat`) is the same
  one already unit-tested and live-verified for the main chat pipeline in
  the prior provider-routing session.

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
