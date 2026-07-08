# Architecture

## Phase 1: Foundation Shell (complete)

**Frontend**: Next.js 16 (App Router, static export, React 19) + TypeScript + Tailwind.
No SSR, API routes, middleware, or server actions — `next.config.js` sets
`output: "export"` and `images.unoptimized: true`. Built to `out/`, loaded by
Tauri via `frontendDist`.

- `app/page.tsx` — top-level shell layout (toolbar, explorer, editor, bottom panel, status bar)
- `components/Editor.tsx`, `EditorPane.tsx`, `TabBar.tsx` — Monaco-based multi-tab editor
- `components/FileExplorer.tsx` — lazy-loaded file tree
- `components/Terminal.tsx` — xterm.js-backed PTY client
- `components/LogViewer.tsx` — polls and renders backend logs
- `hooks/useWorkspace.ts` — shared workspace/tab/file state
- `hooks/useEvent.ts` — typed wrapper around Tauri's event `listen()`
- `lib/fs.ts` — typed IPC wrappers over the filesystem commands

**Backend**: Tauri 2 / Rust. `main.rs` is initialization-only; `lib.rs` wires
plugins, managed state, and the command registry.

```
src-tauri/src/
  core/
    config.rs    - memory-folder scaffold constants + ensure_memory_scaffold()
    errors.rs    - AppError (Serialize'd to the frontend as a string), AppResult<T>
    events.rs    - typed event-name constants + emit_file_changed/emit_terminal_output
    logging.rs   - tracing/tracing-subscriber/tracing-appender setup, get_recent_logs, export_logs
    state.rs     - AppState { workspace_root: Mutex<Option<PathBuf>> }
  filesystem/    - read_dir/read_file/write_file/create_file/create_dir/delete_path/rename_path,
                   all path-validated against the open workspace root
  terminal/      - portable-pty backed PTY sessions (spawn_shell/write_to_pty/resize_pty/close_pty),
                   TerminalRegistry managed state, killed on ExitRequested
```

Not yet built (by design — Phase 1 boundary): `database/`, `hardware/`, `ai/`,
`agent/`. These are Phase 2+.

## Phase 2: Local AI Engine (complete)

```
src-tauri/src/
  hardware/
    cpu.rs, memory.rs  - sysinfo-based detection (cores, frequency, RAM)
    gpu.rs             - DXGI adapter enumeration (Windows-only path; vendor
                         from PCI vendor ID, dedicated VRAM). utilization_percent
                         is always None for now - no NVML/ADLX integration yet.
    mod.rs             - HardwareInfo aggregate + get_hardware_info command
  ai/
    providers/
      mod.rs   - ProviderId enum (Ollama + 10 cloud providers), ProviderMetadata,
                 has_api_key() stub (always false - no credential storage)
      ollama.rs - real HTTP client (reqwest) against localhost:11434.
                 chat_stream() is the pure/testable core (NDJSON parsing,
                 generic on_token callback) - no AppHandle dependency at all.
    health.rs   - HealthRegistry: rolling latency window (20 samples) +
                 failure-count cooldown (3 failures -> 30s) per provider
    model_manager.rs - VRAM estimate from parameter_size + quantization_level,
                 compared against detected GPU VRAM (falls back to system RAM
                 if no dedicated GPU)
    context.rs  - Phase 3: memory injection + prompt management, see below
    cache.rs    - Phase 4: response cache, see below
    benchmarks.rs - Phase 4: model benchmarking, see below
    router.rs   - Phase 4: pricing/scoring/preferences, see below
    mod.rs      - command layer: chat_with_model_core (plain async fn, takes
                 &HealthRegistry + a token callback, no Tauri types at all)
                 composes list_models -> VRAM gate -> health cooldown check
                 -> ollama::chat_stream, recording health either way.
                 chat_or_use_cache wraps that with an owned Option<String>
                 cache value (see Phase 4 note on why it's owned, not a
                 &Connection). The #[tauri::command] chat_with_model is a
                 thin shell: read cache (before any await) -> chat_or_use_cache
                 -> write cache (after the await).
```

Frontend additions: `lib/ai.ts` (typed IPC wrappers), `components/ChatPane.tsx`
(model dropdown from real installed models, streams tokens into the active
assistant message keyed by `request_id`, graceful "Ollama not detected"
state). Mounted as a right-side panel in `app/page.tsx`.

## Phase 3: Context Intelligence (complete)

