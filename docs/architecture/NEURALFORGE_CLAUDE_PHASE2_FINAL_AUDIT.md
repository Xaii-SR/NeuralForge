# NeuralForge Combined Step 2 Phase A Audit

## 1. Repository Baseline

- Branch: `codex/neuralforge-final-perfection-pass`
- Phase A baseline (pre-correction): `e3ca89f`
- Phase A final commit: `4ae8ee6`
- Audit range: `14c8c06..4ae8ee6` (Waves 3-5 + corrections)

## 2. Phase A Findings

### Critical Defects Found and Fixed

**NF-SESSION-001 — `append_message_with_metadata` missing transaction**

The function claimed atomicity but contained two independent `conn.execute()` calls:
1. `INSERT INTO session_messages`
2. `UPDATE sessions`

If step 1 succeeded and step 2 failed, the message was persisted without metadata update — a silent partial-write. This is exactly the defect NF-SESSION-001 was supposed to fix.

Fix: Wrapped both statements in `crate::database::in_transaction()`. Added regression test that sabotages the `sessions` table mid-transaction and verifies both the INSERT and UPDATE are rolled back.

**NF-AGENT-002 — Base hash check ran AFTER write**

The `approve_task` function in `agent/mod.rs` called `apply_and_verify` (which writes the file) and THEN checked `current_content != original_content`. By the time the conflict was detected, the file was already overwritten.

Fix: Moved the content check to BEFORE the `apply_and_verify` call. The correct flow is now:
1. Read current content
2. Compare to approved snapshot
3. If different → CONFLICT immediately
4. If same → write file and verify

### Defects Verified (No Repair Needed)

- **RequestRegistry**: Single `Arc<Mutex<HashMap>>` — canonical owner, no duplication. Correct.
- **`chat_with_model_core` accepting `&[ChatMessage]`**: Avoids clone, callers pass `&messages` or `.to_vec()` as needed. Correct.
- **Cache identity includes provider + model + effective_options**: Prevents cross-provider contamination. Correct.
- **Ghost Text switched to `request_async_completion`**: Emits `ghost-text-stream` with `request_id`. Correct.
- **Inline Edit apply guard checks `workspaceGeneration > 0`**: Sanity check; backend authoritative. Correct.
- **`run_code_via_extension` and `format_extension_output`**: Dead code, suppressed with `#[allow(dead_code)]`. Correct for release.
- **`change_executor.rs` dead scaffold**: No production caller, `never_loop` suppressed. Correct.
- **`ensure_task_type_is_approvable`**: Rejects `RUN_CODE` tasks before approval. Correct.
- **`create_and_plan_code_task`**: Not registered in `lib.rs`, unreachable from renderer. Correct.

### Defects Deferred (Not in Scope of Committed Waves)

The committed waves (3, 4, 5) do not address these accepted findings:

| Finding | Wave | Status |
|---------|------|--------|
| NF-SEC-001 | 0 | Claimed completed; unverified in committed diff |
| NF-SEC-002 | 0 | Verified: dead code suppression + approval gate |
| NF-TERM-001 | 0 | Claimed completed; unverified in committed diff |
| NF-COMP-001 | 0 | Claimed completed; unverified in committed diff |
| NF-DATA-001 | 1 | Not in committed waves |
| NF-DATA-002 | 1 | Not in committed waves |
| NF-WS-001 | 1 | Not in committed waves |
| NF-WS-002 | 1 | Not in committed waves |
| NF-FS-001 | 1 | Not in committed waves |
| NF-EDITOR-001 | 1 | Not in committed waves |
| NF-AGENT-001 | 4 | Not in committed waves |
| NF-PRIV-001 | 2 | Not in committed waves |
| NF-PRIV-002 | 2 | Not in committed waves |
| NF-CTX-001 | 2 | Not in committed waves |
| NF-SEC-003 | 2 | Not in committed waves |
| NF-REL-001 | 7 | Not in committed waves |
| NF-REL-002 | 7 | Not in committed waves |
| NF-QUAL-001 | 7 | Partially addressed (Clippy fixed) |
| NF-DOC-001 | 7 | Not in committed waves |
| NF-PROC-001 | 6 | Not in committed waves |
| NF-PERF-001 | 6 | Not in committed waves |
| NF-UX-001 | 6 | Not in committed waves |
| NF-REL-003 | 6 | Not in committed waves |
| NF-SUPPLY-001 | 7 | Not in committed waves |

