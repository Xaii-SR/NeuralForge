# Current State

**Phase 1 (Foundation Shell): complete.**

All 9 build steps done, tested, and committed individually:
1. Repo scaffolding
2. Static Next.js frontend
3. Monaco editor + tabs
4. Tauri 2 desktop shell
5. Filesystem IPC + file explorer
6. PTY terminal emulator
7. Centralized event bus
8. Logging system + log viewer
9. Memory folder scaffolding

**Final gate verification:**
- `cargo test` (src-tauri): 10/10 passing — filesystem path-validation/traversal
  tests, terminal spawn/write/read integration test (real PTY, not mocked),
  memory-scaffold creation + non-overwrite tests.
- `cargo tauri dev`: launches, loads the Next.js UI (`GET / 200`), terminal
  spawns a real PTY session on mount (confirmed via log output).
- Log file confirmed on disk at `%LOCALAPPDATA%\com.neuralforge.ide\logs\app.log`
  with correct JSON structure (timestamp/level/target/fields).
- `cargo tauri build`: produces both
  `src-tauri/target/release/bundle/msi/neuralforge_0.1.0_x64_en-US.msi` and
  `.../bundle/nsis/neuralforge_0.1.0_x64-setup.exe`, no warnings.

**Not yet manually click-tested in the running GUI** (open folder → browse →
edit → save → terminal → logs, end to end as a human). Verified instead
through automated tests + direct disk/log inspection at each step, per
explicit instruction to avoid requiring manual confirmation between steps.
Worth a real click-through before calling Phase 1 fully done from a UX
standpoint, not just a correctness one.

**Phase 2 (Local AI Engine): complete.**

Built: hardware detection (cpu/memory/gpu via sysinfo + DXGI), Ollama client
(health/list/pull/remove/chat), provider registry (11 providers, auth stub -
no credential storage), provider health tracking (latency window + failure
cooldown), VRAM-gated model loading, streaming chat wired to a ChatPane UI.

**Verification:**
- `cargo test`: 16/16 passing, plus 2 `#[ignore]`d live tests run on demand
  against the real local Ollama instance and `deepseek-coder:latest`:
  - `chat_stream_produces_real_tokens_from_local_model` - the low-level HTTP
    streaming layer, genuine streamed content and a final `done:true`.
  - `chat_with_model_core_logs_and_records_health` - the *full* command path
    (model lookup, VRAM gate, health-cooldown check, health recording, and
    the exact `chat_completed` JSON log line LogViewer reads). Added after
    noticing the first test only covered the low-level stream, not the
    command a user's click actually triggers.
- `cargo tauri dev`: boots clean with the full AI module registered, no
  runtime panics.
- Ollama was already installed and running locally (v0.31.1, 4 models
  pulled) - no install/download was needed for gate testing.
- Note: `tauri::test`'s `MockRuntime` crashes this project's test binary at
  process launch (`STATUS_ENTRYPOINT_NOT_FOUND`) in this environment -
  confirmed unrelated to app correctness (the real app binary launches
  fine). Worked around by keeping Tauri-dependent commands as thin wrappers
  around plain, runtime-independent core functions instead of using
  MockRuntime at all. See decisions.md.

**Not yet manually click-tested in the running GUI** (same caveat as Phase 1 -
select model → type question → watch it stream in the actual window). The
underlying HTTP/streaming pipeline is verified for real; the last mile
(React state updates rendering correctly) is exercised by TypeScript's
compiler and the code's own logic, not a human eye.

**Phase 3 (Context Intelligence): complete** (vector indexing deferred - see below).

Built: SQLite index per workspace (`.neuralforge/index.db`, rusqlite bundled-full
for FTS5), a walkdir-based indexer that chunks text files into overlapping
line windows and skips unchanged files via content hash, FTS5 keyword search
with the porter stemmer, memory injection (reads the Phase-1
`.neuralforge/memory/*.md` files), and prompt management
(`build_context_prompt` combining memory + top search matches into a system
message). ChatPane now injects workspace context into every chat
automatically, plus an "Index Workspace" button.

**Vector indexing scope decision**: no embedding model is available locally
(this Ollama instance is running without `--embeddings` enabled, and no
dedicated embedding model like nomic-embed-text is pulled) - restarting a
service the user has running for other purposes wasn't something to do
unilaterally. Built the full schema/storage path for embeddings (chunks.embedding
BLOB column exists) but did not implement embedding generation or vector
similarity search. FTS5 keyword search is the real, working "workspace search"
capability for now; vector search can be layered on later without schema
changes.

