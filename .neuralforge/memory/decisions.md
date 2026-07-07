# Decisions

- **Next.js 16 instead of 14** at scaffold time: Next 14 had multiple open
  Server-Components/Middleware/Image-Optimizer CVEs. Since the project was
  greenfield (three trivial files written), switched to latest stable before
  anything depended on the older API. Requires React 19.
- **dompurify pinned via npm `overrides`** to 3.4.11: `monaco-editor@0.55.1`
  transitively pins a vulnerable dompurify (XSS advisories). Overriding is
  safe since 3.4.11 is semver-compatible with what monaco expects.
- **Accepted risk**: a moderate advisory in Next's *internal* bundled postcss
  (CSS-stringify XSS) — irrelevant since we only transform our own trusted
  CSS at build time, and npm's suggested "fix" is actually a Next downgrade
  to 9.3.3, which is nonsensical.
- **Filesystem commands validate against workspace root**, not just trust the
  frontend: canonicalize + prefix-check on every path. New-path operations
  (create/rename-dest) validate the *parent* dir instead, since
  `fs::canonicalize` fails on paths that don't exist yet — validating after
  creation would be a TOCTOU gap (a first draft had this bug; caught and
  fixed before merging, see filesystem module tests).
- **Terminal PTY**: `portable-pty` (wezterm project), matches blueprint's "Rust
  PTY" requirement. A synthetic Rust-only test/debug harness initially looked
  like it hung indefinitely — root cause was that ConPTY sends a startup
  cursor-position query (`ESC[6n]`) that a real terminal client answers
  automatically (xterm.js does this out of the box); the bare test harness
  didn't, so cmd.exe's console host stalled waiting for a reply. Not a bug in
  the shipped feature. Fixed the test to answer the handshake like a real
  terminal would; kept as a regression test.
- **Logs**: `tracing` + `tracing-subscriber` + `tracing-appender` replacing
  the scaffolded `tauri-plugin-log` (console-only, dev-only) — one logging
  system instead of two overlapping ones. JSON file layer for the log
  viewer/export; stdout layer for the dev console. Fixed filename (no
  rotation) for Phase 1 — rotation can be layered on later without touching
  the read-back commands.
- **Bundle identifier** changed from `com.neuralforge.app` to
  `com.neuralforge.ide` — Tauri warned the `.app` suffix collides with macOS
  bundle conventions. Fixed immediately since bundle IDs are painful to
  change once anything (updater, installed-app registry) depends on them.
- **GPU detection via DXGI, not a cross-platform crate**: `wgpu` doesn't
  reliably expose total VRAM across backends; DXGI's
  `DXGI_ADAPTER_DESC1.DedicatedVideoMemory` does, and the project targets
  Windows first per the blueprint's Definition of Done. Utilization
  (real-time %) is deliberately not implemented - that needs vendor-specific
  SDKs (NVML for NVIDIA, etc.), disproportionate effort for Phase 2's
  foundation-level gate ("VRAM check rejects if insufficient"), which only
  needs static VRAM capacity, not live utilization.
- **VRAM check runs server-side in `chat_with_model`**, not just as an
  advisory frontend call: it re-fetches the model's real parameter/quant
  info via `list_models()` and refuses with `InsufficientResources` before
  ever hitting Ollama. A frontend-only check would be trivially bypassable
  and wouldn't actually satisfy "refuse to load if insufficient VRAM."
- **`ollama::chat_stream` is pure (no AppHandle)**; `ai::chat_with_model_core`
  wraps it with the model-lookup/VRAM-gate/health-recording logic, also
  without an AppHandle; the `#[tauri::command] chat_with_model` is a thin
  shell that just adds the `AppHandle::emit` call. Same pure-core/thin-wrapper
  pattern as filesystem/terminal. Both `#[ignore]`d-by-default regression
  tests (`chat_stream_produces_real_tokens_from_local_model`,
  `chat_with_model_core_logs_and_records_health`) run against a live local
  Ollama + `deepseek-coder:latest` without needing any Tauri runtime at all.