### Blocked Findings

| Finding | Reason |
|---------|--------|
| NF-IDX-001 | Requires modifying `src-tauri/src/database/indexer.rs` (protected file) |
| NF-IDX-002 | Depends on NF-IDX-001 |
| NF-DB-001 | P2; deferred to future wave |

## 3. Validation Results

| Gate | Result |
|------|--------|
| `npx tsc --noEmit` | PASS (0 errors) |
| `npm run build` | PASS (Next.js 16.2.10, 3 pages) |
| `cargo check` | PASS (269 warnings, 0 errors) |
| `cargo clippy --all-targets --all-features` | PASS (0 errors) |
| `cargo test` | 426 passed, 0 failed, 19 ignored |
| `cargo test release_validation -- --ignored` | 2 passed |
| `cargo fmt --check` | DRIFT in pre-existing files (not our changes) |
| `git diff --check` (committed) | PASS |

## 4. Security Verdict

- No unsafe surfaces reachable in committed code
- `run_code_via_extension` properly dead-coded
- `change_executor.rs` properly disconnected
- Credential handling: unverified in committed waves (Wave 2)
- Secret indexing: unverified in committed waves (Wave 2)

## 5. Data Integrity Verdict

- Session atomicity: FIXED (transaction)
- Agent stale-file: FIXED (check-before-write)
- Dirty buffer protection: unverified (Wave 1)
- Workspace atomicity: unverified (Wave 1)

## 6. Provider/Model Verdict

- Provider-scoped model identity: VERIFIED
- Four-mode resolver: VERIFIED
- Cache identity includes provider: VERIFIED
- Cancellation registry: VERIFIED
- Cloud-only Chat: VERIFIED

## 7. Request Correlation Verdict

- Inline Edit: request ID + workspace gen + document version + selection
- Ghost Text: request ID + event validation + cleanup on unmount
- Exactly-one terminal event per request: VERIFIED
- Cancellation reaches transport: VERIFIED (25ms polling loop)

## 8. Documentation Verdict

- Phase 1 handoff updated with corrections
- Completion correction document created
- Provider/model/resizable-UI spec created

## 9. Phase A Completion Gate

The following conditions are met for Phase A:

- NF-SESSION-001: FIXED
- NF-AGENT-002: FIXED
- Clippy errors: RESOLVED
- Dead-code warnings: SUPPRESSED with documentation
- TypeScript: PASS
- Production build: PASS
- Rust tests: PASS
- Release validation: PASS

The following conditions are NOT met (Wave 0-2 findings not in committed waves):

- NF-SEC-001, NF-SEC-002, NF-TERM-001, NF-COMP-001 (Wave 0): Unverified
- NF-DATA-001, NF-DATA-002, NF-WS-001, NF-WS-002, NF-FS-001, NF-EDITOR-001 (Wave 1): Unverified
- NF-PRIV-001, NF-PRIV-002, NF-CTX-001, NF-SEC-003 (Wave 2): Unverified
- NF-AGENT-001 (Wave 4): Unverified
- NF-REL-001, NF-REL-002, NF-QUAL-001, NF-DOC-001, NF-PROC-001, NF-PERF-001, NF-UX-001, NF-REL-003, NF-SUPPLY-001 (Waves 6-7): Not addressed

## 10. Residual Risk

- Waves 0-2 findings are claimed as completed by the previous session's handoff but their implementation is not visible in the committed diff. This requires independent verification.
- Protected-file findings (NF-IDX-001, NF-IDX-002) remain blocked.
- P2 findings (NF-DB-001, NF-QUAL-001, etc.) are deferred.
- The provider/model/resizable-UI upgrade spec is created but not implemented.

## 11. External Limitations

- MSI/NSIS build: Not performed
- Installed-app smoke: Not performed
- Remote release validation: Not authorized
- Live provider tests: Not performed
- Keyboard/axe accessibility tests: Not performed
- Performance benchmarks: Not performed
