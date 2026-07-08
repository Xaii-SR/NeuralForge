# Troubleshooting

## "Ollama not detected"

The chat, agent, and benchmarking panels all check `http://localhost:11434` on load.

- Confirm Ollama is actually running: `ollama list` in a terminal should return without error.
- If you just installed Ollama, it may need a manual start (`ollama serve`, or relaunch the Ollama app).
- Firewall/antivirus software occasionally blocks local loopback connections on port 11434 — check if it's blocking NeuralForge or Ollama specifically.
- This is not a crash — the rest of the app (editor, file explorer, terminal, extensions) works normally without Ollama.

## Chat model dropdown is empty

You have Ollama running but no models pulled. Run `ollama pull deepseek-coder` (or any model) in a terminal, then reopen NeuralForge or click into the chat panel again.

## Agent task fails during planning

Check the task's `error` field (visible if you inspect `agent_tasks` in `.neuralforge/index.db`, or watch the Logs tab for a `task_planning_failed` event). Common causes:

- Ollama became unreachable mid-request — same fixes as "Ollama not detected" above.
- The target file doesn't exist yet — the agent edits existing files; it doesn't create new ones from scratch (yet).
- The file path was outside the workspace — the agent, like every other file-touching feature, refuses to operate outside the open folder.

## Agent task was rolled back

This is the safety mechanism working correctly, not a bug. The proposed change failed verification (for `.rs` files, a real `cargo check`) and the original file content was automatically restored. Check the **Verification** field in the task detail pane for the actual compiler error, then either try a more specific objective or make the change yourself.

## Search returns nothing / chat has no workspace context

Click **Index Workspace** in the chat panel first. Search only knows about files that have been indexed; a freshly opened workspace has an empty index. Re-index after making significant changes — unchanged files are skipped automatically, so it's cheap to run again.

## Terminal doesn't respond / shows garbled output

The terminal is a real PTY (`portable-pty`) running `cmd.exe` (or your platform's default shell) — issues here are almost always shell-level, not NeuralForge-level. If the terminal panel itself won't open at all, check the Logs tab for a `spawn_shell` failure.

## The app won't launch at all

Check the log file directly (doesn't require the app to be running):

- Windows: `%LOCALAPPDATA%\com.neuralforge.ide\logs\app.log`

Each line is a JSON object with `timestamp`, `level`, `target`, and `fields`. Look for `ERROR` or `WARN` entries near the end of the file.

## Building from source fails

- **`error: linker 'link.exe' not found`** (Windows): the MSVC "Desktop development with C++" workload isn't installed. Install it via the Visual Studio Installer, or `winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools"`.
- **`rusqlite`/`libsqlite3-sys` compile errors**: these bundle and compile SQLite from source (`bundled-full` feature) — this needs a working C compiler, which the MSVC workload above provides on Windows.
- **First build is very slow**: expected — it compiles the full Rust dependency graph (Tauri, SQLite, reqwest, etc.) from scratch. Subsequent builds are incremental and much faster.

## Benchmarks show "n/a" for tokens/sec

The model hasn't been benchmarked yet, or the last benchmark run failed (network issue, model unloaded mid-run). Click **Benchmark** next to the model in Settings to run it again — it's a real short prompt against the live model, so it takes a few seconds.

## I changed my theme preference and it didn't stick

Theme is stored in browser local storage, not in a workspace file — if you're testing across multiple app data profiles or a fresh install, this resets. This is intentional (it's a personal display preference, not project data).
