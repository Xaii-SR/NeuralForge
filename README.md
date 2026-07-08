# NeuralForge

A local-first, offline-capable, AI-native desktop IDE. NeuralForge pairs a Monaco-based editor with a Rust/Tauri backend, a local Ollama-powered AI assistant that understands your project, and a safety-gated autonomous coding agent — all running on your machine, with no account, no cloud dependency, and no telemetry.

## What it is

- **A real code editor**: Monaco (the engine behind VS Code), a file explorer, tabs, and a full PTY terminal.
- **A local AI assistant**: chat with a model running on your own GPU/CPU via [Ollama](https://ollama.com), with your project's code and architecture decisions automatically injected as context.
- **A safety-first coding agent**: describe an objective, review the AI's proposed change and risk assessment *before anything is written*, and get automatic rollback if the change breaks your build.
- **An optimization layer**: response caching, per-model benchmarking, and automatic model selection based on your speed/quality/cost preferences.
- **An extensible platform**: a process-isolated plugin system for adding new capabilities without touching the core.

## Features

| Area | What's there |
|---|---|
| Editor | Monaco, multi-tab, syntax highlighting, Ctrl+S save, light/dark themes |
| Filesystem | Sandboxed file explorer and operations (every path validated against the open workspace) |
| Terminal | Real PTY (`portable-pty`), streams live shell output |
| AI Chat | Local Ollama models, streamed token-by-token, automatic workspace-context injection |
| Search | Full-text (FTS5) workspace search with stemming |
| Memory | Every workspace gets `.neuralforge/memory/` — architecture, decisions, rules, and history the AI reads before answering |
| Auto Mode | Picks a model automatically based on your goal (speed/quality) and cost preference |
| Caching | Identical questions return instantly from a local response cache |
| Benchmarking | Real tokens/sec and latency measured per model, stored and used for routing |
| Agent | Plans a file change, shows you the risk before applying it, runs `cargo check` (or equivalent) after, rolls back automatically on failure |
| Extensions | Process-isolated plugins with a mediated API — see [ARCHITECTURE.md](ARCHITECTURE.md) |
| Logging | Structured JSON logs, in-app viewer, exportable |

## Screenshots

Not included in this build — capturing them requires a human to run the installed app and take real screenshots (the automated preview tooling used during development renders a plain browser tab, not the actual desktop window, so a screenshot from it would be misleading). Take a few after installing and drop them here.

## Quick start

1. Install [Ollama](https://ollama.com) and pull at least one model: `ollama pull deepseek-coder`
2. Install NeuralForge (see [INSTALLATION.md](INSTALLATION.md))
3. Launch NeuralForge, click **Open Folder**, and pick a project
4. Ask a question in the chat panel, or describe a task in the **Agent** tab

See [USAGE_GUIDE.md](USAGE_GUIDE.md) for a full walkthrough of chat, agents, settings, and caching.

## Why local-first?

NeuralForge exists to replace subscription-based cloud AI coding tools with something that runs entirely on hardware you already own. It prioritizes local models first, degrading gracefully (not silently) when a capability — like semantic search, which needs an embedding model — isn't available on your machine. See [decisions.md](.neuralforge/memory/decisions.md) for the specific tradeoffs made throughout the build.

## Status

Phases 1 through 7 of the build are complete: foundation shell, local AI engine, context intelligence, AI optimization, the agent platform, an extension system, and self-analysis tooling. See [ROADMAP.md](ROADMAP.md) for what's deliberately deferred and why.

## Documentation

- [INSTALLATION.md](INSTALLATION.md) — setup on Windows, macOS, and Linux
- [USAGE_GUIDE.md](USAGE_GUIDE.md) — how to use every feature
- [ARCHITECTURE.md](ARCHITECTURE.md) — how it's built, for contributors
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) — common issues and fixes
- [ROADMAP.md](ROADMAP.md) — what's next, and what's intentionally not built yet

## License

Not yet set — add one before any public release.
