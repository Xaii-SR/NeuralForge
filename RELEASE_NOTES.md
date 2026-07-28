# NeuralForge v1.4.4

A local-first, offline-capable, AI-native desktop IDE. Tauri 2 (Rust) backend, Next.js 16 frontend, powered by local (Ollama) and configurable cloud AI providers.

v1.4.4 is a **maintenance and stabilization release**. It contains security, data-integrity, lifecycle, and correctness repairs verified against the repository. It does **not** contain the planned provider-adapter architecture rewrite, the searchable provider/model picker, or the resizable Chat/Terminal redesign — those remain deferred future work.

## What's fixed in v1.4.4

**Ghost Text (inline AI completion) now works**
- Repaired a broken IPC contract: the frontend previously sent `{prefix, suffix}` while the Rust command required `{content, cursorLine, cursorColumn}`, so every ghost-text request failed argument deserialization and no suggestion ever rendered in the shipped build. The editor now threads real Monaco document content and cursor position through to the backend.
- Fixed an unmount guard that was shadowed by a local variable, so streamed events can no longer update an unmounted editor.
- Added real backend request cancellation: a superseding request cancels the previous one through the shared request registry, and unmount cancels any in-flight completion. Exactly one terminal outcome occurs per request and no registry entry leaks.

**Transactional indexing (data integrity)**
- Per-file index writes (metadata, chunks, symbols, dependencies) are now wrapped in a single database transaction with error propagation instead of independent, error-discarding writes. A failure partway through a file now rolls back completely instead of committing a fresh content hash over stale chunks — which previously created silent, sticky index corruption.
- Regression tests sabotage a table mid-write to force a real failure through the production path and assert zero partial rows survive and that a retry fully recovers the file.

**Filesystem watcher lifecycle**
- Wired a real OS filesystem watcher (`notify`) to the indexer. It watches the active workspace recursively, debounces events, and dispatches changed paths through the same hardened transactional reindex path. At most one watcher is ever active, scoped to one workspace generation; replacing or closing a workspace stops the OS watch and joins its thread within one poll interval (no indefinite hang).

**Database migration hardening**
- Additive-column migrations now inspect `PRAGMA table_info` and only issue an `ALTER` when a column is genuinely missing, so a real DDL failure (disk/permissions/corruption) propagates instead of being silently discarded alongside the expected "duplicate column" case.
- Added deterministic tests, including a genuinely pre-migration schema upgrade and two two-connection contention tests proving transient contention is absorbed by the busy-timeout retry and that contention outlasting the timeout returns a bounded, actionable error (not a hang, not a false success).

**Session and agent correctness**
- Session message persistence and metadata updates are atomic.
- The governed agent checks file state before writing to prevent stale-file overwrite.

**Tooling and dependencies**
- `npm run lint` is now a real ESLint gate instead of a no-op.
- Resolved the `dompurify` sanitizer-bypass advisory (3.4.11 → 3.4.12) and an eslint-chain DoS advisory via a non-breaking `npm audit fix`.

## Known limitations

- `npm run lint` surfaces 37 pre-existing React-hooks-rule violations across unrelated hooks. These are static-analysis findings on existing patterns, not runtime defects; they are tracked for a dedicated follow-up pass and do not affect the shipped build.
- 12 `npm audit` high-severity advisories remain in Next.js's server runtime (`postcss`/`sharp` chain). This app ships as a static export (`output: "export"`, `images.unoptimized: true`) with no Next.js server process in the Tauri bundle, so that attack surface is not present in what ships. No non-breaking upstream fix is available.
- Frontend consumption of the new `workspace-file-index-updated` event (explorer refresh / dirty-buffer conflict UI) is not yet implemented — the backend emits the event; the UI listener is deferred.

## Validation

- Rust: `cargo test` — 441 passed, 0 failed, 19 ignored.
- Rust: `cargo clippy --all-targets --all-features` — no errors.
- Frontend: `npx tsc --noEmit` — clean.
- Frontend: `npm run build` — static export succeeds.
- Windows installers (MSI + NSIS) built and packaged.

## Installation

See [INSTALLATION.md](INSTALLATION.md) for full setup. Windows x64 installers (MSI and NSIS) are attached to this release.
