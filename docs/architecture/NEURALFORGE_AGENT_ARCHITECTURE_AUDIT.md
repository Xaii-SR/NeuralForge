# NEURAL FORGE AGENT & EXECUTION SUBSYSTEM AUDIT

Version: 1.0
Date: 2026-07-17
Current Commit: `2d898f9` (working tree clean at time of audit; this audit
itself adds only documentation)
Companion documents: `docs/architecture/NEURALFORGE_COMPLETE_PROJECT_AUDIT.md`,
`PROJECT_STATE.md`, `.clinerules`

This is a read-only investigation. No source files were modified, moved, or
deleted to produce this report.

---

## HEADLINE FINDING (read this first)

**Two separate, fully-built "autonomous agent" stacks exist. Only one of
them is reachable from the running application, and it is the older,
less-polished one. The newer stack, including its dedicated UI
(`AgentWorkbench.tsx`), is entirely unreachable — not because it's broken,
but because nothing renders it and its Tauri commands never invoke the
functions that do real work.**

| | **Stack A — `agent_v2`** (real) | **Stack B — `task_orchestrator` / `multi_agent`** (inert) |
|---|---|---|
| Frontend | `AgentPanel.tsx` — **rendered** in `page.tsx`'s bottom tab bar | `AgentWorkbench.tsx` — **imported but never rendered anywhere** |
| Calls real AI | Yes — `intelligence::gateway::OllamaGateway` (a fourth, independent Ollama HTTP client) | No — zero AI/LLM calls anywhere in `task_orchestrator.rs`, `planning_engine.rs`, `agent_controller.rs`, or `change_executor.rs` |
| Writes files | Yes — `FileExecutor::safe_write` (real `fs::write`, with path-traversal guard and content backup) | No — `change_executor.rs`'s `PatchApplier::apply` is a stub that always returns `Ok(vec![])` and touches nothing |
| Verifies changes | Yes — real `cargo check` subprocess, real retry-with-compiler-error-feedback loop (up to `MAX_RETRIES = 3`) | No — no verification command is ever invoked from the orchestrator's Tauri commands |
| Human approval | Yes — real `tokio::sync::oneshot` channel gate, wired to `approve_agent_task`/`reject_agent_task` commands `AgentPanel.tsx` actually calls | Cosmetic — `orchestrator_approve_task` only flips an enum from `AwaitingApproval` to `Executing{current_step:0,...}` and returns; nothing downstream ever reads that transition to do work |
| Routes through `ai::provider_router` | **No** — bypasses it entirely via its own `OllamaGateway` | N/A — no AI calls exist to route |
| Multi-agent roles (Research/Coding/Testing/Review) | No — single Coder + single Reviewer prompt pair | `multi_agent.rs` defines the full Supervisor + 4-role structure, but **has zero callers anywhere in the crate** (not registered as a Tauri command, not called by `task_orchestrator`, not called by anything) |

The practical implication: **Neural Forge's one real, file-writing,
AI-driven autonomous coding agent today is `agent_v2` via `AgentPanel.tsx`,
and it does not use the provider-routing architecture the previous session
built.** Any AI Council work that assumes `task_orchestrator`/`multi_agent`
is the live agent substrate would be building on top of dead code.

---

## 1. CURRENT AGENT ARCHITECTURE

There are, by rough count, **four distinct "agent" or "planning" concepts**
in this codebase, each with its own module, its own state enum, and (in
three of four cases) its own test suite. They are not layers of one
system — they are parallel, largely non-interacting implementations built
at different times:

1. **`agent/`** (`executor.rs`, `planner.rs`, `memory.rs`) — the original,
   `.clinerules`-frozen governed pipeline: single-file LLM-driven content
   planning (`planner::plan_change`, real Ollama call) → sandboxed apply +
   verify + auto-rollback-on-failure (`executor::apply_and_verify`) → ledger/
   promotion bookkeeping (`governance::`). **Real, heavily tested** (exercised
   by `governance/mod.rs`, `planning/mod.rs`, `release_validation.rs`,
   `bootstrap/mod.rs` — dozens of call sites), but IPC-exposed only via
   `agent::create_and_plan_task` / `create_and_plan_code_task` /
   `approve_task` / `reject_task` / `list_agent_tasks` (5 commands in
   `lib.rs`). Not driven by any visible dedicated UI panel found in this
   audit — see §3 for what's actually reachable.