- **Rejected `tauri::test`'s `MockRuntime`** for testing `chat_with_model`:
  first attempt added it as a dev-dependency to construct a real `AppHandle`
  for testing, but it crashed the *entire* test binary at process launch
  (`STATUS_ENTRYPOINT_NOT_FOUND`) - confirmed Windows-specific to that
  feature by verifying the real (non-test) app binary still launched fine
  with identical code otherwise. Rather than debug Tauri's Windows linking
  internals, made `chat_with_model` generic-over-nothing again and pushed
  all its logic into a plain async function (`chat_with_model_core`) that
  takes `&HealthRegistry` and a token callback instead of `AppHandle` +
  `State`. Zero Tauri runtime dependency for the test, zero risk of hitting
  this class of bug again.
- **No credential storage in Phase 2**: `providers::has_api_key()` always
  returns `false`. Blueprint explicitly asked for an "authentication handler
  stub" in this phase and forbids storing secrets outside OS
  keychain/encrypted SQLite - neither exists yet, so building real key
  storage now would mean building it twice.
- **No vector embeddings in Phase 3**: checked for a locally available
  embedding model before starting - none pulled, and the running Ollama
  instance doesn't have `--embeddings` enabled. Restarting a service the user
  has running for their own purposes isn't something to do unilaterally just
  to build this feature. Built the storage path (`chunks.embedding BLOB`)
  but shipped FTS5 keyword search as the real, working search capability
  instead of a semantic search that would silently do nothing.
- **FTS5 query construction**: raw natural-language queries don't work with
  FTS5's default MATCH syntax (bare words are ANDed - a question needs every
  one of its words to literally appear in a chunk to match at all). Convert
  to an OR-of-terms query instead. Also enabled the porter stemmer
  (`tokenize = 'porter unicode61'`) after a test caught that "authentication"
  (query) and "authenticate" (code) don't match as exact tokens even with
  OR-of-terms - stemming both to the same root fixes it. Both were found by
  a failing test, not code review.
- **rusqlite `bundled-full`, not `bundled` + a separate `fts5` feature**:
  rusqlite has no feature literally named `fts5` (cargo error caught this
  immediately) - FTS5/FTS3/RTREE all come together via `bundled-full`.
- **model_benchmarks.db is a separate global DB, not part of the per-
  workspace index.db**: benchmarks are about the user's machine + installed
  Ollama models, not any particular project - reusing the workspace DB would
  mean re-benchmarking the same model for every workspace even though
  nothing about the model or hardware changed. Opened once at app startup in
  `app_data_dir` (confirmed via `cargo tauri dev` that this actually lands in
  the Roaming folder on Windows, not Local where logs go - these are
  genuinely different base directories in Tauri's path resolver, not the
  same dir with different subfolders).
- **Real TPS from Ollama's own stats, not word-count approximation**:
  `chat_stream` now returns `ChatStats` (eval_count/eval_duration/
  total_duration parsed from the final `done:true` chunk) instead of `()`.
  This changed an already-shipped, already-tested function's signature -
  worth it because approximating token count from whitespace-split words
  would have been meaningfully wrong (real tokenizers don't split on
  whitespace) for a number whose entire purpose is to drive routing
  decisions.
- **`chat_or_use_cache` takes owned `Option<String>`, not `&Connection`**:
  `#[tauri::command]` async fns must return `Send` futures; `rusqlite::
  Connection` is `Send` but not `Sync`, so a `&Connection` held across an
  `.await` inside the function would make it non-Send and fail to compile
  as a real command (this was worked out ahead of time by reasoning through
  the Send bound, not discovered via a failed build - unlike most other
  gotchas this session, which were genuinely caught by tests/compiler
  errors rather than anticipated).
- **Cloud provider pricing is real but currently inert**: `router::
  price_per_1k_tokens` has actual ballpark numbers per provider, but since
  `providers::has_api_key()` still always returns false (Phase 2 decision,
  unchanged), `select_model` only ever has Ollama candidates in practice.
  The pricing table exists now so the scoring/cost logic has real numbers to
  operate on rather than needing a rework when a second provider gets wired
  up later.
