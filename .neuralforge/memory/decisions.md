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