```
src-tauri/src/
  database/
    mod.rs     - per-workspace SQLite at .neuralforge/index.db (rusqlite,
                 bundled-full for FTS5). files/chunks tables + chunks_fts
                 (FTS5, porter+unicode61 tokenizer) kept in sync via triggers.
                 DbState { conn: Mutex<Option<Connection>> } managed state,
                 (re)opened in filesystem::open_workspace.
    indexer.rs - walkdir over the workspace (same excluded dirs as the repo's
                 own .gitignore, plus .neuralforge itself), skips binaries/
                 >1MB files, chunks into 40-line windows with 5-line overlap,
                 skips re-indexing unchanged files via a content hash.
    search.rs  - FTS5 keyword search. Converts the raw query into an
                 OR-of-terms FTS5 query (FTS5's default MATCH ANDs every bare
                 word, which is useless for natural-language questions).
  ai/
    context.rs - read_memory_context() reads .neuralforge/memory/*.md (Phase
                 1), skipping empty/header-only files. build_context_prompt()
                 combines that with the top FTS5 matches for the query into a
                 single context block, sent as a system message ahead of the
                 user's actual question.
```

Not implemented: vector embeddings/semantic search (schema ready, no
embedding model available - see decisions.md).

## Phase 4: AI Optimization Engine (complete)

```
src-tauri/src/
  ai/
    cache.rs      - response_cache table (in the per-workspace index.db):
                    prompt+model hash -> response. get_cached/store_response/
                    clear_cache are plain sync functions over &Connection.
    benchmarks.rs - separate global model_benchmarks.db (app data dir,
                    Roaming on Windows - distinct from the Local dir logs
                    use), opened once in lib.rs's .setup(). run_benchmark()
                    runs a real short prompt and reads TPS from Ollama's own
                    eval_count/eval_duration (ChatStats, added to
                    ollama::chat_stream's return value for this).
    router.rs     - Preferences (goal/cost_preference) stored in the
                    workspace settings table. price_per_1k_tokens (static
                    table, Ollama=$0) + estimate_cost. score_models is pure/
                    testable: speed goal ranks by benchmarked TPS (falls back
                    to smaller param count when unbenchmarked), quality goal
                    ranks by larger param count. select_model layers in
                    HealthRegistry status and cost into the final reason.
```

Command-layer detail worth remembering: `chat_or_use_cache` (in `ai/mod.rs`)
takes an *owned* `Option<String>` for the cached value, not a `&Connection`.
`#[tauri::command]` async fns must return `Send` futures, and
`rusqlite::Connection` is `Send` but not `Sync` - a borrowed `&Connection`
held across an `.await` makes the future non-Send. The fix is structural:
DB reads happen before the await, DB writes happen after, and the async
core function itself never touches the connection type at all.

Frontend: `ChatPane` has an "Auto" toggle (default on) that calls
`auto_select_model` before every send and shows the selection + reason +
cost; cached responses get a "from cache" tag. `SettingsPanel` (new) exposes
goal/cost preferences, per-model benchmark runs with live results, and a
cache-clear button.

## Phase 5: Agent Platform (complete - single agent type)

```
src-tauri/src/
  agent/
    mod.rs      - AgentTask struct matches the blueprint's JSON protocol
                  exactly. agent_tasks table in the workspace index.db.
                  Commands: create_and_plan_task, approve_task, reject_task,
                  list_agent_tasks. Tasks are inserted in "planning" status
                  *before* the LLM call so a real record exists even if
                  planning fails.
    planner.rs  - Simulation Mode: plan_change() proposes a full replacement
                  file via the LLM and estimate_risk() scores it (added/
                  removed line ratio). No filesystem write capability at
                  all - not a permission check, an absent capability.
    executor.rs - only called after approve_task (explicit human approval).
                  apply_and_verify(): write proposed content -> verify
                  (real `cargo check` for .rs files under a Cargo project;
                  other types are written but flagged unverified, not
                  falsely marked as checked) -> on failure, restore the
                  original content. Same canonicalized-path-prefix
                  discipline as filesystem::validate_within_workspace.
    memory.rs   - appends a one-line entry to the Phase 1
                  .neuralforge/memory/agent_history.md on every finished
                  task.
```

Only one agent type (Coder) is implemented - Tester/Security/Documentation
from the blueprint's agent list are not built. A Supervisor dispatching
across multiple agent types is a natural extension of this same task-queue/
approval architecture later, not a rework.

Frontend: `AgentPanel.tsx`, a new "Agent" tab in the bottom panel - objective
+ file path inputs, task list with live status, proposed-content preview,
Approve/Reject (only shown in `awaiting_approval`).

## Phase 6: Advanced Platform - Extension System (complete)

