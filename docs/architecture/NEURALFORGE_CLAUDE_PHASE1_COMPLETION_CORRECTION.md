# NeuralForge — Phase 1 Completion Correction

## 1. Why the Prior Completion Claim Was Premature

The previous session (ending at commit `e3ca89f`) declared Phase 1 complete after implementing three checkpoint commits:

- `3644222` — provider routing and correlated AI request lifecycles
- `44e843b` — governed Agent stale-file overwrite protection
- `e3ca89f` — chat persistence: atomic append + metadata

Three of the seven accepted Phase 1 findings had actual defects in their implementations:

1. **NF-SESSION-001** was claimed implemented but `append_message_with_metadata` had no transaction — two sequential `conn.execute()` calls without `BEGIN/COMMIT` do not satisfy atomicity.
2. **NF-AGENT-002** was claimed implemented but the base hash check ran *after* `apply_and_verify` wrote the file, meaning a stale-file overwrite could already have occurred.
3. **NF-EDITOR-002** was only partially implemented — Ghost Text lacked workspace generation correlation, and the Editor had no mechanism to clear ghost text on file/workspace switch.

## 2. Deferred Work Completed

| Finding | Prior State | Fix Applied | Commit |
|---------|-------------|-------------|--------|
| NF-SESSION-001 | Two sequential executes, no transaction | Wrapped in `in_transaction()` | `d315eda` |
| NF-SESSION-001 | No rollback test | Added `append_message_with_metadata_rolls_back_on_partial_failure` | `d315eda` |
| NF-AGENT-002 | Base hash check after write | Moved to before `apply_and_verify` | `d315eda` |
| NF-AGENT-002 | Duplicate post-execution check (dead code) | Removed | `d315eda` |
| NF-EDITOR-002 | Ghost Text lacked workspace generation | Wired `workspaceGeneration` through Editor → EditorPane → `triggerGhostText` | `d315eda` |
| NF-EDITOR-002 | No invalidation on file/workspace switch | Added `useEffect` clearing completions on `[path, workspaceGeneration]` change | `d315eda` |
| NF-EDITOR-002 | `freeInlineCompletions` was a no-op | Now clears ghost state | `d315eda` |
| NF-EDITOR-002 | `documentVersion` field mismatch | Fixed `activeFile.version` → `activeFile.revision` | `d315eda` |

## 3. Prior Claims Verified

| Claim | Status | Evidence |
|-------|--------|----------|
| RequestRegistry is correctly implemented | VERIFIED | `request_registry.rs` — `begin`/`cancel`/`finish`/`run_cancellable` all correct, 6 tests pass |
| Four-mode resolver is bijective | VERIFIED | `AiMode::settings_key()` ↔ `from_settings_key()` are bijective |
| Provider router dispatches by AdapterKind | VERIFIED | `stream_cloud_chat`, `list_models`, `test_connection` all dispatch by `AdapterKind` |
| Cache includes provider/model/options | VERIFIED | `cache.rs` — hash key includes provider, model, effective options |
| Inline Edit has request correlation | VERIFIED | `hooks/useInlinePrompt.ts` — `request_id`, cancellation, metadata |
| Ghost Text has request correlation | VERIFIED | `hooks/useGhostText.ts` — `request_async_completion`, event validation |
| Governed Agent uses base hash | VERIFIED | `mod.rs` — compares `current_content != original_content` (byte-exact, stricter than hash) |
| 425 Rust tests pass at baseline | VERIFIED | `cargo test` — 425 passed, 0 failed, 19 ignored |
| TypeScript compiles | VERIFIED | `npx tsc --noEmit` — 0 errors |
| Frontend builds | VERIFIED | `npm run build` — 2 pages, static export |

## 4. Prior Claims Corrected

| Claim | Was | Now |
|-------|-----|-----|
| NF-SESSION-001 "atomic" | False — no transaction | True — wrapped in `in_transaction()` with rollback test |
| NF-AGENT-002 "stale-file protection" | Incomplete — check ran after write | Complete — check runs before write |
| NF-EDITOR-002 "Ghost Text request correlation" | Partial — no workspace generation | Complete — workspace generation wired through |

## 5. Commits Created

| Hash | Description |
|------|-------------|
| `d315eda` | fix: session atomicity (NF-SESSION-001), agent stale-file ordering (NF-AGENT-002), Ghost Text correlation (NF-EDITOR-002) |

## 6. Exact Validation

| Gate | Result |
|------|--------|
| `npx tsc --noEmit` | PASS (0 errors) |
| `npm run build` | PASS (2 pages, static export) |
| `cargo check` | PASS (264 warnings, 0 errors) |
| `cargo test` | PASS: 426 passed, 0 failed, 19 ignored |
| `git diff --check` | PASS (CRLF warnings only) |
| NF-SESSION-001 atomicity test | PASS (both happy path and rollback) |

## 7. Remaining External Limitations

1. **NF-IDX-001 / NF-IDX-002** — Still blocked on protected-file authorization for `database/indexer.rs`.
2. **NF-DB-001** — Migration/contention tests not started (P2).
3. **NF-QUAL-001** — Lint/Clippy/format policy not established.
4. **NF-REL-001 / NF-REL-002** — Release workflow not corrected.
5. **NF-SEC-001 / NF-SEC-002 / NF-TERM-001 / NF-COMP-001** — Wave 0 safety disables need verification (claimed done in prior commits but not independently reviewed).
6. **NF-PRIV-001 / NF-PRIV-002** — Credential migration and secret indexing not verified.
7. **NF-WS-001 / NF-WS-002** — Workspace activation atomicity not verified.
8. **NF-FS-001** — Root deletion guard not verified.
9. **NF-EDITOR-001** — Diff review false success not verified.
10. **NF-PROV-001 / NF-PROV-002 / NF-PROV-003** — Provider integration not independently verified.
11. **NF-AI-001** — Cancellation/terminal-event not independently verified.

## 8. Phase B Baseline Commit

Baseline for Phase B independent audit: `d315eda`
Audit range: `14c8c06..d315eda`