**Verification** (all real, not mocked):
- `cargo test`: 24/24 passing, 2 `#[ignore]`d live-Ollama tests unaffected.
- Two real bugs caught by the tests themselves, not just inferred: (1) FTS5's
  default MATCH syntax ANDs every word, so natural-language queries almost
  never matched anything - fixed by converting queries to OR-of-terms; (2)
  even with OR-of-terms, "authentication" (query) didn't match "authenticate"
  (code) - different exact tokens - fixed by enabling FTS5's porter stemmer.
  Also hit and fixed a Windows-specific file-lock issue in three tests
  (couldn't `remove_dir_all` a temp dir while its SQLite connection was still
  open - scoped the connections to drop before cleanup).
- `cargo tauri dev`: boots clean with the full database module registered.

**Not yet manually click-tested in the running GUI** (same caveat as Phases 1-2).

**Phase 4 (AI Optimization Engine): complete.**

Built: Preferences (goal: speed/quality, cost: free/cheap/quality_first)
persisted in workspace settings; response cache (prompt+model hash ->
response) checked before every chat; per-model benchmarking (real TPS from
Ollama's own eval_count/eval_duration stats, not word-count approximation)
in a global `model_benchmarks.db`; pure/testable model scoring combining
benchmarks + preference; auto_select_model tying it together with a
human-readable "why" string; ChatPane auto-mode + SettingsPanel UI.

**Verification** (all real, not mocked):
- `cargo test`: 33/33 passing, plus 4 `#[ignore]`d live tests against real
  Ollama + `deepseek-coder:latest`, including the cache gate test itself:
  identical question sent twice, second call proven (not just assumed) to
  skip real generation - same content, >2x faster.
- Full app boots clean with the complete module registered; confirmed
  `model_benchmarks.db` actually gets created at startup in the Roaming app
  data dir (distinct from the Local dir logs use - a real Tauri path-resolver
  detail, not an assumption).

**Not yet manually click-tested in the running GUI** (same caveat as
Phases 1-3).

**Phase 5 (Agent Platform): complete** (single agent type - see below).

Built: SQLite-backed task queue matching the blueprint's JSON protocol
exactly ({id, objective, agent, files, status, verification, rollback}).
Simulation Mode (planner.rs) proposes a full replacement file via the LLM
and a risk estimate, with no filesystem write capability at all - a hard
boundary, not a flag. Only after explicit human approval does the executor
write the change, run real verification (`cargo check` for .rs files under
a Cargo project), and roll back to the original content on failure.
Finished tasks get appended to the Phase 1 `agent_history.md`. AgentPanel
UI (new "Agent" tab): objective+file inputs, task list, proposed-content
preview, Approve/Reject.

**Scope decision**: implemented one fully-working agent type (Coder) rather
than Coder+Tester+Security+Documentation in parallel - same pattern as every
prior phase's scoping (cloud providers in Phase 2, vector search in Phase 3).
A Supervisor that dispatches across multiple specialized agents is a
natural Phase 6+ extension of this same task-queue/approval architecture,
not a rework.

**Verification** (all real, not mocked):
- `cargo test`: 42/42 passing, plus 2 `#[ignore]`d live-Ollama tests. The
  standout: `broken_rust_change_is_automatically_rolled_back` sets up a real
  temp Cargo project, applies a genuine syntax error, and asserts both that
  a real `cargo check` caught it AND that the original file content was
  restored - the actual safety property, not a mocked assertion of it.
- Full app boots clean with the complete agent module registered.

**Not yet manually click-tested in the running GUI** (same caveat as
Phases 1-4) - the plan/approve/reject flow works per the command-layer
tests, but no human has clicked through it in the actual window yet.

**v1.0 Polish (UI refinement + documentation): complete.**

8px-grid spacing pass, hover/loading states, dark/light theme (persisted
`html.dark` class + localStorage, blocking init script in `layout.tsx` to
avoid a flash of the wrong theme), elegant empty states, ChatPane message
bubbles/timestamps, AgentPanel status/risk badges + rollback banner. Real
bug caught and fixed during this pass: `hover:bg-neutral-850` in
`TabBar.tsx` isn't a valid Tailwind class (no 850 step on the neutral
scale) - the hover state was silently a no-op. Six root-level docs added:
README, INSTALLATION, USAGE_GUIDE, ARCHITECTURE, TROUBLESHOOTING,
ROADMAP.

**Phase 6 (Advanced Platform - Extension System): complete.**

