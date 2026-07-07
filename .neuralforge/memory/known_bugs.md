# Known Bugs

None open as of Phase 1 completion.

**Resolved during Phase 1:**
- Filesystem `create_file`/`create_dir`/`rename_path` originally wrote to
  disk *before* validating the target was inside the workspace root
  (validation used `fs::canonicalize`, which requires the path to already
  exist — a TOCTOU gap for not-yet-existing paths). Fixed by validating the
  parent directory instead for new paths. Caught before merging via the
  filesystem unit tests.
- Terminal PTY spawn appeared to hang indefinitely in a synthetic test
  harness. Root cause was ConPTY's startup cursor-position query
  (`ESC[6n]`) going unanswered — not a bug in the shipped terminal feature
  (xterm.js, used by the real frontend, answers this automatically). See
  `decisions.md`.

**Open items for later phases (not bugs, just not yet done):**
- No manual GUI click-through of the full workspace flow yet (see
  `current_state.md`).
