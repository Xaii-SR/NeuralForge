# NeuralForge v1.0.0

A local-first, offline-capable, AI-native desktop IDE. Tauri 2 (Rust) backend, Next.js 16 static-export frontend, powered entirely by a local [Ollama](https://ollama.com) instance — no cloud dependency, no telemetry, no account.

This is the first complete release: all seven phases of the original build plan (`blueprint.md`), built and tested against real infrastructure (real SQLite, real PTY, real Ollama calls, real `cargo check`/`cargo test`, real git operations) rather than mocks.

## Installation

See [INSTALLATION.md](INSTALLATION.md) for full setup (Ollama requirement, model pulls, platform notes). Quick version:

1. Install and start [Ollama](https://ollama.com), pull a coding model (`ollama pull deepseek-coder`).
2. Windows: run `neuralforge_1.0.0_x64-setup.exe` (NSIS) or `neuralforge_1.0.0_x64_en-US.msi` (MSI) from this release.
3. Launch NeuralForge, click **Open Folder**, pick a project.

**Platform note**: build artifacts in this release are Windows x64 only — this project has been built and tested on Windows throughout (see "Known limitations" below). The codebase has no Windows-only code outside `hardware/gpu.rs` (DXGI), so a macOS/Linux build should work via `cargo tauri build` on those platforms, but that hasn't been done or verified yet.

## Feature checklist

**Phase 1 — Foundation Shell**
- [x] Tauri 2 desktop shell (Next.js 16 static export frontend)
- [x] Monaco-based multi-tab editor
- [x] Sandboxed file explorer (all paths validated against the open workspace root)
- [x] Real PTY terminal (portable-pty + xterm.js)
- [x] Structured JSON logging + in-app log viewer/export
- [x] Per-workspace memory scaffold (`.neuralforge/memory/*.md`)

**Phase 2 — Local AI Engine**
- [x] Hardware detection (CPU/RAM via sysinfo, GPU/VRAM via DXGI on Windows)
- [x] Ollama HTTP client: health check, list/pull/remove models, streaming chat
- [x] VRAM-gated model loading (refuses to load a model the detected hardware can't fit)
- [x] Provider health tracking (rolling latency window + failure cooldown)
- [x] Streaming chat UI (ChatPane)

**Phase 3 — Context Intelligence**
- [x] Per-workspace SQLite index (FTS5 full-text search, porter stemmer)
- [x] Content-hash-based incremental indexing (skips unchanged files)
- [x] Memory injection + prompt management (project memory + top search matches into every chat)
- [ ] Vector/semantic search — schema ready (`chunks.embedding BLOB`), not implemented (no embedding model available in the build environment)

**Phase 4 — AI Optimization Engine**
- [x] Response cache (prompt+model hash → response)
- [x] Real per-model benchmarking (genuine TPS from Ollama's own `eval_count`/`eval_duration`, not a word-count approximation)
- [x] Cost router + preference-driven auto model selection (speed/quality goals, cost preference)
- [x] Settings UI: preferences, live benchmark runs, cache control

**Phase 5 — Agent Platform**
- [x] Safety-gated Coder agent: plan → human approval → apply → verify → automatic rollback on failure
- [x] Real verification (`cargo check` for `.rs` files under a Cargo project; honest "not checked" note for everything else — never a fabricated pass)
- [x] Proven rollback: a real syntax error in a real temp Cargo project is caught and the original file restored
- [x] Task queue persisted per-workspace, matching the blueprint's JSON protocol
- [ ] Additional agent types (Tester/Security/Documentation) — only Coder is implemented

**Phase 6 — Advanced Platform (Extension System)**
- [x] Extension manifest schema (`extension.json`)
- [x] Plugin loader (scans `~/.neuralforge/extensions/`)
- [x] Real process isolation: each extension runs as a separate child process, mediated JSON protocol over stdin/stdout — no direct host access
- [x] Two working example extensions: `python-repl` (executes Python, captures real output/exceptions), `file-search` (fuzzy filename search)
- [x] Extension manager UI (list, enable/disable, uninstall, direct test-invoke)
- [x] Agent integration: `run_code` tasks execute LLM-generated Python through the identical plan → approve → execute flow used for file edits
- [ ] OS-level sandboxing (seccomp/AppContainer) — not implemented; see "Known limitations"
- [ ] Extension marketplace backend — "install" is a local folder, not a registry

**Phase 7 — Self Bootstrap**
- [x] Self-analysis: reads a workspace's own project memory + scans its source files
- [x] Suggestion engine: asks the local model to propose one focused improvement (validated against the real file list — a hallucinated file path is rejected)
- [x] Diff generation (hand-rolled LCS differ)
- [x] Local branch creation (`neuralforge/suggest-<slug>`) + real commit
- [x] Real test run (`cargo test` / `npm test`, or an honest "not checked" note)
- [x] Human-readable PR summary (why, risk, test results, diff)
- [x] **Human approval required** before any git operation happens at all
- [x] **Never pushes, never opens a PR, never merges** — verified by a gate test that asserts `git remote` stays empty throughout

**v1.0 Polish**
- [x] Dark/light theme (persisted, no flash-of-wrong-theme)
- [x] 8px-grid spacing, hover/loading states, elegant empty states
- [x] Full documentation suite: [README](README.md), [INSTALLATION](INSTALLATION.md), [USAGE_GUIDE](USAGE_GUIDE.md), [ARCHITECTURE](ARCHITECTURE.md), [TROUBLESHOOTING](TROUBLESHOOTING.md), [ROADMAP](ROADMAP.md)

## Known limitations

Carried forward honestly from [ROADMAP.md](ROADMAP.md#deliberately-not-built-and-why) — nothing below is a silently broken feature, each is a deliberate, documented scope boundary:

- **No vector/semantic search.** FTS5 keyword search (with stemming) is the real, working capability today.
- **No cloud AI providers.** Only Ollama has a working client; no credential storage exists yet (by design — no OS keychain/encrypted-SQLite integration built).
- **Only one agent type** (Coder). Tester/Security/Documentation agents from the original design aren't built.
- **No autonomous GitHub operations**, anywhere in the codebase — Phase 7 creates local branches and runs tests, but pushing and opening PRs are always a human action.
- **Extension isolation is process-level, not a true OS security sandbox.** A plugin process could still make its own network calls or spawn subprocesses; there's no seccomp/AppContainer restriction on it.
- **No extension marketplace service** — a local manager for bundled/manually-installed extensions, not a browsable registry.
- **Windows-only verification.** Built and tested on Windows; other platforms should work (no other OS-specific code) but haven't been run end-to-end.
- **No automated frontend E2E test suite.** Every backend capability has real, non-mocked automated tests; frontend verification relied on TypeScript's compiler, manual review, and live app boots.

## Screenshots

Not attached to this release — capturing the native Tauri window wasn't available in the build environment this release was prepared in (only a browser-based preview of the frontend, which can't exercise the real desktop IPC bridge). The UI was visually verified via that browser preview (both light and dark theme render correctly) and via multiple full `cargo tauri dev` boots in the real desktop window. Screenshots of the actual app are a good first addition for whoever cuts the next release.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for what's next — resource governor, multi-file agent tasks, a real quality benchmark, an auto-updater, and the "next step" written down for every item in "Known limitations" above.

## Changelog

Full history: `git log`. Highlights by phase:

- **Foundation Shell** (`0866b00` and earlier) — repo scaffold through Tauri shell, editor, filesystem, terminal, logging, memory scaffold.
- **Local AI Engine** (`2224d57`..`f5560e5`) — hardware detection, Ollama integration, VRAM gate, streaming chat.
- **Context Intelligence** (`8a207f4`..`f2a62fa`) — SQLite/FTS5 index, memory injection, context-aware chat.
- **AI Optimization Engine** (`f85bfb1`) — cache, benchmarking, cost router, auto mode.
- **Agent Platform** (`62342ba`) — task queue, planner, executor, rollback, AgentPanel.
- **v1.0 Polish** (`1839d22`) — UI refinement pass + full documentation suite.
- **Advanced Platform / Extensions** (`aed39de`) — manifest, loader, mediated API, example extensions, extension manager UI, `run_code` agent integration.
- **Self Bootstrap** (`1fbe74c`, `3aae1b7`, `f50815c`, `58ec5bf`) — self-analysis, suggestion engine, diff generation, branch/test/PR formatting, Bootstrap UI.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
