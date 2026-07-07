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
