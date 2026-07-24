# NeuralForge v1.4.2

A local-first, offline-capable, AI-native desktop IDE. Tauri 2 (Rust) backend, Next.js 16 frontend, powered by local (Ollama) and configurable cloud AI providers.

This release packages the validated stabilization and workflow improvements currently present in the repository, with a focus on database reliability, workspace context handling, provider model selection, and Ollama usability.

## What's new in v1.4.2

**Stability and Database Reliability**
- Enabled the correct SQLite concurrency configuration for the application's shared and background connection pattern.
- Corrected WAL initialization so journal mode is read correctly instead of being executed like a rowless statement.
- Preserved the existing database schema while improving concurrent session and indexing reliability.

**Workspace and Indexing Reliability**
- Improved workspace indexing for files modified more than once within the same filesystem timestamp interval.
- Added content-hash verification so same-second file changes are not incorrectly treated as unchanged.
- Preserved the existing workspace scoping behavior for sessions and reopened workspaces.

**File and Folder Context**
- Added explicit file and folder selection behavior in the explorer.
- Connected the selected file or folder to the active chat context.
- Preserved the existing file-opening workflow while making context selection available.

**Provider and Model Improvements**
- Added model selection to the provider configuration flow.
- Populated model presets for supported non-Ollama providers.
- Ensured saved Ollama model selections are honored instead of always defaulting to the first discovered model.

**Ollama Improvements**
- Added an Ollama model installation workflow from the provider UI.
- Added installed-model state handling to avoid duplicate or confusing install actions.
- Preserved the existing Ollama discovery architecture.

**Interface Improvements**
- Increased the default chat pane width.
- Corrected provider guidance placement and related form details.

## Validation

- `npx tsc --noEmit`: passed
- `npm run build`: passed
- `cargo check`: passed
- `cargo test`: passed, 376 passed / 0 failed / 19 ignored

Existing legacy Rust warnings remain in the codebase and were unchanged by this release.

## Installation

See [INSTALLATION.md](INSTALLATION.md) for full setup. Windows x64 build artifacts are produced through the repository's tag-driven GitHub Actions release workflow.