Built: `extensions/manifest.rs` (extension.json schema: name/version/
author/entry_point/runtime/permissions), `extensions/loader.rs` (scans
`~/.neuralforge/extensions/`, bundles two example extensions
non-destructively on first access), `extensions/api.rs` (spawns the
extension as an isolated child process via `tokio::process::Command`,
one line of JSON on stdin, parses the last stdout line as the result -
no direct host access of any kind). Two real bundled extensions:
python-repl (execs Python, captures stdout, reports real exceptions) and
file-search (fuzzy filename ranking). `ExtensionsPanel.tsx` (new
"Extensions" tab): list/enable/disable/uninstall, plus a direct
test-invoke UI. Agent integration: `agent_tasks` gained a `task_type`
column (`edit_file` default, `run_code` new), `planner::plan_code()`
generates Python for an objective, `executor::run_code_via_extension()`
invokes python-repl through the exact same process-isolation path
`run_extension` uses, and `approve_task` branches on `task_type` - a
`run_code` task goes through the identical plan -> awaiting_approval ->
approve -> execute flow as a file edit, not a lesser safety bar.

**Explicit non-claims** (see ROADMAP.md "Deliberately not built"): no
security sandbox in the WASM/OS-container sense - a plugin process could
still make its own network calls or spawn its own subprocesses outside
the mediated API, since there's no seccomp/AppContainer-level restriction
on the child process itself. No marketplace backend - "install" means
dropping a folder into `~/.neuralforge/extensions/`, not browsing a
registry.

**Verification** (all real, not mocked): `cargo test` (src-tauri):
57/57 passing (6 `#[ignore]`d live-Ollama tests), including 3 tests that
spawn a real `python.exe` subprocess through the full mediated protocol
and a full lifecycle gate test
(`agent::tests::gate_test_run_python_code_via_agent_task_lifecycle`)
that loads python-repl via the real loader, plans a fixed code task,
approves it, and asserts the real subprocess computed the right answer.
`cargo tauri dev` boots clean with the Extensions tab wired to live data.

**Phase 7 (Self Bootstrap): complete.**

Built: `bootstrap/selfanalyze.rs` (reads a workspace's own project memory
via the existing `ai::context::read_memory_context`, scans source files
skipping build/dependency noise dirs, read-only). `bootstrap/suggest.rs`
(asks the model to name exactly one file - validated against the real
scanned list, a hallucinated path is rejected outright - plus a title/
rationale; `slugify()` turns the title into a branch-safe string).
`bootstrap/diff.rs` (a small hand-rolled LCS line differ, no new
dependency, with a size-capped fallback for pathologically large files).
`bootstrap/git.rs` (real `git checkout -b neuralforge/suggest-<slug>`,
write + commit locally, then `cargo test --lib` or `npm test` if a
runner is actually detected for the changed file - an honest "not
checked" note otherwise, same discipline as the agent's `executor::
verify`). `bootstrap::propose_self_improvement` composes analyze ->
choose_target -> `agent::planner::plan_change` (reused directly, not
reimplemented) -> diff, entirely read-only. `bootstrap::
apply_self_improvement` is the only function that touches git/disk, and
is only reachable after a human approves the diff in `BootstrapPanel.tsx`
(new "Bootstrap" tab) - the same approve-before-write discipline as
Phase 5's agent, applied one level up.

**Hard boundary, not a missing feature**: nothing in Phase 7 pushes to a
remote, opens a pull request, or merges anything. `apply_self_improvement`
stops at a local commit on a local branch and a formatted PR-summary
string; pushing and opening a PR stay a human clicking their own git
tooling. There is no `git push` call anywhere in `bootstrap/`.

**Verification** (all real, not mocked): `cargo test` (src-tauri):
71/71 passing (7 `#[ignore]`d live-Ollama tests), including
`bootstrap::tests::gate_test_self_improvement_lifecycle_on_a_throwaway_repo`
- builds a real throwaway git repo + Cargo project (never the live
NeuralForge checkout), runs the real analyze/diff pipeline, creates a
real `neuralforge/suggest-*` branch, commits for real, and asserts a
real `cargo test` run passes - then asserts `git remote` is empty,
proving nothing was pushed. `cargo tauri dev` boots clean with the
Bootstrap tab and both `propose_self_improvement`/
`apply_self_improvement` commands registered.

**Not yet manually click-tested in the running GUI** (same caveat as
Phases 1-5, for Phase 6/7's UI specifically) - every backend path is
covered by real, non-mocked tests, and the app has been confirmed to
boot cleanly with all of it wired in, but no human has clicked through
Extensions/Bootstrap in the actual window yet.

**v1.0 status**: Phases 1-7 all built, tested, and committed
individually per the blueprint's own phase ordering. 71 backend tests
passing (7 intentionally `#[ignore]`d - they require a live local Ollama
instance and are run explicitly during development, not on every
`cargo test`). Known, honestly-scoped gaps carried forward from every
phase (see ROADMAP.md "Deliberately not built, and why"): no vector/
semantic search, no cloud AI providers wired up (no credential storage
exists), only one agent type (Coder), no true OS-level extension
sandboxing, no extension marketplace backend, Windows-only verification,
no automated frontend E2E suite. None of these are silently broken
features - each is a documented, deliberate scope boundary with a
concrete "next step" written down.
