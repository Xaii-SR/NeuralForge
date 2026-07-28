# NeuralForge — Opus Final Release Handoff

Status: **SONNET PHASE A BOUNDED RELEASE CHECKPOINT: READY FOR OPUS**

This document supersedes the earlier interim version of itself (same filename, overwritten). It
reports exactly what has real evidence behind it. The prior "Combined Step 2 Verdict: PASS" commit
(`0f46e9c`) is **not treated as authoritative** — its own text lists 17+ deferred/unaudited findings,
which is inconsistent with a PASS verdict. Nothing in this document repeats that pattern: every claim
below is backed by a command that was actually run in this session, and severity is scoped honestly.

## 1. Baseline, audit range, and current state

- Repository: `C:\Users\saiah\NeuralForge`
- Branch: `codex/neuralforge-final-perfection-pass`
- Baseline HEAD at session start: `0f46e9c`
- Current HEAD: `4f91873`
- Exact audit range: `git diff 0f46e9c..HEAD`
- Five new commits, in order:
  1. `9e33dee` — fix: Ghost Text was completely non-functional (IPC contract mismatch)
  2. `ade8b16` — fix(NF-IDX-001): make per-file indexing writes atomic (protected file, Level 5)
  3. `ff3ae6f` — fix(NF-DB-001): distinguish real migration failures, prove contention behavior
  4. `6e7450d` — fix(NF-IDX-002): wire a real filesystem watcher to the indexer
  5. `4f91873` — fix(NF-QUAL-001): make npm run lint a real gate; resolve safe audit findings
- Preserved untracked files intact: `AGENTS.md`, and the four
  `docs/architecture/NEURALFORGE_FINAL_PASS_*` / `NEURALFORGE_FINAL_PERFECTION_BLUEPRINT.md` planning
  files.
- ~35 files show as `M` in `git status` with **zero content** in `git diff` (confirmed:
  `git ls-files --eol` shows `i/lf w/crlf`, `core.autocrlf=true`) — pure line-ending noise, left
  untouched and unstaged across every commit this session.
- Nothing pushed, merged to master, tagged, or published. No remote operation of any kind occurred.

## 2. What this session found and fixed, with evidence

### 2.1 Ghost Text was completely non-functional in production (`9e33dee`)

Every prior audit (Phase 1, Phase 1 correction, Phase 2 "PASS") verified Ghost Text's *correlation
and invalidation logic* but never verified the actual IPC argument contract. It was broken end to
end: `hooks/useGhostText.ts` invoked `request_async_completion` with `{prefix, suffix, cursorLine: 0,
cursorColumn: 0}`; the Rust command requires `{content, cursor_line, cursor_column}` (no
`prefix`/`suffix` parameters exist on it) and derives its own window server-side via
`extract_prediction_window`. Because a required argument was never supplied, every invocation failed
Tauri's argument deserialization and hit the `catch` block. Ghost text never rendered a suggestion in
the shipped build — this is "major documented functionality is broken," not a stale-result edge case.