2. **`agent_controller.rs`** + **`planning_engine.rs`** — a newer, from-scratch
   five-phase state machine (`Idle → Analyzing → Planning → Executing →
   Observing → Verifying → Completed/Failed`) with its own `AgentContext`/
   `TaskPlan`/`Subtask` types, unrelated to `agent/`'s types. Planning here
   is **real but non-AI**: `PlanningEngine::analyze_task` does keyword
   splitting on the user's request string and `decompose_task` emits one
   heuristic subtask per affected file ("Modify `{file}` to implement the
   change"); verification is hardcoded to `["cargo check", "cargo test"]`
   as plan metadata, never actually executed by this module. No LLM is
   called anywhere in either file.

3. **`task_orchestrator.rs`** — a higher-level wrapper around
   `agent_controller` + `planning_engine` + `context_retrieval` +
   `error_analyzer` + `knowledge_store`, adding its own `TaskLifecycle` enum
   (yet a third phase enum, structurally similar to but distinct from
   `AgentPhase`). Its real logic (`analyze`, `plan`, `execute_step`,
   `observe`, `recover`, `verify`, `persist`, `restore`) is fully
   implemented and unit-tested — **but only `create_task`, `transition`,
   and `cancel` are ever reached from the 6 registered Tauri commands.**
   `execute_step`, `observe`, `recover`, `verify`, `persist`, and `restore`
   have zero callers outside `task_orchestrator.rs`'s own `#[cfg(test)]`
   module. See §3 for the exact command-by-command breakdown.

4. **`agent_v2.rs`** — the actual working autonomous coding agent (see
   headline finding). Its own `AgentState` enum (a fourth phase enum),
   its own `AgentTask` type, its own approval-gate mechanism
   (`ApprovalRegistry`, real `tokio::oneshot` channels — the only one of
   the four stacks with genuine human-in-the-loop blocking, not just a
   cosmetic state flip).

**`multi_agent.rs`** (Supervisor + Research/Coding/Testing/Review
specialized agents, per its own doc comment) sits on top of #2/#3's types
(`AgentContext`... via `context_retrieval`, `TaskOrchestrator`,
`KnowledgeStore`, `ExecutionResult`) but is **entirely unreferenced**
outside its own file — not a Tauri command, not called by
`task_orchestrator`, not called by anything. It is presently pure library
code with no entry point.

### Naming collisions carried forward from the prior audit, reconfirmed

- **Three "router"s**: `ai::router` (Ollama-only cost/scoring heuristics),
  `ai::provider_router` (the real cross-provider dispatch layer),
  `intelligence::router` (worker capability matching for governance — *and*,
  per this audit's new finding, also home to `route_through_gateway`/
  `route_with_system`, which is `agent_v2`'s AI call path, via yet another
  independent HTTP client in `intelligence::gateway::OllamaGateway`). A
  future engineer told to "route through the AI router" must be told
  explicitly which one.
- **Four phase/state enums**: `agent::status` (string constants, DB-backed —
  `AWAITING_APPROVAL`, `COMPLETED`, etc.), `agent_controller::AgentPhase`,
  `task_orchestrator::TaskLifecycle`, `agent_v2::AgentState`. No shared
  vocabulary between them.
- **Two "planning" concepts**: `planning/` (DAG-based multi-task planning,
  frozen-adjacent, real, tested via `agent::executor`) vs. `planning_engine.rs`
  (single-task heuristic decomposition, feeds `task_orchestrator` only).

---

## 2. DATA FLOW DIAGRAMS

### 2a. The REAL, end-to-end-capable path (Stack A — `agent_v2`)

```
AgentPanel.tsx  (user types a task description, clicks Send)
   │
   │ invoke("start_agent_task", { description })
   ▼
agent_v2::start_agent_task  (#[tauri::command])
   │  spawns tauri::async_runtime::spawn(...)
   ▼
AgentRunner::process_task
   │
   ├─► task.transition_to(Planning)
   ├─► intelligence::router::route_through_gateway(prompt)
   │        └─► intelligence::gateway::OllamaGateway::execute_chat
   │                 └─► REAL reqwest call to Ollama, model hardcoded
   │                     "deepseek-coder:latest"
   │                     (NOT ai::provider_router — separate client)
   │
   ├─► task.transition_to(AwaitingApproval)
   │        └─► emits state event; registers a tokio::oneshot in
   │            ApprovalRegistry keyed by task.id
   │
   │   ◄── AgentPanel.tsx: invoke("approve_agent_task", {id}) or
   │       invoke("reject_agent_task", {id})  ── REAL BLOCKING GATE,
   │       process_task's .await on the oneshot receiver actually
   │       suspends here until the user acts
   │
   ├─► LOOP (up to MAX_RETRIES = 3):
   │     ├─► transition_to(ExecutingCoder)
   │     ├─► router::route_with_system(coder_system_prompt, ...)
   │     │        └─► same OllamaGateway path, model
   │     │            "deepseek-coder:latest"
   │     ├─► PayloadParser::parse_write_payloads(response)
   │     │        (parses <write_file path="..."> tags — brittle,
   │     │        string-based, no structured output/JSON schema)
   │     ├─► transition_to(ExecutingReviewer)
   │     ├─► router::route_with_system(reviewer_system_prompt, ...)
   │     │        (review output is logged, NOT parsed/acted upon —
   │     │        an "LGTM or list faults" response never blocks
   │     │        the write below)
   │     ├─► transition_to(Verifying)
   │     ├─► FileExecutor::safe_write(path, content)  for each parsed
   │     │        file — REAL fs::write, path-traversal-checked,
   │     │        backs up prior content in memory
   │     ├─► WorkspaceVerifier::verify_cargo_with_stderr
   │     │        — REAL `cargo check` subprocess against workspace
   │     │        root "." (hardcoded, not the user's actual open
   │     │        workspace path — see §5 risk)
   │     ├─► on success: transition_to(Completed), return
   │     └─► on cargo failure: roll back all writes via FileExecutor::
   │           rollback, increment retries, re-prompt coder with the
   │           compiler stderr, loop again; on exhausting retries,
   │           transition_to(Failed) and return
   ▼
Task lifecycle events emitted throughout; AgentPanel.tsx presumably
listens for them (not verified in this audit — out of the requested
file list for exhaustive read).
```

### 2b. The INERT path (Stack B — `task_orchestrator` / `AgentWorkbench`)

```
AgentWorkbench.tsx   ⚠ NEVER RENDERED — imported in app/page.tsx line 11,
   │                    but no JSX anywhere mounts <AgentWorkbench />.
   │                    Confirmed by grepping every .tsx file for the
   │                    tag; only the import line matches.
   │
   │  (if it WERE rendered, per lib/orchestrator.ts's evident design:)
   │ invoke("orchestrator_create_task", { goal })
   ▼
task_orchestrator::orchestrator_create_task
   │   TaskOrchestrator::create_task — pure struct construction,
   │   phase = Created, no analysis, no files touched
   ▼
   (user clicks "Approve" in the UI, if it existed)
   │ invoke("orchestrator_approve_task")
   ▼
task_orchestrator::orchestrator_approve_task
   │   TaskOrchestrator::transition(task, Executing{current_step:0,...})
   │   — THIS IS THE ENTIRE COMMAND. No plan was ever generated (plan()
   │   is never called from any command). No AgentContext.analyze was
   │   ever invoked. No file is read, no AI is called, no code executes.
   │   The task now silently sits in "Executing" state forever unless
   │   orchestrator_cancel_task or orchestrator_reset is called.
   ▼
   DEAD END — no command ever calls execute_step, observe, verify,
   recover, persist, or restore. These six real, tested, non-trivial
   functions exist purely for TaskOrchestrator's own unit tests.
```

**This is not a "partially wired" situation — it is a complete disconnect.**
`orchestrator_create_task`/`_approve_task`/`_reject_task`/`_cancel_task`/
`_get_state`/`_reset` are the entire IPC surface (6 commands, confirmed via
`lib.rs`), and none of them reach `planning_engine::plan_task`,
`ChangeGenerator::generate_patches`, `PatchApplier::apply`, or any AI/LLM
call.

---

## 3. ACTIVE EXECUTION PATHS (only what can run end-to-end today)

| Path | Entry point | Reaches real AI? | Reaches real file writes? | Reaches real verification? | Status |
|---|---|---|---|---|---|
| `agent_v2` autonomous coder | `AgentPanel.tsx` → `start_agent_task` | Yes (`OllamaGateway`) | Yes (`FileExecutor::safe_write`) | Yes (`cargo check`) | **ACTIVE, real, end-to-end** |
| `agent/` governed single-file pipeline | `agent::create_and_plan_task`/`create_and_plan_code_task` (registered commands; no frontend caller found in this audit's scope — `AgentPanel.tsx`/`AgentWorkbench.tsx` do not call these) | Yes (`agent::planner::plan_change`, real Ollama call via `ai::providers::ollama`) | Yes (`agent::executor::apply_and_verify`) | Yes (verification string stored per task; exercised for real in `governance`/`planning`/`release_validation` tests) | **Backend-active, tested, but no confirmed frontend entry point found in this audit** — worth a follow-up grep across the rest of `components/` beyond the files this audit was scoped to |
| `task_orchestrator` / `AgentWorkbench` | N/A — UI never renders | No | No | No | **Fully inert** |
| `multi_agent` Supervisor/Research/Coding/Testing/Review | None — zero callers anywhere | No | No | No | **Fully inert, unreachable library code** |
| `planning/` DAG multi-task planning | `planning::get_dag_runnable_tasks` + others registered in `lib.rs`; real logic exercised via `agent::executor` in its own extensive test suite | Indirectly (via `agent::executor`/`agent::planner` when tasks execute) | Yes, when driven | Yes, when driven | **Backend-active, tested**; frontend entry point not confirmed within this audit's file scope |

---

## 4. DUPLICATE / OVERLAPPING SYSTEMS

1. **Four independent "agent state machine" concepts** with no shared type
   or vocabulary: `agent::status` (string constants), `agent_controller::AgentPhase`,
   `task_orchestrator::TaskLifecycle`, `agent_v2::AgentState`.
2. **Two independent "planning" concepts**: `planning::planner` (DAG-aware,
   real, `agent/`-adjacent) vs. `planning_engine::PlanningEngine` (single-task,
   heuristic, `task_orchestrator`-only).
3. **Two independent "change execution" concepts**: `agent::executor::apply_and_verify`
   (real: sandboxed apply, verify, auto-rollback, frozen per `.clinerules`)
   vs. `change_executor::{ChangeGenerator, PatchApplier, DiffGenerator, PatchValidator}`
   (100% stub, see §5 — every method either returns an empty
   `Ok`/`Vec`/`false` or is otherwise a no-op, and has zero callers
   anywhere in the crate).
4. **Four independent Ollama HTTP clients**: `ai::providers::ollama`
   (the one `ai::provider_router` dispatches to), `agent::planner.rs`'s
   direct use of `ai::providers::ollama::ChatMessage` (same client,
   different call site — not itself a duplicate, but bypasses
   `provider_router`'s health/cooldown/model-resolution logic),
   `intelligence::gateway::OllamaGateway` (`agent_v2`'s client — genuinely
   separate `reqwest::Client`, separate request/response types, hardcoded
   model string), and (per the prior audit) `inline.rs`/`completion.rs`
   also calling `ai::providers::ollama` directly rather than through
   `provider_router`.
5. **Two agent-facing frontend panels** covering overlapping intent
   ("give the AI a task, watch it work, approve/reject"): `AgentPanel.tsx`
   (real, rendered) and `AgentWorkbench.tsx` (dead UI, never rendered,
   richer-looking phase visualization wired to nothing that does work).

---

## 5. MISSING CAPABILITIES / CRITICAL GAPS

- **`change_executor.rs` is a complete stub**, confirmed line-by-line this
  audit:
  ```rust
  ChangeGenerator::generate_patches  → Ok((vec![], vec![]))
  DiffGenerator::generate_diff       → empty UnifiedDiff
  PatchApplier::apply                → Ok(vec![])   // writes nothing
  PatchApplier::rollback             → Ok(false)
  PatchValidator::validate           → Ok(vec![])
  PatchValidator::detect_conflicts   → vec![]
  ```
  Zero tests, zero callers anywhere in the crate (confirmed via
  crate-wide grep for `change_executor::` and `ChangeGenerator`). This is
  the most unambiguous "known gap" in the entire subsystem — not a subtle
  partial implementation, a pure placeholder.
- **`task_orchestrator`'s real logic is unreachable from the running app.**
  Six substantial, tested functions (`execute_step`, `observe`, `recover`,
  `verify`, `persist`, `restore`) exist and pass their unit tests but have
  no path from any Tauri command to actually being called during real use.
- **`AgentWorkbench.tsx` is dead UI** — imported, never mounted. Whatever
  UX work went into its phase timeline/approval controls/terminal preview
  (per the commit `22e37cc` title) is currently invisible to any user.
- **`multi_agent.rs`'s Supervisor + 4-role system has no entry point at
  all** — not even a stub Tauri command. It cannot currently be exercised
  outside its own unit tests under any circumstance.
- **`agent_v2`'s workspace root is hardcoded to `"."`**
  (`FileExecutor::new(".")`, `WorkspaceVerifier::verify_cargo_with_stderr(Path::new("."))`
  inside `AgentRunner::process_task`), not derived from the actual open
  workspace (`AppState.workspace_root`, used correctly elsewhere in the
  codebase, e.g. `ai::get_context_for_query`). In practice this likely
  resolves to the Tauri process's current working directory, which may or
  may not coincide with the user's open workspace — a real correctness
  risk, not confirmed exploitable in this audit but worth flagging loudly.
- **`agent_v2`'s reviewer step is advisory-only** — `router::route_with_system`
  is called with the reviewer prompt, its response is printed to stdout via
  `println!`, and the result is completely discarded (`Ok(_) => println!(...)`).
  A reviewer that says "this code has a critical security flaw" cannot
  currently block the write that happens two lines later.
- **No AI Council code exists yet** — confirmed no files/directories named
  council, no references to "council" found in `src-tauri/src/` or
  `components/` during this audit's searches.

---

## 6. RISKS

- **Naming-collision risk (high, actionable today):** an agent (human or
  AI) asked to "wire up the agent execution path" has at least four
  plausible-sounding places to start, only one of which does anything.
  Recommend renaming or a prominent top-of-file "STATUS: DEAD CODE, DO NOT
  BUILD ON THIS" comment in `task_orchestrator.rs`, `agent_controller.rs`,
  `planning_engine.rs`, `change_executor.rs`, and `multi_agent.rs` as a
  cheap, non-code-changing mitigation, or a real reconciliation as a
  planned Level 3+ change.
- **Provider-routing regression risk:** the previous session's entire
  mission was consolidating AI calls behind `ai::provider_router` so that
  Ollama-vs-cloud selection, health/cooldown, and future multi-provider
  support work uniformly. `agent_v2` (the one REAL, actively-used
  autonomous-agent AI caller in the whole codebase) was never brought into
  that consolidation and uses a fourth, independent Ollama client with a
  hardcoded model string. Any future work that assumes "all AI calls go
  through `provider_router` now" is factually wrong for this specific,
  real, user-facing feature.
- **Silent no-op risk:** because `task_orchestrator`'s commands return
  `Ok(...)` at every step (create/approve/reject/cancel all succeed), a
  caller has no signal that "approve" did nothing. If `AgentWorkbench.tsx`
  were ever accidentally wired into the UI in a future change without
  also wiring `execute_step`/`observe`/`verify` into the commands, it
  would silently appear to work (state transitions animate, no errors)
  while doing nothing real — a much harder failure mode to notice than an
  outright crash.
- **Reviewer-bypass risk in the one real path:** `agent_v2`'s reviewer
  output being discarded means the "Testing/Review" half of the
  human-approval promise implied by the UI (approve → coder → reviewer →
  commit) doesn't actually gate anything after the human's own approval
  click. The human approves the *plan*, not the *code*, and the review
  step that runs after is cosmetic.
- **Hardcoded workspace root risk in `agent_v2`** — see §5. Real
  filesystem writes with a possibly-wrong root are a correctness and
  (mildly) safety concern, not just a code-quality one.
- **Fragility of `agent_v2`'s output parsing:** `PayloadParser::parse_write_payloads`
  is a hand-rolled string scanner for `<write_file path="...">` tags with
  no escaping/malformed-input handling beyond "stop at first unmatched
  tag." A model that deviates even slightly from the exact expected format
  (e.g., adds a code fence around the tags, as `WorkerPrompts::coder_system`
  explicitly asks it not to) produces zero parsed payloads and the task
  fails with "Coder failed to generate any valid structured tags" rather
  than a partial/degraded result.

---

## 7. RECOMMENDED AGENTCORE ARCHITECTURE

This is a proposal only — no code was changed to produce it. It is scoped
to resolve the fragmentation documented above while keeping AI Council's
stated future needs (§12 of the companion project audit: Council must be a
caller of `ai::provider_router`, never a parallel provider client) in
mind, and extends that same principle to agent execution: **one execution
core, one AI entry point, one state vocabulary.**

```
                    Frontend (ONE agent-facing panel,
                    not two competing ones)
                              │
                              │ invoke("agentcore_*")
                              ▼
                    AgentCore IPC Command Layer
                    (new, thin — supersedes agent_v2's
                    command fns AND task_orchestrator's
                    command fns; both currently exist,
                    only one should)
                              │
                              ▼
                    AgentCore State Machine
                    (ONE phase enum, replacing AgentPhase /
                    TaskLifecycle / AgentState / agent::status
                    — e.g. reuse task_orchestrator::TaskLifecycle's
                    shape since it's the most complete of the four,
                    or agent_v2::AgentState since it's the one
                    actually battle-tested end-to-end; pick one,
                    don't invent a fifth)
                              │
              ┌───────────────┼────────────────────┐
              ▼                                     ▼
      Planning Stage                         Execution Stage
      (keep agent_v2's REAL AI-driven               │
      planning call, retarget it through             │
      ai::provider_router instead of                 │
      intelligence::gateway::OllamaGateway —          │
      this is the highest-value single change:        │
      it makes the one real agent multi-provider-      │
      capable for free, and deletes a duplicate         │
      HTTP client)                                       │
              │                                            │
              ▼                                            ▼
      ai::provider_router::stream_cloud_chat      File Execution Stage
      / existing Ollama path                      (keep agent_v2's real
                                                    FileExecutor, but fix
                                                    the hardcoded "." root
                                                    to use the actual
                                                    workspace_root — this
                                                    is the second highest-
                                                    value single change)
                                                             │
                                                             ▼
                                                    Verification Stage
                                                    (keep agent_v2's real
                                                    cargo-check retry loop;
                                                    consider generalizing
                                                    beyond Rust/cargo if
                                                    Neural Forge is meant
                                                    to edit non-Rust
                                                    workspaces too — not
                                                    audited here whether
                                                    that's a stated goal)
                                                             │
                                                             ▼
                                                    Review Stage
                                                    (make agent_v2's
                                                    currently-discarded
                                                    reviewer response
                                                    actually gate the
                                                    commit — e.g. require
                                                    "LGTM" or a second
                                                    human approval on
                                                    flagged output)
```

**What this proposal deliberately does NOT do:** invent a fifth agent
concept from scratch, or try to salvage `task_orchestrator`/`change_executor`/
`multi_agent`/`agent_controller`/`planning_engine`'s code by wiring them
up as-is. Their state-machine/phase-tracking design (rich lifecycle enum,
recovery-attempt tracking, completion criteria, `KnowledgeStore` persistence
for resumability) is more sophisticated than `agent_v2`'s simpler loop and
may be worth keeping conceptually — but `agent_v2` is the one with real,
tested, working AI calls and real, tested, working file writes. The
pragmatic path is retrofitting `task_orchestrator`'s better lifecycle
bookkeeping onto `agent_v2`'s real execution guts, not the reverse, and
explicitly retiring (not silently abandoning) whichever pieces don't make
the cut so a future audit doesn't rediscover the same dead-code confusion.

**For AI Council specifically:** whatever AgentCore shape is chosen, Council
must call it the same way a single-agent task does — through
`ai::provider_router` for every model call, never through
`intelligence::gateway::OllamaGateway` or any other bespoke client. This
audit's single most concrete, actionable recommendation is: **before any
AI Council code is written, migrate `agent_v2`'s two `router::route_*`
call sites to go through `ai::provider_router` instead of
`intelligence::gateway::OllamaGateway`.** That one change collapses the
"four Ollama clients" problem to three, makes the one real agent
provider-agnostic, and directly unblocks Council from inheriting the same
duplication on day one.

---

## VALIDATION

- File created: `docs/architecture/NEURALFORGE_AGENT_ARCHITECTURE_AUDIT.md`
- No source files modified, moved, or deleted — verified via `git status --short`
  showing only the new, untracked `docs/architecture/` content before and
  after this audit (the prior project-wide audit file plus this one).
- All claims above are grounded in direct file reads and crate-wide greps
  performed during this session; where something could not be fully
  confirmed within the requested file scope (e.g., whether any frontend
  component calls `agent::create_and_plan_task`), it is explicitly marked
  "not confirmed within this audit's scope" rather than assumed either way.
