# NeuralForge v1.3.0

A local-first, offline-capable, AI-native desktop IDE. Tauri 2 (Rust) backend, Next.js 16 frontend, powered by local (Ollama) and configurable cloud AI providers.

This release adds persistent, multi-session AI chat and automatic workspace indexing on top of the v1.2.0 foundation. All v1.2.0 functionality (providers, AI Council, Prompt Maker, build identity display, editor/file/terminal shell) is preserved unchanged.

## What's new in v1.3.0

**Persistent Chat Sessions**
AI chat conversations are now saved to a per-workspace SQLite database instead of living only in memory. Closing and reopening NeuralForge restores your conversation history automatically — no manual save/export step required.

**SessionTabs**
A tab strip above the chat panel lets you manage multiple parallel conversations per workspace:
- Create a new session at any time
- Switch between sessions instantly (each keeps its own isolated message history)
- Rename a session by double-clicking its tab
- Delete a session; NeuralForge automatically selects a replacement session (or shows a clean empty state if none remain)

**Automatic Workspace Indexing**
Opening a folder now indexes it automatically — repository-aware AI chat context is available immediately, with no manual "Index Workspace" click required. The existing manual "Index Workspace" button remains available as an explicit rebuild/recovery option; both paths use the same underlying indexer, so there is exactly one indexing implementation to reason about. Re-opening an already-indexed workspace is fast — unchanged files are skipped via the existing content-hash/mtime incremental logic.

**Workspace-aware AI Chat & Session Persistence**
Session storage is scoped per workspace at the database level (each workspace has its own SQLite file) — conversations from one project never leak into another. Streaming responses, model auto-selection, and cancellation behavior are unchanged from v1.2.0; persistence hooks into the existing send/stream-complete lifecycle without altering it.

## Known limitation

**First-time indexing of very large workspaces can temporarily freeze the UI.** Automatic indexing currently runs synchronously on workspace open. For a typical-sized project this completes in well under a second on repeat opens (thanks to incremental skipping), but the *first* index of a large, never-before-indexed workspace can take tens of seconds, during which the window may appear unresponsive. The workspace still opens successfully once indexing completes, and no data is lost — this is a responsiveness issue, not a correctness one. Making this indexing pass asynchronous is tracked as a future improvement.

## Compatibility

- Workspaces opened for the first time with v1.3.0 initialize cleanly with no manual setup.
- Workspaces previously indexed under v1.2.0 continue to work — the session tables are created automatically on first open, with no manual database migration step.
- All v1.2.0 features (provider management, AI Council, Prompt Maker, build identity display) are unchanged.

## Installation

See [INSTALLATION.md](INSTALLATION.md) for full setup. Windows x64 build artifacts are provided with this release (NSIS `.exe` and `.msi`).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
