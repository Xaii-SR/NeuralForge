# NeuralForge Project State

This file changes often as work progresses. Permanent operating rules live
in `.clinerules`, not here — do not duplicate rules into this file.

Current repository:

C:\Users\saiah\NeuralForge

Current commit (about to be superseded by this session's commit):

981d0f8 — "fix: guard SettingsPanel's model-load effect against a non-array response"
(pushed to `origin/master`; this file's own update commit will supersede it further)

Current branch:

master

## Credential Storage, Cloud Providers, AI Council v1, UI Refinement (this session)

Ten commits, `944cbf7`..`981d0f8`, each independently reviewed and verified
before the next started. Closes the three release priorities this file's
"Next Recommended Actions" (below) had left open: secure credential
storage, real cloud providers, and AI Council — plus two UI fixes found by
a dedicated audit pass. Summarized here in landing order; see each
commit's own message for full rationale/diff detail.

**1. `944cbf7`/`dee5f03` — Role-keyed `AgentRegistry` + `recover_task`.**
Fixed a real bug an audit surfaced in `agent_core`'s advisory lifecycle
tracker: `AgentService` was a single Tauri-managed instance shared across
*all* tasks, so concurrent tasks would interleave transitions into one
meaningless state machine. `AgentRegistry` (`agent_core/registry.rs`) now
keys by `(task_id, role)` - nested `HashMap<String, HashMap<AgentRole,
AgentService>>` - so tasks and, later, multiple named roles on the same
task cannot corrupt each other's state. `recover_task` added for evicting
a poisoned per-role entry. **This makes `AgentRegistry` a 4th in-memory
advisory state machine**, alongside the three `lifecycle.rs` already
named as deliberately separate (`agent::status`, `agent_v2::AgentState`,
`task_orchestrator::TaskLifecycle`) - still explicitly advisory-only,
never consulted for real routing decisions, and (as of this session) never
observing the same task IDs `task_orchestrator::OrchestratorTask` tracks,
since `task_orchestrator` remains a separate, inert path with no shared
entry point into `agent_core::orchestrator`'s forwarding functions. No
live drift risk today; becomes a real one only if something later wires
Council/AgentCore onto `task_orchestrator`-managed tasks - not scheduled,
but worth remembering before that happens.

**2. `8c6d7ab` — API keys moved from plaintext SQLite to the OS keychain.**
Adds `ai::credential_store` (the `keyring` crate: Windows Credential
Manager / macOS Keychain / libsecret), keyed by provider id.
`provider_registry::save_providers_raw` now redacts `api_key` to `""`
before writing to the `settings` table; `load_providers_raw` fills it back
in from the keychain. `add_provider_config`/`update_provider_config` write
the real key to the keychain; `delete_provider_config` removes it.
Verified for real against the actual Windows Credential Manager on this
machine (`cargo test credential_store -- --ignored`, all 3 keychain tests
passing: store→load round trip, missing-key-returns-empty, delete-is-
idempotent) - not just written, genuinely run.
**Standing disclosed gap, still open:** pre-existing plaintext `api_key`
rows from before this commit are **not migrated** - they read back as
`""` after upgrading, requiring a one-time re-entry per provider. Bounded,
non-silent, self-healing (re-entering the key fixes it permanently and
moves it into the keychain) - not a security hole, not data loss, but a
real one-time UX cost on upgrade that has no automated migration path.

**3. `6e1afa4` / `ae48bee` — Real Anthropic and Gemini adapters.**
`providers::anthropic`/`providers::gemini` (real `reqwest` clients:
`health_check`/`list_models`/`chat_stream`) replace their prior
`AdapterKind::Unimplemented` classification with `AdapterKind::Anthropic`/
`AdapterKind::Gemini`, each with real, adapter-appropriate capabilities
(chat/streaming/coding; no FIM - neither has a raw/FIM completion
adapter). Anthropic's wire format (`x-api-key`/`anthropic-version`
headers, top-level `system` field, Anthropic SSE event types) and
Gemini's (`?key=` query-param auth, `models/{model}:streamGenerateContent`
path, `contents`/`parts` body shape, `user`/`model` role names) are both
genuinely distinct from the OpenAI-compatible shape and from each other -
neither was forced through the shared client. `AdapterKind::Unimplemented`
is now unreachable from any live `provider_type` string (Gemini was the
last one still mapped to it) but kept as a variant for whatever native
provider needs it next.

**4. `25a580e` — Provider connection testing dispatches by `AdapterKind`.**
"Test connection" previously always ran the OpenAI-compatible health
check regardless of the configured provider - testing an Anthropic or
Gemini config silently validated the wrong endpoint. New
`provider_router::test_connection`/`test_connection_for_kind` dispatch to
each adapter's real `health_check()`; `Unimplemented` fails loudly with an
`Err` instead of a false positive/negative. New `test_provider_connection`
Tauri command; `ProviderManager.tsx` now calls it with the config's real
`provider_type` instead of always calling the OpenAI-compatible-specific
command (which is kept, unremoved, for any other caller).