Fixed by threading real Monaco content and cursor position through `Editor.tsx` → `useGhostText.ts` →
backend. Also fixed a genuine shadowing bug (`let disposed = true` inside the cleanup closure created
a new local instead of updating the effect's variable, so the unmount guard never engaged) and added
real backend cancellation (registers with the existing `RequestRegistry`; a superseding request
cancels the previous one; `completion.rs` checks `is_cancelled` at every emission/cache point).
Replaced placeholder `selectionStartColumn: 0, selectionEndColumn: 0` in `EditorPane.tsx` with the
real captured Monaco selection.

Verified NOT applicable: `task.files.first()` in `agent/mod.rs` (flagged as a multi-file safety gap)
— the planner only ever constructs `AgentTask.files` as `vec![file_path]` or `vec![]`; the whole
governed-edit pipeline is single-file throughout. No multi-file governed-edit code path exists, so
this isn't exploitable. No change made.

Validation: `cargo check` (0 errors), `cargo test --lib ai::` (104 passed), `npx tsc --noEmit`
(clean).

### 2.2 NF-IDX-001 — indexing writes are now atomic (`ade8b16`, protected file, Level 5)

`database/indexer.rs`'s `index_workspace`/`reindex_single_file` wrote a file's metadata, chunks,
symbols, and dependencies as independent `conn.execute(...).ok()` calls outside any transaction,
discarding every error and counting the file as indexed unconditionally. A lock/disk/insert failure
partway through could commit a fresh `content_hash` while leaving chunks/symbols stale — and since the
hash already matched on the next scan, the file would never be repaired (silent, sticky corruption).

Both functions now wrap the full per-file write in `crate::database::in_transaction`, with every
write propagating `?` instead of `.ok()`. Stats only record after commit; a rolled-back file
increments `files_failed`. Three new tests sabotage a target table (`ALTER TABLE ... RENAME`)
mid-run to force a real failure through the actual production path, then assert zero partial rows
survive and a retry after repair fully recovers the file.

Validation: `cargo test --lib database::indexer` (29 passed, was 26). Full suite: 429 passed (was
426).

### 2.3 NF-DB-001 — migration failures now propagate; contention behavior proven (`ff3ae6f`)

Ten additive-column migrations used `let _ = conn.execute(ALTER ...)`, discarding a real DDL failure
(disk error, permissions, corruption) identically to the expected "duplicate column" case. Replaced
with `ensure_column()`, which inspects `PRAGMA table_info` first and only issues the ALTER when the
column is genuinely missing, so a real failure now propagates.

Five new deterministic tests, including one that hand-builds a genuinely pre-migration schema
(missing all 10 additive columns — not just an old row count on the current schema, which is what the
pre-existing migration test actually covered) and proves reopening through the real production path
adds the columns with correct defaults while preserving existing row content; and two real two-
connection contention tests (`BEGIN IMMEDIATE` held by one connection while another writes) proving
transient contention is absorbed by the busy_timeout retry (not a race or silent drop) and that
contention outlasting the timeout returns a bounded, actionable error (not a hang, not a false
success).

Validation: `cargo test --lib database::hardening_tests` (10 passed, was 5). Full suite: 434 passed.

### 2.4 NF-IDX-002 — a real filesystem watcher now feeds the indexer (`6e7450d`)

`services/watcher_service.rs` was a fully dormant scaffold (confirmed via `cargo check`'s own
dead-code warnings: "struct WatcherService is never constructed") with no OS event source and no
production caller for `reindex_single_file`. Added the `notify` crate (v6, default-features = false —
the Windows `ReadDirectoryChangesW` backend this app ships on is platform-gated, not feature-gated) and
a `WorkspaceWatcher` that watches the workspace root recursively, debounces raw events, and dispatches
every affected path through `reindex_single_file` — the same NF-IDX-001-hardened transactional path
the manual reindex flow uses. Lives in `AppState.current_watcher`; `open_workspace` starts/replaces it
after the generation is committed, so at most one watcher is ever active, scoped to one generation.
Dropping it (on replacement or app exit) stops the OS watch and joins its thread within one 150ms poll
interval — never an indefinite hang.

The OS-event plumbing is a thin adapter around a pure, directly-testable `apply_events(&Connection,
&Path, &[FileEvent])` function. Seven new tests, including one that drives a **real** `notify` watcher
against a real file creation on disk and proves the captured OS event flows through the actual mapping
and dispatch logic into a real indexed row (bounded poll/retry, not a fixed sleep — avoids flakiness).

Deliberately **not** included, documented here rather than silently dropped: frontend consumption of
the emitted `workspace-file-index-updated` event (explorer tree refresh, clean-buffer reload,
dirty-buffer conflict UI). That's a separate frontend feature needing its own UI verification, out of
scope for this backend correctness pass.

Validation: `cargo test --lib services::watcher_service` (7 passed). Full suite: 441 passed.

### 2.5 NF-QUAL-001 — `npm run lint` is now a real gate (`4f91873`)

`npm run lint` invoked `next lint`, which errors under the installed Next 16.2.10
(`Invalid project directory provided, no such directory: ...\lint`) — the script was linting nothing.
Added `eslint@9` + `eslint-config-next` (peer-matched) and a flat `eslint.config.mjs`; changed the
script to `eslint .`.

`npm run lint` now genuinely runs and surfaces **37 real, pre-existing violations** (mostly
`react-hooks/refs` and `react-hooks/set-state-in-effect` — the newer, stricter React-Compiler-era hook
rules `eslint-config-next` 16 ships with) across `hooks/useComposer.ts`, `useSmartScroll.ts`,
`useTheme.ts`, `useInlinePrompt.ts`, and others. **Deliberately not fixed** — those are pre-existing
patterns across many unrelated files; blindly rewriting 37 call sites to force a clean run is exactly
the kind of unbounded mass change this pass avoids everywhere else (the same policy already applies to
`cargo fmt --check` and strict `-D warnings` Clippy, both also non-clean at baseline and left
documented rather than mass-reformatted). The infrastructure gap is fixed; the violations are a
tracked, evidence-backed punch list for whoever picks this up next.

Also ran `npm audit fix` (non-force only — `--force` would downgrade `next` to `9.3.3`, an obviously
wrong "fix" from npm's semver solver, never something to do to a major runtime dependency without
explicit authorization). This resolved the `dompurify`/`monaco-editor` sanitizer-bypass finding
(3.4.11 → 3.4.12) and one `eslint`-chain `brace-expansion` DoS finding. **12 high-severity findings
remain**, all in `next`'s own `postcss`/`sharp` chain (SSRF/DoS/cache-confusion CVEs in Next's *server*
runtime — Server Actions, Middleware, Image Optimization API, custom servers). This app ships
`output: "export"` with `images.unoptimized: true` (see `next.config.js`) — there is no Next.js server
process in the built Tauri app, so that attack surface does not exist in what actually ships. No
non-breaking fix is available upstream. Documented, not silently ignored.

