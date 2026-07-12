# NeuralForge v1.0.0

A local‑first, Cursor‑parity AI development environment built with Tauri, Next.js, and Rust.

## Architecture

| Layer | Technology |
|---|---|
| Desktop Shell | Tauri 2.0 |
| Backend | Rust |
| Frontend | Next.js 14 + React 19 + TypeScript |
| Editor | Monaco Editor |
| Terminal | Xterm.js + PTY |
| Storage | SQLite, JSON file cache, `.neuralforge/` workspace directory |
| AI Inference | Local Ollama, ONNX embeddings (fastembed) |

## Features (v1.0.0)

### Composer (Multi‑file AI Agent)
- Floating chat‑style pane with drag, resize, and close
- Code block parsing with file‑path headers (` ```language:path/to/file `)
- `@Codebase` semantic search with local ONNX vector embeddings
- `@Docs` context injection from locally cached documentation
- `@Web` real‑time web search via DuckDuckGo Lite (zero API keys)
- `@Git` context injection of workspace diff and status
- `@terminal` terminal output interception and injection
- Autonomous agentic tool‑calling loop (`<search_codebase>` tags)
- Agent status events and debug logging

### Editor Pane
- Monaco Editor with syntax highlighting, minimap, and theme support
- Side‑by‑side diff viewer for reviewing AI‑generated changes
- File explorer sidebar with tab management
- `Cmd+K` / `Ctrl+K` floating inline prompt widget with:
  - @‑mention menu for files, docs, and web search
  - Context pill visualization
  - Streaming code generation into the editor
  - Inline diff decorations (red/green line highlighting)
  - Accept/Reject with `Cmd+Enter` / `Escape` keybindings
- Ghost text autocomplete via Fill‑in‑the‑Middle (FIM) with Ollama
- File targeting system with `pendingDiff` queue and multi‑file apply

### Context Injection System
- `@Codebase` – semantic search via ONNX + cosine similarity
- `@Docs` – scrape and cache external documentation
- `@Web` – DuckDuckGo Lite scraping with HTML‑to‑text extraction
- `@Git` – read workspace git status and full diffs
- `@terminal` – rolling buffer of terminal history (ANSI stripped)
- `.neuralforgerules` – persistent project‑specific system instructions
- Agentic recursion – LLM can autonomously search the codebase

### Terminal Pane
- Integrated Xterm.js PTY terminal
- Background process spawning with `stdout`/`stderr` streaming
- "Fix with AI" button on error detection
- "Run in Terminal" button for executable bash/shell code blocks
- ANSI‑to‑text stripping for AI context

### AI Backend
- Ollama model management (list, pull, remove, health)
- Model auto‑selection based on task, VRAM, and benchmarks
- Response caching with hit/miss diagnostics
- `stream_inline_edit` for character‑by‑character inline streaming
- FIM ghost text predictions via `fetch_ghost_suggestion`
- Documentation scraper with `html2md` conversion to local cache

### Diff Review System
- Multi‑file `pendingDiffs[]` queue with sequential navigation
- DiffActionBar with `◀ File X of Y ▶`, Accept, and Reject
- Monaco DiffEditor for side‑by‑side comparison
- Transactional file writes on Accept, clean dismissal on Reject

## Quick Start

```bash
# Install dependencies
npm install

# Run in development mode (Next.js + Tauri)
npm run tauri dev

# Build for production
npm run tauri build
```

## Requirements

- Node.js 18+
- Rust 1.77+
- Ollama (optional, for local AI inference)

## License

MIT