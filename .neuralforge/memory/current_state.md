# Current State

**Phase 1 (Foundation Shell): complete.**

All 9 build steps done, tested, and committed individually:
1. Repo scaffolding
2. Static Next.js frontend
3. Monaco editor + tabs
4. Tauri 2 desktop shell
5. Filesystem IPC + file explorer
6. PTY terminal emulator
7. Centralized event bus
8. Logging system + log viewer
9. Memory folder scaffolding

**Final gate verification:**
- `cargo test` (src-tauri): 10/10 passing — filesystem path-validation/traversal
  tests, terminal spawn/write/read integration test (real PTY, not mocked),
  memory-scaffold creation + non-overwrite tests.
- `cargo tauri dev`: launches, loads the Next.js UI (`GET / 200`), terminal
  spawns a real PTY session on mount (confirmed via log output).
- Log file confirmed on disk at `%LOCALAPPDATA%\com.neuralforge.ide\logs\app.log`
  with correct JSON structure (timestamp/level/target/fields).
- `cargo tauri build`: produces both
  `src-tauri/target/release/bundle/msi/neuralforge_0.1.0_x64_en-US.msi` and
  `.../bundle/nsis/neuralforge_0.1.0_x64-setup.exe`, no warnings.

**Not yet manually click-tested in the running GUI** (open folder → browse →
edit → save → terminal → logs, end to end as a human). Verified instead
through automated tests + direct disk/log inspection at each step, per
explicit instruction to avoid requiring manual confirmation between steps.
Worth a real click-through before calling Phase 1 fully done from a UX
standpoint, not just a correctness one.

**Phase 2 (Local AI Engine): complete.**

Built: hardware detection (cpu/memory/gpu via sysinfo + DXGI), Ollama client
(health/list/pull/remove/chat), provider registry (11 providers, auth stub -
no credential storage), provider health tracking (latency window + failure
cooldown), VRAM-gated model loading, streaming chat wired to a ChatPane UI.

**Verification:**
- `cargo test`: 16/16 passing, plus 1 `#[ignore]`d live test
  (`chat_stream_produces_real_tokens_from_local_model`) run on demand against
  the real local Ollama instance and `deepseek-coder:latest` - genuine
  streamed content and a final `done:true`, not mocked.
- `cargo tauri dev`: boots clean with the full AI module registered, no
  runtime panics.
- Ollama was already installed and running locally (v0.31.1, 4 models
  pulled) - no install/download was needed for gate testing.

**Not yet manually click-tested in the running GUI** (same caveat as Phase 1 -
select model → type question → watch it stream in the actual window). The
underlying HTTP/streaming pipeline is verified for real; the last mile
(React state updates rendering correctly) is exercised by TypeScript's
compiler and the code's own logic, not a human eye.

**Next**: Phase 3 (Context Intelligence) — SQLite database, vector indexing,
code parsing, workspace search, memory injection, prompt management.
