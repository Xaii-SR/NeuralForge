# Installation

## Requirements

- **[Ollama](https://ollama.com)**, installed and running, with at least one model pulled (e.g. `ollama pull deepseek-coder`). NeuralForge's chat, agent, and benchmarking features all depend on a local Ollama instance at `http://localhost:11434`. Without it, NeuralForge still works as a plain editor — the AI panels will show a clear "Ollama not detected" state instead of failing silently.
- Optional: **Python 3** on your `PATH`, if you want to use the bundled Python REPL extension.

## Windows

1. Install Ollama from https://ollama.com/download and pull a model.
2. Run the NeuralForge installer:
   - `neuralforge_<version>_x64_en-US.msi` (MSI, for managed/enterprise installs), or
   - `neuralforge_<version>_x64-setup.exe` (NSIS, standard installer)
3. Launch NeuralForge from the Start menu.

Both installers are produced by `cargo tauri build` from `src-tauri/target/release/bundle/`.

### Building from source (Windows)

```powershell
# Prerequisites: Rust (via rustup), MSVC "Desktop development with C++" workload, Node.js
npm install
cargo install tauri-cli --version "^2.0.0" --locked
cargo tauri build
```

The build takes several minutes on first run (compiles the full Rust dependency graph, including bundled SQLite with FTS5). Installers land in `src-tauri/target/release/bundle/msi/` and `.../bundle/nsis/`.

## macOS / Linux

This build was developed and tested on Windows only. Tauri 2 supports macOS and Linux, and the codebase has no Windows-only Rust code *except* GPU detection (`src-tauri/src/hardware/gpu.rs` uses the Windows DXGI API directly) — on other platforms, GPU vendor/VRAM detection returns an empty list rather than failing to build, and everything else (chat, agent, search, caching) works identically since it doesn't depend on GPU info being present.

To build on macOS/Linux:

```bash
npm install
cargo install tauri-cli --version "^2.0.0" --locked
cargo tauri build
```

You'll need the platform's usual Tauri prerequisites (Xcode Command Line Tools on macOS; `libwebkit2gtk`, `libssl-dev`, `build-essential` and friends on Linux — see [Tauri's prerequisites guide](https://tauri.app/start/prerequisites/)). This hasn't been verified end-to-end on either platform — treat it as "should work," not "confirmed working."

## Verifying the install

1. Open NeuralForge.
2. Click **Open Folder** and pick any local project.
3. Check the **Agent** tab status — if Ollama is reachable, the chat panel will populate a model dropdown instead of showing "Ollama not detected."
4. Click **Index Workspace** in the chat panel, then ask a question about your code — a fast response with real project context confirms the whole pipeline (indexing, search, memory injection, local inference) is working.

## Uninstalling

Standard Windows "Apps & Features" removal. NeuralForge's own data lives in:

- `%LOCALAPPDATA%\com.neuralforge.ide\` — logs
- `%APPDATA%\com.neuralforge.ide\` — model benchmarks database
- `<your-workspace>\.neuralforge\` — per-project index, memory, and settings (created inside whatever folder you open, not centrally — remove manually per project if desired)