```
src-tauri/src/
  extensions/
    manifest.rs - extension.json schema (name/version/author/entry_point/
                  runtime/permissions), interpreter_for() maps runtime -> interpreter binary
    loader.rs   - home_dir()/extensions_dir() (~/.neuralforge/extensions),
                  ensure_bundled_extensions() writes the two example
                  extensions if missing (never overwrites), scan() reads
                  every extension.json, enabled_state.json for on/off
    api.rs      - invoke_extension(): spawns the interpreter as a child
                  process, writes one line of JSON to stdin, closes it,
                  parses the last stdout line as the result. No direct
                  host access - a plugin can only get what the host put
                  in the request.
    mod.rs      - command layer: list_extensions, set_extension_enabled,
                  uninstall_extension, run_extension
```

Two bundled example extensions (Python, written into `~/.neuralforge/
extensions/` on first access): `python-repl` (execs code, captures
stdout via `contextlib.redirect_stdout`, reports real exceptions as
`{"success": false, "error": ...}`) and `file-search` (substring-based
fuzzy filename ranking over a file list the host provides).

Agent integration: `agent_tasks` gained a `task_type` column
(`edit_file` | `run_code`, additive `ALTER TABLE` migration for
pre-existing DBs). `agent::planner::plan_code()` generates Python for an
objective; `agent::executor::run_code_via_extension()` calls
`extensions::ensure_and_scan()` then `invoke_extension()` against
python-repl - the exact same path `run_extension` uses, so a run_code
task gets no more host access than a manually-invoked extension.
`approve_task` branches on `task_type`: `run_code` skips the
workspace-write/rollback path entirely (nothing on disk to roll back)
and goes straight to the extension call, but is gated behind the same
`awaiting_approval` -> explicit-approve transition as a file edit.

Frontend: `ExtensionsPanel.tsx` (new "Extensions" tab) - list installed
extensions, enable/disable, uninstall, and a direct test-invoke panel.
`AgentPanel.tsx` gained an Edit File / Run Code mode toggle.

## Phase 7: Self Bootstrap (complete)

```
src-tauri/src/
  bootstrap/
    selfanalyze.rs - analyze(): read_memory_context() (reused from
                     ai::context, Phase 3) + a walkdir scan for .rs/.ts/
                     .tsx files, skipping target/node_modules/.git/.next/
                     dist/.neuralforge, capped at 150 files. Read-only.
    suggest.rs     - choose_target(): one Ollama call asking for exactly
                     three lines (FILE/TITLE/WHY). The named file is
                     validated against the real scanned list - a
                     hallucinated path is a hard error, not a best-effort
                     guess. slugify() turns the title into a branch-safe
                     string (used directly in a git branch name).
    diff.rs        - unified_diff(): hand-rolled O(n*m) LCS line diff (no
                     new crate dependency), falls back to a summary line
                     above a size cap instead of a huge DP table.
    git.rs         - create_branch() (real `git checkout -b
                     neuralforge/suggest-<slug>`, errors if not a git
                     repo), write_and_commit() (real local commit, no
                     push anywhere in this file), run_tests() (`cargo
                     test --lib` if the file is under a Cargo project,
                     `npm test` if package.json declares a test script,
                     an honest "not checked" note otherwise).
    mod.rs         - propose_self_improvement (read-only: analyze ->
                     choose_target -> agent::planner::plan_change reused
                     directly -> diff::unified_diff) and
                     apply_self_improvement (git::create_branch ->
                     git::write_and_commit -> git::run_tests ->
                     format_pr_summary). apply_self_improvement is only
                     ever called after a human approves the diff
                     propose_self_improvement returned.
```

Frontend: `BootstrapPanel.tsx` (new "Bootstrap" tab) - Analyze & Propose
shows the diff for review; Approve calls apply_self_improvement and
displays the branch name, real test output, and formatted PR summary;
Reject discards the proposal with zero side effects (no git operation
of any kind happens before approval).

**Hard boundary**: no function anywhere in `bootstrap/` calls `git
push`, opens a pull request, or merges a branch. `apply_self_improvement`
stops at a local commit and a summary string - pushing and opening a PR
are left to the human, on purpose.

## Key invariants
- All filesystem commands validate the target path is canonicalized-within the
  open workspace root before touching disk (rejects traversal, symlink escape
  attempts resolve via `fs::canonicalize`). See `filesystem::validate_within_workspace`
  and `validate_new_path_in_workspace` (the latter handles not-yet-existing
  paths by validating the parent instead).
- Logs go to the OS-standard app-log directory (`app.path().app_log_dir()`),
  not a workspace-relative folder — this is a desktop app's own operational
  log, not project data.
- The bundle identifier is `com.neuralforge.ide` (not `com.neuralforge.app` —
  that suffix collides with the macOS `.app` bundle extension).