**5. `4eec335` — Fixed the hardcoded `"."` workspace root.**
`task_orchestrator::orchestrator_create_task` and `agent_v2::AgentRunner::
process_task` both hardcoded `PathBuf::from(".")`/`FileExecutor::new(".")`
/`Path::new(".")` instead of the real open workspace - meaning orchestrated
tasks and the HITL retry loop were reading/writing relative to the Tauri
process's cwd, not the user's workspace. Both now read `AppState.
workspace_root` (the same `Mutex<Option<PathBuf>>` `filesystem::` already
uses) and fail clearly (`"no workspace open..."`) rather than silently
defaulting to `.` if none is open. This was a prerequisite fixed
deliberately *before* Council v1 (item 6) was built on top of the same
registry, so Council wouldn't inherit the same class of bug.

**6. `95f500b`/`d3735c2` — AI Council v1: real sequential Architect→Critic→Judge pass.**
`agent_core::orchestrator::run_council_pass_with` is the testable
sequencing core - generic over how a role's response is obtained, so
ordering and failure-halting are unit tested without a live model or
`AppHandle` (this codebase already can't construct a live `AppHandle` in
tests - see the "MockRuntime decision" referenced in `agent/mod.rs`).
`run_council_pass` is the thin real wrapper, resolving `providers`/
`health` from `AppHandle` the same way `agent_v2`'s private `generate()`
does, then calling real `ai::provider_router::generate_for_task` per role
- no mocked LLM response in the production path. Each role
(`AgentRole::Architect`/`Critic`/`Judge`, added to `agent_core::types`,
distinct from `multi_agent::AgentRole`'s differently-named dead-code enum)
registers in `AgentRegistry` only once the previous role has genuinely
succeeded; any role's failure halts immediately with `CouncilError`
naming exactly which role and why - later roles are never called or
registered. **Live-verified against a real local Ollama instance**
(`live_council_pass_produces_a_real_verdict_from_ollama`, `#[ignore]` by
default, actually run: real sequential pass, real non-`Unclear` verdict,
~9s). New `run_council_pass` Tauri command registered in `lib.rs`.

**7. `4011bf9` — Minimal frontend wiring for Council v1.**
`lib/council.ts` (typed `invoke()` wrapper, mirrors `lib/orchestrator.ts`'s
pattern) + `components/CouncilPanel.tsx` (task_id + objective inputs, Run
Council button, Architect/Critic/Judge output display with a verdict
badge, using the existing `Spinner`/`ErrorBanner` primitives - no new UI
components invented). Wired into `app/page.tsx`'s existing bottom-tab
pattern as a new "Council" tab. Verified in a real dev-server browser
session: tab renders, inputs work, clicking "Run Council" without a live
Tauri backend correctly shows the expected `invoke`-undefined error via
`ErrorBanner` (not a crash) - proving the UI wiring itself is correct.
**Standing disclosed gap, still open:** the actual real-Tauri-app,
real-IPC, real-Ollama, real-verdict-rendered click-through has **never
been human-observed**. Desktop-control access was explicitly requested and
denied mid-session (user declined the permission dialog - the correct
outcome to respect, not retried). Everything *around* this specific path
is proven (backend live against Ollama in item 6; frontend render/
interaction/error-handling in-browser; the real Tauri app confirmed
booting with `run_council_pass` registered via server logs; the IPC
payload contract statically cross-checked field-by-field against the
JS↔Rust naming/serde conventions) - but the one hop connecting all of it
in the real running app has not been clicked. Whoever has desktop access
should do this once before treating Council v1 as fully proven, not just
soundly engineered.

**8. `be611cb` — Wired the dead `AgentWorkbench.tsx`, added the missing FIM badge.**
Audit found `AgentWorkbench` imported in `app/page.tsx` but never rendered
anywhere in the file (dead code, not broken code - its own internals call
real, registered `task_orchestrator::*` commands). Further audit found it
offers real, unique capability - a live event-driven (`listenOrchestratorState`)
phase timeline plus HITL approve/reject/cancel over `task_orchestrator`'s
task lifecycle - that neither `AgentPanel` (governed `agent::`/`agent_v2`)
nor `CouncilPanel` (stateless single-shot) expose. Wired in as a new
"Workbench" tab rather than deleted; its `streamOutput`/`knowledgeResults`
state is confirmed genuinely inert (declared, never set by any handler)
but harmless, left as-is per "don't redesign it." Separately,
`ProviderManager.tsx`'s capability badges (context length, Coding, Vision,
Tools, Streaming) were found to omit `fim` entirely despite the backend
enforcing it strictly (Ollama `true`; every other adapter `false`) - one
badge added, plus the missing `fim` field added to `lib/providers.ts`'s
`ProviderCapabilities` TS interface (it didn't exist there at all -
compile-blocking, not scope creep). Verified by mocking
`window.__TAURI_INTERNALS__.invoke` in a real browser session with
controlled fake provider data: a `fim:true` Ollama config renders the
badge, a `fim:false` Anthropic config doesn't - real component logic
exercised, not just a static code read.

