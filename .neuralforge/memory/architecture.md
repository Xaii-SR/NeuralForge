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
    mod.rs      - command layer: chat_with_model_core (plain async fn, takes
                 &HealthRegistry + a token callback, no Tauri types at all)
                 composes list_models -> VRAM gate -> health cooldown check
                 -> ollama::chat_stream, recording health either way. The
                 #[tauri::command] chat_with_model is a thin shell that just
                 adds the AppHandle::emit call.
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

Frontend: ChatPane fetches context via `get_context_for_query` before every
send (best-effort - silently skipped if no workspace/index is open) and an
"Index Workspace" button that runs `index_workspace` and shows file/chunk counts.

**Not implemented**: vector embeddings / semantic search. The schema has a
`chunks.embedding BLOB` column ready for it, but no embedding model is
available in this environment (see decisions.md) - FTS5 keyword search is the
real "workspace search" capability for now.

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