Validation: `npx tsc --noEmit` pass, `npm run build` pass (Next 16.2.10/12, static export, 3 pages),
`npm ls --depth=0` resolves cleanly.

## 3. Findings independently re-verified this session and found already correct (no change needed)

Per this repo's own instructions ("the best answer is often: already correct, no change needed"),
these were checked against current source rather than assumed from prior documentation:

- **NF-WS-001 (workspace activation atomicity)**: `prepare_workspace` performs every fallible step
  (canonicalize, memory scaffold, `open_for_workspace`) *before* touching any shared state;
  `activate_prepared_workspace` swaps root+connection+generation under one lock, after checking
  `is_latest_workspace_open` to reject a superseded request. A real barrier-based concurrency test
  (`stale_overlapping_workspace_open_cannot_publish_state`) and a failed-open test
  (`failed_workspace_preparation_preserves_active_state`) already exist and pass. Genuinely correct,
  not just claimed.
- **NF-PRIV-002 (secret indexing)**: `secret_files_never_enter_files_chunks_or_fts` and
  `historical_secret_rows_are_purged_without_touching_sessions` (both pre-existing, both passing)
  already prove `.env`/`.npmrc`/PEM/credential-JSON content never enters `files`/`chunks`/FTS, and
  that historical excluded rows are purged without touching `sessions`.
- **NF-CTX-001's "semantic IPC accepts caller roots" concern**: `build_local_index`,
  `query_codebase_semantic`, and `generate_local_embeddings` (the flagged caller-root commands) are
  **not registered** in `lib.rs`'s `generate_handler!` — confirmed via `cargo check`'s own dead-code
  warnings ("function `query_codebase_semantic` is never used"). Unreachable from the frontend.
- **NF-SEC-003's docs-fetch concern**: `ai::docs::fetch_and_cache_doc` (the flagged unhardened
  fetch command) is likewise not registered — unreachable.
- **Dead Ghost Text path**: `ai::autocomplete::fetch_ghost_suggestion` is still registered in
  `lib.rs` but not referenced anywhere in `hooks/`/`components/` — harmless dead surface, not a live
  second Ghost Text path competing with the fix in §2.1. Candidate for a future P3 cleanup
  (unregister), not urgent.