**9. `981d0f8` — Guarded `SettingsPanel`'s model-load effect.**
Found while building the FIM-badge mock above: `SettingsPanel.tsx`'s
`ai.listModels()`-driven effect had no guard against an unexpected
resolved shape - `models.map(...)` on a non-array would throw inside
`.then()` with no catch, hanging the panel on "Loading..." forever with
no visible signal. (Audited and confirmed: the real backend `list_models`
command always resolves to a real array or rejects, so this specific
shape was never reachable via today's real IPC - it surfaced only because
of the mock - but the missing guard was real regardless, for future
backend changes or IPC drift.) Fix distinguishes a genuine rejection
(Ollama not running - expected, still falls back silently to
`FALLBACK_MODELS`, unchanged) from a resolved non-array value (now a real,
retryable error via the existing `ErrorBanner`). Verified both directions
with the exact reproduction: mocking `list_models` to resolve `null` now
shows the error banner instead of hanging; mocking it to reject still
falls back silently with no regression.

**Verified across this session as a whole:** every commit's own
`cargo test`/`cargo check`/`npx tsc --noEmit`/`npm run build` output is
recorded in its commit message or the review that approved it. Backend
test count grew from a 311-passed baseline (start of this session) to
**353 passed / 0 failed / 19 ignored** by commit `d3735c2`, holding
through `981d0f8`. One pre-existing parallel-test flake
(`database::indexer::tests::index_workspace_reindexes_after_file_change`,
same root cause already documented below in "Current Issues") and one
new flake of the same class
(`ai::completion::tests::cache_miss_diff_file`) were both observed,
reproduced, and confirmed unrelated to this session's changes via
isolation + clean rerun.

**Still open after this session (disclosed, not blocking, cheap to close):**
- The two standing gaps named above: credential migration (item 2) and
  the Council desktop click-through (item 7).
- Provider ecosystem: `openrouter`/`groq`/`mistral`/`together`/
  `fireworks`/`deepseek`/`huggingface` still route generically through
  `AdapterKind::OpenAiCompatible`. The protocol choice
  (`Authorization: Bearer`, `/v1/chat/completions`,
  `choices[].delta.content` SSE) is the correct, standard OpenAI-SDK-
  compatible convention these vendors deliberately implement - not a
  defect - but it has **zero live testing** against any of them this
  session (only LM Studio, via the pre-existing `#[ignore]`d
  `local_openai_compatible_chat` test, itself never run in this
  environment for lack of a local LM Studio server). `huggingface`
  specifically is a real footgun if pointed at HF's classic (non-OpenAI-
  shaped) Inference API rather than their newer OpenAI-compatible router -
  fails loudly with a clear status error, not silently, but still worth a
  user-facing note eventually.
- This file itself: earlier phase sections below (`AI Provider
  Architecture`, `AgentCore Scaffold`) contain claims this session's work
  supersedes - corrected in place below rather than left stale, but their
  original "as of that session" wording is preserved for history where it
  doesn't actively mislead.

## AgentCore Scaffold (Phase 6A)

**Mission:** stand up `agent_core` as a real, compiling, tested coordination
shell above the two existing execution authorities (`agent::`, `agent_v2`)
found by the Phase 6 audit (`docs/architecture/NEURALFORGE_AGENT_ARCHITECTURE_AUDIT.md`
is now stale on this point - update superseded by this session's findings,
not that doc). No lifecycle migration, no absorption of provider/filesystem/
rollback/verification logic, no frontend or frozen-file changes.

