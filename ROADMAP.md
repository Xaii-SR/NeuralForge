# Roadmap

## What's built (Phases 1-7)

1. **Foundation Shell** — Tauri 2 desktop app, Monaco editor, sandboxed file explorer, real PTY terminal, structured logging, per-workspace memory scaffold.
2. **Local AI Engine** — Ollama integration, hardware detection, VRAM-gated model loading, provider health tracking, streaming chat.
3. **Context Intelligence** — SQLite-backed FTS5 workspace search, memory injection, prompt management.
4. **AI Optimization Engine** — response caching, real per-model benchmarking, preference-driven auto model selection.
5. **Agent Platform** — a safety-gated Coder agent: propose → human approval → apply → verify → automatic rollback on failure.
6. **Advanced Platform** — process-isolated extension system with a mediated API; example extensions.
7. **Self Bootstrap** — NeuralForge can analyze its own codebase and propose changes to itself, gated behind the same human-approval flow as any other project.

## Deliberately not built, and why

- **Vector/semantic search.** The schema is ready (`chunks.embedding BLOB`), but no embedding model was available in the development environment, and enabling one would have meant restarting a service the developer was actively using for other purposes. FTS5 keyword search (with stemming) is the real, working search capability today. **Next step**: detect an available embedding-capable model at runtime, generate embeddings during indexing, add cosine-similarity ranking alongside FTS5 results.
- **Cloud AI providers.** `providers::ProviderId` lists all 11 providers from the original design, and pricing/cost-estimation logic already has real numbers for each — but only Ollama has a working HTTP client. No credential storage exists yet (by design: the blueprint requires OS keychain or encrypted SQLite for secrets, neither of which is built). **Next step**: pick one cloud provider, build its client matching `providers::ollama`'s shape, and build real credential storage before wiring in the first API key.
- **Multiple agent types.** Only "Coder" is implemented. The original design named Tester, Security, and Documentation agents as well. **Next step**: a Supervisor that routes tasks to the right agent type, reusing the existing task-queue/approval/rollback machinery — this is additive, not a rework.
- **Autonomous GitHub operations.** The self-bootstrap capability creates local git branches and runs tests, but does not push to a remote or open pull requests automatically. This is a deliberate boundary: pushing code and opening PRs are public, hard-to-reverse actions that should always go through a human clicking "push" themselves, not an agent doing it unprompted. **Next step, if wanted**: a "Push branch" button that's a clearly separate, explicit action from "apply suggestion" — never bundled into one click.
- **True extension sandboxing.** The extension system uses process isolation with a mediated API (the host validates every file/command request), which is real but not equivalent to a security sandbox (no seccomp/AppContainer-level OS restriction on what a plugin process itself can do, e.g. make arbitrary network calls). **Next step, if this matters for your use case**: run extension processes inside an OS-level sandbox, or migrate to a WASM runtime (wasmtime/wasmer) for genuine capability-based isolation.
- **Extension marketplace (as a live service).** There's no backend registry — the "marketplace" is a local manager for bundled and manually-installed extensions, not a browsable catalog of third-party plugins. **Next step, if wanted**: a real registry service, which is a meaningfully larger undertaking (hosting, review process, versioning, abuse prevention) than anything else on this list.
- **Cross-platform verification.** Built and tested on Windows only. The codebase has one platform-specific module (`hardware/gpu.rs`, Windows DXGI) that degrades gracefully elsewhere, but macOS/Linux builds have not been run end-to-end. **Next step**: CI on all three platforms before calling cross-platform support "done" rather than "should work."
- **Automated UI/E2E testing.** Every backend capability has real automated tests (SQLite, HTTP, PTY, git, filesystem). The frontend does not have an automated click-through test suite — verification during development relied on TypeScript's compiler, manual review, and (where possible) browser-preview inspection of a plain browser tab standing in for the actual Tauri window. **Next step**: a WebDriver-based E2E suite driving the actual built app, not a browser preview.

## Longer-term ideas (not scoped, not started)

- Resource governor (blueprint-specified: monitor RAM/CPU/VRAM/disk, throttle indexing or unload models under pressure). Today the VRAM check only runs at chat/agent time, not continuously.
- Snapshot/rollback for multi-file agent tasks (today: one file per task).
- A real model-quality benchmark (today's "quality" ranking is a parameter-count proxy, not an actual capability measurement).
- Auto-updater for the desktop app itself.