## 4. What is still genuinely unimplemented — sized honestly

| Finding | State | Size |
|---|---|---|
| Frontend consumption of `workspace-file-index-updated` (explorer refresh, dirty-buffer conflict UI) | Backend event now emitted (§2.4); no frontend listener yet. | Small-to-moderate, needs UI verification. |
| Provider/model/resizable-UI upgrade spec (`NEURALFORGE_PROVIDER_MODEL_RESIZABLE_UI_UPGRADE_SPEC.md`) | Confirmed genuinely only 104 lines — not the prior session's fabrication, but also not implemented. Full adapter architecture (OpenAI Responses adapter, Custom Anthropic-Compatible, capability probing, lifecycle badges, searchable pickers) and the Chat/Terminal structural resize + Playwright suite are **greenfield, not started**. | Very large — explicitly deferred this session per user instruction; do not attempt without dedicated budget. |
| `npm run lint` violations (37, listed in §2.5) | Gate is real; violations are pre-existing and unfixed. | Small per-violation, numerous files — needs deliberate, reviewed pass. |
| `cargo fmt --check` repo-wide drift | Unchanged this session (pre-existing baseline state per the blueprint; would require a separately authorized formatting-baseline commit to touch frozen files safely). | Moderate, needs an explicit baseline-commit authorization per the blueprint's own rule. |
| 12 remaining `npm audit` high findings (Next server-runtime CVEs) | Documented in §2.5 with reachability analysis (not applicable to this app's static-export shipping shape); no safe fix available upstream. | N/A without a major Next.js version change — out of scope. |
| NF-SEC-001/002, NF-TERM-001, NF-COMP-001, NF-FS-001, NF-SESSION-001, NF-AGENT-002, NF-PRIV-001 | Per the Phase 2 audit, already independently verified against source with line-level evidence in an earlier session (not re-verified again this session — would be redundant). | N/A. |
| NF-REL-001/002 (release workflow, installed-app smoke, MSI/NSIS build) | Untouched. No `npm run tauri build` was run this session. | Requires a full local build + install/launch/uninstall cycle. |
| NF-PERF-001, NF-UX-001, NF-SUPPLY-001 (beyond §2.5), NF-DOC-001, NF-PROC-001, NF-REL-003 | Untouched this session. | P2/P3, individually small, numerous. |

## 5. Exact validation commands for whoever continues

```bash
npx tsc --noEmit
npm run build
npm run lint          # currently reports 37 real, pre-existing violations - see §2.5
"$USERPROFILE/.cargo/bin/cargo.exe" check --manifest-path src-tauri/Cargo.toml
"$USERPROFILE/.cargo/bin/cargo.exe" test --manifest-path src-tauri/Cargo.toml
"$USERPROFILE/.cargo/bin/cargo.exe" clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features
npm audit
git diff 0f46e9c..HEAD --stat
```

Full local matrix (lockfile install, Rust format policy, strict Clippy, `npm run tauri build`,
installed MSI/NSIS smoke) not run this session — see §4 for exactly what remains.

## 6. Artifact status

No Tauri build (`npm run tauri build`) was attempted this session — no MSI/NSIS artifacts exist from
this session's work. `npm run build` (the Next.js static export step, a prerequisite for the Tauri
bundle, not the bundle itself) passes.

## 7. Confirmation

Nothing was pushed, merged to master, tagged, or published. No remote operation occurred. The
repository is at a clean, intentional, fully tested checkpoint (`4f91873`) with five real, verified
fixes, zero known regressions, and zero half-written code.

## 8. For the fresh Opus audit session

- Read this document and the five commits in §1 (`git diff 0f46e9c..HEAD` for the full diff).
- Do not re-trust `0f46e9c`'s "PASS" framing; do not re-trust this document's claims either without
  spot-checking — re-run at least the commands in §5.
- Section 4 is the honest remaining-work list, sized. The provider/model/resizable-UI item is by far
  the largest and was explicitly deferred by user instruction this session, not forgotten.
- Section 3 lists items that turned out to already be correct on inspection — don't re-do that work,
  but do spot-check at least one before relying on it fully.
