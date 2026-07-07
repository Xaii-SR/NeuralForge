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

**Next**: Phase 5 (Agent Platform) — supervisor, task queue, JSON protocol,
permissions, simulation mode, snapshots, rollback. Not started. Per the
blueprint's own SAFETY section, this phase's deliverables *are* the approval
gates (plan mode before autonomous changes, snapshot+test+rollback) - not
something layered on after the fact.
