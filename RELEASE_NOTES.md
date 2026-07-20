# NeuralForge v1.4.0

A local-first, offline-capable, AI-native desktop IDE. Tauri 2 (Rust) backend, Next.js 16 frontend, powered by local (Ollama) and configurable cloud AI providers.

This release adds automatic workspace restoration on top of the v1.3.0 foundation. All v1.3.0 functionality (persistent chat sessions, SessionTabs, automatic background workspace indexing, providers, AI Council, Prompt Maker, build identity display, editor/file/terminal shell) is preserved unchanged.

## What's new in v1.4.0

**Workspace Restoration**
NeuralForge now remembers the last workspace you had open and reopens it automatically on launch — no manual "Open Folder" step required to pick up where you left off. Restoration goes through the exact same open flow as a manual open, so background indexing runs and your saved chat sessions (SessionTabs) reappear with the workspace, exactly as you left them.

Restoration is fully graceful:
- First launch (nothing to restore) starts at the normal "No folder open" state.
- If the remembered folder was moved or deleted, NeuralForge simply starts fresh — never an error screen.
- Restoration can never block or break startup; any failure silently falls back to a normal fresh launch.

## Carried forward from v1.3.0

- **Persistent Chat Sessions** — conversations are saved per workspace in SQLite and restored across restarts.
- **SessionTabs** — create, switch, rename, and delete parallel conversations per workspace.
- **Automatic Workspace Indexing** — opening a folder indexes it on a background thread; the UI stays responsive even for very large workspaces, and repeat opens skip unchanged files.
- **Workspace-aware AI Chat** — repository context is retrieved automatically for your questions.
- **AI Council** — the sequential Architect → Critic → Judge multi-agent reasoning pass, available from the red Council toolbar button.
- **Prompt Maker** — guided prompt generation from the toolbar.
- **Provider support** — local Ollama plus configurable cloud providers (Anthropic, Gemini, OpenAI-compatible endpoints) with secure OS-keychain credential storage and per-provider connection testing.

## Compatibility

- Workspaces opened for the first time with v1.4.0 initialize cleanly with no manual setup.
- Workspaces and sessions created under v1.3.0 continue to work unchanged — no migration step.
- All v1.3.0 features are unchanged.

## Installation

See [INSTALLATION.md](INSTALLATION.md) for full setup. Windows x64 build artifacts are provided with this release (NSIS `.exe` and `.msi`).

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