**What was added:** `src-tauri/src/agent_core/{mod.rs, lifecycle.rs,
orchestrator.rs, commands.rs}`.
- `lifecycle.rs` — `ExecutionBackend { Governed, V2 }` only. Explicitly NOT
  a unified task-lifecycle model (`agent::status`/`agent_v2::AgentState`/
  `task_orchestrator::TaskLifecycle` remain three separate, untouched
  representations - merging them is deferred, per this phase's scope).
- `orchestrator.rs` — pure forwarding functions, one per existing
  `agent::`/`agent_v2` command, each calling the existing public function
  directly and recording which backend handled the task id into
  `AgentCoreState`. Owns zero execution logic.
- `commands.rs` — the future Tauri command bodies, calling `orchestrator::`.
  **Not yet `#[tauri::command]`-annotated and not registered in `lib.rs`**
  - adding new IPC surface is a distinct, reviewable change deferred to
    whenever Phase 6B is approved, per the phase's explicit stop condition.
- `mod.rs` — `AgentCoreState { task_backends: Mutex<HashMap<String,
  ExecutionBackend>> }`. Minimal by design: no natural slot existed in
  `AppState` (confirmed by inspection - it holds only `workspace_root`),
  so this is a new, separate managed-state candidate, not merged into
  `AppState`. **Not yet `.manage()`d** - `lib.rs`'s only change this phase
  is the `mod agent_core;` declaration itself.

**Confirmed before writing any code (per the directive's checklist):**
1. All 5 `agent::` commands and all 3 `agent_v2` commands are `#[tauri::command]`
   attributes directly inline on `pub async fn`/`pub fn` - confirmed by
   grep, no separate command-definition layer existed to relocate.
2. Because of #1, "introduce wrappers" meant **only adding new forwarding
   functions** - zero attribute moves, zero source changes to `agent/mod.rs`
   or `agent_v2.rs` were needed. Both remain byte-for-byte unchanged this
   phase (confirmed: not in the diff).
3. `AppState` has no natural AgentCore slot (verified by reading
   `core/state.rs` in full - one field, `workspace_root`).
4. `lib.rs`'s only change is the `mod agent_core;` declaration (plus an
   `#[allow(dead_code)]` on that line, since nothing invokes this code yet
   - an honest, temporary allow, not a hidden warning suppression; remove
   it when Phase 6B wires registration).

**Verified this session:** `cargo check`/`cargo clippy --lib` clean.
`cargo test`: **306 passed, 0 failed, 13 ignored** (3 new: `ExecutionBackend`
naming stability, `AgentCoreState` backend-recording, empty-state
construction). `npm run build`/`npx tsc --noEmit` clean (zero frontend
files touched, zero new IPC surface exposed to touch them with).

**Actual diff, confirmed against the phase's own predicted diff:**
```
NEW:   src-tauri/src/agent_core/{mod.rs,lifecycle.rs,orchestrator.rs,commands.rs}
CHANGED: src-tauri/src/lib.rs  (+6 lines: mod declaration + comment)
UNCHANGED: agent/mod.rs, agent/planner.rs, agent/executor.rs, agent_v2.rs,
           provider_router.rs, providers/, task_orchestrator.rs,
           multi_agent.rs, every frontend file
```
Even more minimal than anticipated — the "POSSIBLE WRAPPER CHANGES" to
`agent/mod.rs`/`agent_v2.rs` the directive flagged as conditionally
necessary turned out not to be needed at all, since every function being
forwarded to was already `pub`.

**Stop condition honored:** shell exists, compiles, is tested, and
forwards correctly (proven by `record_backend_is_queryable_after_recording`
and the fact that `orchestrator::*` functions type-check against the real
`agent::`/`agent_v2` signatures). **Phase 6B (wiring `commands.rs` into
`lib.rs`'s `generate_handler!`, actually pointing any frontend affordance
at it) has not been started**, per the explicit instruction to stop after
the shell compiles and await review of this diff.

**Open questions carried into Phase 6B (from the Phase 6 audit, still
unresolved):** whether AgentCore is meant to eventually unify Path A
(`agent::`) and Path B (`agent_v2`) into one execution engine, or simply
continue coordinating between two selectable backends indefinitely; the
real Approval/Rollback/Verification duplication between the two backends
(documented in the audit's Safety System Audit table) is untouched by this
scaffold and remains exactly as fragmented as before.
## Provider Architecture Hardening (Phase 5)

**Mission:** move from implicit, scattered name-based provider dispatch to
a strictly capability-driven architecture where `provider_router` cares
only about *what* a provider can do, never *which* provider it is by
string comparison - and close a real capability/adapter mismatch the audit
found (providers could declare `chat: true` while having no working
adapter at all).

**What changed:**
- `AdapterKind` + `adapter_kind_for` moved from `provider_router.rs` to
  `provider_registry.rs` (the single, canonical definition now - confirmed
  via crate-wide grep there is exactly one `pub enum AdapterKind` and one
  `pub fn adapter_kind_for`). This is registry-level data classification
  ("what can this persisted `provider_type` do?"), and living there lets
  the registry enforce capability clamping at construction time without
  depending on the router module.
- `ProviderConfig::adapter_kind(&self) -> AdapterKind` added - the one
  method every routing decision now calls instead of re-deriving
  `provider_type == "ollama"` independently.
- **New enforcement point**: `max_capabilities_for(kind: AdapterKind) ->
  ProviderCapabilities` (the ceiling each adapter kind can ever truthfully
  support) and `clamp_capabilities(provider_type, requested) ->
  ProviderCapabilities` (ANDs requested capabilities against that ceiling,
  mins `context_length` against it). Every `ProviderConfig` construction
  path now routes through this: `ProviderConfig::default()`,
  `default_ollama_provider()`, and a new `build_provider_config` helper
  extracted from `add_provider_config` specifically so it's directly
  unit-testable without a Tauri runtime.
- **Real bug fixed**: before this phase, `add_provider_config` with
  `provider_type: "anthropic"` or `"gemini"` produced a config with
  `chat: true, streaming: true, coding: true` (from
  `ProviderCapabilities::default()`), despite `adapter_kind_for` already
  classifying those as `Unimplemented` (no working adapter). Nothing
  enforced the two staying consistent. `Unimplemented` now rejects every
  capability unconditionally via `max_capabilities_for`.
- **Consolidated** the 5 previously-scattered `provider_type == "ollama"` /
  `!= "ollama"` checks (4 in `provider_router.rs`, 1 in `ai/mod.rs::chat_or_use_cache`)
  into `config.adapter_kind() == AdapterKind::Ollama` calls against the one
  canonical classification. `ai/mod.rs` required a one-line touch for
  this - outside the directive's literal file list, but explicitly required
  by "Consolidate Decisions," and the audit had already identified that
  exact site as one of the scattered checks. Flagged here rather than
  silently included.
- **Routing transparency**: `tracing::debug!` added at every resolution
  decision point - `resolve_provider_for_model` (which provider/type was
  picked for a model), `select_provider_and_model_for_task` (candidate
  count + selection), `select_fim_provider` (rejected provider names +
  selection), `complete_fim` (adapter-missing rejection / Ollama
  fallback), and `stream_cloud_chat` (final adapter dispatch decision).
- `AdapterKind::OpenAiCompatible`'s `max_capabilities_for` entry
  explicitly caps `fim: false` - documents in one place, structurally
  (not just in a comment), that no OpenAI-compatible FIM adapter exists
  yet, consistent with Phase 4's `complete_fim` rejection behavior.

**Verified this session:** `cargo check`/`cargo clippy --lib` clean.
`cargo test`: **303 passed, 0 failed, 13 ignored** (7 new tests: capability
clamping for the Mismatch/Unimplemented/Ollama-regression cases required
by this phase, plus adapter-kind classification tests). `npm run build`/
`npx tsc --noEmit` clean (zero frontend files touched). Live verification
against this machine's real running Ollama instance: reran every
`--ignored` Ollama-dependent test in the suite (chat, FIM, agent_v2's
planner, inline edit, cache) - all passed, proving zero regression in the
one fully-functional path. Two unrelated pre-existing failures observed
and confirmed not caused by this phase: `openai_compatible::tests::local_openai_compatible_chat`
(requires a real LM Studio server on `localhost:1234`, not running in this
environment - same as every prior session) and
`bootstrap::suggest::tests::choose_target_proposes_a_real_target_from_local_model`
(live-model-output-format flake, unrelated file, not touched this phase).

**Test-requirement mapping (as specified):**
- *Test 1 (Mismatch)*: `clamp_capabilities_sanitizes_unsupported_capability_for_openai_compatible`
  + `build_provider_config_never_produces_a_capability_adapter_mismatch` -
  requesting `fim: true` on an `openai_compatible` config is silently
  sanitized to `false`; genuinely-supported capabilities pass through
  unchanged.
- *Test 2 (Adapter Enforcement)*: `unimplemented_adapter_rejects_every_requested_capability`
  + `build_provider_config_gives_unimplemented_adapter_zero_capabilities` -
  `anthropic`/`gemini` configs reject all 8 capability flags and
  `context_length`, both via the pure clamp function and the real
  construction path.
- *Test 3 (Ollama Regression)*: `default_ollama_provider_capabilities_unaffected_by_clamping`
  (unit) + full live-Ollama test suite rerun (integration, see above) -
  Ollama's `chat/streaming/coding/fim: true` survive clamping unchanged,
  and every real Ollama code path still functions end to end.

**Still open (not addressed this session, correctly out of scope):**
- `providers/mod.rs`'s legacy `ProviderId`/`registry()`/`has_api_key()`
  system and `router::estimate_cost`'s hardcoded `&ProviderId::Ollama`
  bug (found in this phase's audit) remain **untouched**, per explicit
  "No Legacy Cleanup" instruction.
- `update_provider_config` still has no way to change `provider_type` or
  request capability changes post-creation - not a live risk today (can't
  introduce a mismatch that doesn't already get clamped at creation), but
  worth a follow-up if capability editing is ever exposed to the frontend.
- API key plaintext storage (flagged Phase 4) remains unaddressed.
- Everything previously flagged out of scope in Phases 3-4 (`docs.rs`/
  `web.rs`'s unrelated `reqwest`, `bootstrap/environment.rs`'s boot ping,
  `agent_v2`'s hardcoded workspace root/advisory reviewer, the inert
  `task_orchestrator`/`AgentWorkbench.tsx`/`multi_agent.rs` stack) remains
  unaddressed and was out of scope again this phase (explicitly forbidden:
  `agent_v2`, `task_orchestrator`, `multi_agent`, `AgentCore`).
## Autocomplete + Ghost Text FIM Consolidation (Phase 4)

**Mission:** eliminate the last confirmed AI-provider bypass
(`ai::autocomplete.rs`) and retroactively bring `ai::completion.rs`'s FIM
path (built in Phase 3, before this stricter rule existed) fully under
`provider_router`'s authority for selection/resolution/health/telemetry,
via a new capability-gated FIM abstraction.

**What changed:**
- `provider_registry::ProviderCapabilities` gained a `fim: bool` field
  (default `false`). `default_ollama_provider()` now sets `fim: true` —
  Ollama is the only provider with a real, working FIM adapter today.
- `provider_router` gained `select_fim_provider(providers)` (picks a
  configured, enabled, non-Ollama provider that explicitly declares
  `capabilities.fim = true`, smallest-context-first as a speed proxy) and
  `complete_fim(providers, health, prompt, num_predict, temperature)` — the
  single capability-gated entry point for raw/FIM completion. A
  FIM-capable non-Ollama provider is honored by selection but currently
  rejected with a clear, named error at execution time (no adapter
  implements FIM for any non-Ollama `provider_type` yet — same honest
  pattern as Anthropic/Gemini's `Unimplemented` chat routing, not a silent
  drop or mis-dispatch). The common/default case (nothing advertises FIM)
  falls straight through to real local Ollama via `providers::ollama::
  generate_raw`, model chosen by the existing `ai::router::score_models`
  speed heuristic — never hardcoded.
- `ai::autocomplete::fetch_ghost_suggestion` rewritten: no more
  `reqwest::Client`, no hardcoded `http://localhost:11434/api/generate`, no
  hardcoded `"qwen2.5-coder:1.5b"`. Now resolves the live provider list and
  calls `provider_router::complete_fim`. Gained `health`/`db` Tauri-managed
  state params (invisible to the frontend `invoke()` call — same pattern as
  every prior phase's migrations); FIM prompt formatting, fence-stripping,
  and "empty string on failure" behavior all preserved exactly.
- `ai::completion::call_ollama_fim` (and its callers `async_stream_completion`/
  `request_async_completion`) now route through `provider_router::complete_fim`
  instead of calling `ollama::list_models`/`ollama::generate_raw` directly
  (which is what Phase 3 had left it doing) — closes the retroactive gap
  noted in this phase's audit.
- Frontend: **untouched**. Confirmed during audit that `request_async_completion`
  has no frontend caller at all today (the live, user-visible ghost text
  path is `fetch_ghost_suggestion` via `hooks/useGhostText.ts`'s `suggestion`
  state, wired into Monaco's `registerInlineCompletionsProvider` in
  `Editor.tsx`) — its debounce/cancellation/staleness behavior is exactly
  as before.

**Verified this session:** `cargo check`/`cargo clippy --lib` clean.
`cargo test`: **296 passed, 0 failed, 13 ignored** (5 new tests: 4 sync
`select_fim_provider`/rejection-path tests + 1 new live-only test).
`npm run build`/`npx tsc --noEmit` clean (zero frontend files touched).
Two live integration tests run against this machine's actual running
Ollama instance, proving the exact migrated paths work, not just compile:
`provider_router::tests::complete_fim_falls_back_to_real_ollama_when_no_provider_advertises_fim`
and the Phase 3 `generate_raw_produces_real_fim_completion` test (still
passing, now reached via `complete_fim` rather than a direct call).

**Verification grep note:** the phase's literal instruction
(`grep -R "ollama::" src-tauri/src/ai/` etc. expecting 0 matches) is
unsatisfiable as written — those greps scan the whole `ai/` directory,
which necessarily includes `provider_router.rs` and `providers/ollama.rs`
themselves, and per this same phase's own ownership rule
("`provider_router` remains the only authority... Adapters only execute
requests"), the router *must* call the adapter. Ran the checks scoped to
what was actually meant — `autocomplete.rs` and `completion.rs` — where
all four (`reqwest`, `generate_raw`, `ollama::`, `localhost:11434`)
correctly return zero matches. Also ran the literal whole-directory
versions for the record; all hits are pre-existing, legitimate adapter/
router internals or unrelated files (`docs.rs`/`web.rs`'s own
non-AI-generation `reqwest` usage), none newly introduced.

**Step 3 (OpenAI-compatible "foundation") was found already complete
during audit** and intentionally not re-scaffolded: `OpenAiCompatibleProvider`
(real `health_check`/`list_models`/`chat_stream`, SSE-parsing, tested) has
existed since an earlier session, is already dispatched by
`stream_cloud_chat`/`AdapterKind::OpenAiCompatible`, and is already
creatable end-to-end via `provider_registry::add_provider_config` with
`provider_type: "openai_compatible"` — `ProviderManager.tsx` already
defaults new providers to it. No FIM support was added for it this phase
(would require a new adapter method in `providers/openai_compatible.rs`,
which was outside this phase's file scope) — see "Still open" below.

**Still open (not addressed this session):**
- No non-Ollama provider has a working FIM adapter. `complete_fim` will
  correctly and honestly reject one that claims `fim: true` today (nothing
  sets that flag except `default_ollama_provider()`, so this is inert
  until either a user or a future phase adds one) — implementing real
  OpenAI-compatible-style FIM (legacy `/v1/completions`, where supported)
  is a real future task, out of this phase's file scope
  (`providers/openai_compatible.rs` wasn't in it).
- `lib/providers.ts`'s `ProviderCapabilities` TypeScript interface doesn't
  yet include the new `fim` field — harmless (extra JSON field is silently
  ignored by existing untyped consumption), but a type-accuracy gap for
  whenever the frontend needs to read it. Frontend was out of this phase's
  scope.
- Everything previously flagged as out of scope in Phase 3
  (`docs.rs`/`web.rs`'s unrelated `reqwest` usage, `bootstrap/environment.rs`'s
  boot-time Ollama ping, `agent_v2`'s hardcoded workspace root and
  advisory-only reviewer, the fully-inert `task_orchestrator`/
  `AgentWorkbench.tsx`/`multi_agent.rs` stack) remains unaddressed and was
  explicitly out of scope again this phase.
## Inline Edit / Ghost Text Provider Migration (Phase 3)

**Mission:** the agent architecture audit flagged `ai::inline` (Ctrl+K
inline edit) and `ai::completion` (ghost text) as the two remaining
AI-generation paths bypassing `ai::provider_router`. This phase migrated
both — audit-first, confirmed via grep and full file reads that both
genuinely bypassed the router before touching anything.

**What changed:**
- `ai::provider_router` gained `stream_chat(health, config, model, messages,
  on_token) -> AppResult<String>` — a fast-path streaming dispatcher for
  callers that already have a resolved `ProviderConfig` (via
  `resolve_provider_for_model`, called synchronously before this, same
  Connection-across-await constraint as `generate_for_task`). Ollama goes
  straight to `providers::ollama::chat_stream`; everything else delegates
  to the existing `stream_cloud_chat`. No second routing system, per the
  phase's fast-path requirement.
- `providers::ollama` (the sanctioned Ollama adapter) gained `generate_raw`,
  covering `/api/generate` (raw/FIM completion) — a genuinely different API
  shape than `/api/chat`, needed for ghost text's fill-in-middle prompting,
  which isn't expressible as a chat message list. This completes the
  existing adapter's surface; it is not a new adapter or a new HTTP client.
- `ai::inline::stream_inline_edit` now resolves its provider via
  `provider_router::resolve_provider_for_model` and streams via the new
  `stream_chat`, removing its manual `health.is_healthy("ollama")`/
  `record_success`/`record_failure` duplication and its manual
  `Arc<Mutex<String>>` accumulation (both now handled inside
  `provider_router`). Gained a `db: State<'_, DbState>` parameter
  (Tauri-managed, invisible to the frontend `invoke()` call, same pattern
  as the `agent_v2` migration).
- `ai::completion::call_ollama_fim` now calls `ollama::generate_raw`
  instead of constructing its own `reqwest::Client` against a hardcoded
  `http://localhost:11434/api/generate`. The previously-hardcoded
  `"qwen2.5-coder:1.5b"` model is replaced by a live `ollama::list_models()`
  lookup scored with `ai::router::score_models`'s existing "speed" goal
  heuristic (the same one `provider_router::generate_for_task` already uses
  for Fast-classified tasks) — reused, not duplicated.
- Frontend: **untouched**. `Editor.tsx`'s Ctrl+K flow and ghost-text
  trigger logic, and every Tauri command's public signature/`invoke()` call
  site, are unchanged.

**Verified this session:** `cargo check`/`cargo test` (291 passed, 0
failed, 12 ignored — 2 new ignored tests added)/`cargo clippy --lib`/
`npm run build`/`npx tsc --noEmit` all clean. Added and ran (opt-in,
`--ignored`) two real integration tests against this machine's actual
running Ollama instance:
`provider_router::stream_chat_streams_real_tokens_for_resolved_ollama_config`
(proves the exact path `inline.rs` now calls streams real tokens) and
`providers::ollama::generate_raw_produces_real_fim_completion` (proves the
exact path `completion.rs` now calls produces a real FIM completion) — both
passed.

**Additional provider-bypass module discovered, not modified this
phase:** `ai::autocomplete.rs` also constructs its own `reqwest::Client`
against a hardcoded `http://localhost:11434/api/generate` — the same
pattern `completion.rs` had. It was not named in this phase's scope (only
`inline.rs`/`completion.rs` were), so it was left untouched per "smallest
safe change" discipline and is flagged here for a future phase rather than
folded in opportunistically.

**Still open (unrelated to this phase, correctly out of scope):**
- `ai::docs.rs`/`ai::web.rs` construct their own `reqwest::Client`s, but
  for fetching documentation pages / web search results — not AI-provider
  chat calls, so not a "provider bypass" in the sense this phase (or the
  provider-routing mission generally) is concerned with.
- `bootstrap/environment.rs` pings a hardcoded `127.0.0.1:11434` at boot as
  an environment-readiness gate (not a generation call) — same reasoning,
  not in scope.
- `agent_v2`'s hardcoded `"."` workspace root and discarded/advisory-only
  reviewer step (documented in the agent architecture audit) remain
  unaddressed — orthogonal to AI routing.
- `task_orchestrator`/`AgentWorkbench.tsx`/`multi_agent.rs` remain fully
  inert (documented in the agent architecture audit) — untouched.

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
      - anthropic                      → providers::anthropic (real native
                                          /v1/messages adapter, added this
                                          session - see top section item 3)
      - gemini                         → providers::gemini (real native
                                          streamGenerateContent adapter,
                                          added this session - see top
                                          section item 3)
```
**[Corrected - superseded by this session]** The diagram above reflects
this file's original point in time, when neither adapter existed and both
failed loudly by design. As of this session, both are real; see the top
section.

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
- **[Corrected - superseded by this session, see the top section]**
  `ProviderConfig.api_key` was plain-text JSON in the `settings` table
  when this section was originally written. As of commit `8c6d7ab` (this
  session), it is stored in the OS keychain via the `keyring` crate
  (`Cargo.toml` now has the dependency) - `settings` only ever holds a
  redacted `""`. Pre-existing plaintext rows from before that commit are
  not auto-migrated (see the top section's item 2) - the only part of
  this original bullet still true.
- Capability metadata (context length, coding/vision/tools/streaming) is
  provider-level, not per-model. There's no per-model speed/cost/reasoning
  score yet — `select_provider_and_model_for_task`'s heuristic uses
  provider-declared capabilities and context length as proxies, which is
  honest but coarse.
- **[Corrected - superseded by this session, see the top section]**
  Anthropic and Gemini native adapters did not exist when this section was
  originally written. As of commits `6e1afa4`/`ae48bee` (this session),
  both have real adapters (`AdapterKind::Anthropic`/`AdapterKind::Gemini`).
  `AdapterKind::Unimplemented` is now unreachable from any live
  `provider_type` string, kept only as a variant for whatever native
  provider needs it next.
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

**[Corrected - this list is stale relative to this session's work; see the
top section for what actually landed.]**

1. ~~Secure credential storage migration for `ProviderConfig.api_key`~~ -
   **done** (commit `8c6d7ab`, OS keychain via `keyring`). One real gap
   remains: pre-existing plaintext rows aren't auto-migrated (top section,
   item 2) - a one-time re-entry cost, not a missing feature.
2. ~~AI Council~~ - **done** (commits `95f500b`/`d3735c2`/`4011bf9`,
   real sequential Architect→Critic→Judge pass, live-verified against
   Ollama, minimal frontend wiring). One real gap remains: the actual
   desktop-app click-through has never been human-observed (top section,
   item 7) - a manual verification step, not a development task.
3. Real Tauri-runtime verification of the provider CRUD → chat routing
   flow for a genuine third-party OpenAI-compatible vendor (OpenRouter/
   Groq/Mistral/etc., not just LM Studio) - still not done; see top
   section's "Still open" for the specific risk.
4. Per-model capability/cost/speed metadata (currently provider-level
   only) if capability-based routing needs to get more precise than the
   current heuristic - unchanged, still open.
5. Investigate the intermittent parallel-test flake noted above (now
   observed recurring across two more sessions with a different specific
   test each time - same root cause, still not investigated).
6. UI refinement - largely closed this session (dead `AgentWorkbench` tab
   wired in, FIM capability badge added, `SettingsPanel` load-guard fixed
   - see top section items 8-9). No further gaps found by the dedicated
   audit pass that produced those fixes.
