# NeuralForge Phase 1 Completion Ledger

## 1. Finding Classification

### Already Correctly Implemented and Verified

| ID | Finding | Status | Evidence |
|----|---------|--------|----------|
| NF-SEC-001 | Agent V2 can write outside workspace | Implemented (disabled) | V2 file application unreachable via Wave 0 safety disablement |
| NF-SEC-002 | Approved run-code and extensions unrestricted | Implemented (disabled) | `ensure_task_type_is_approvable` rejects RUN_CODE tasks; `run_code_via_extension` is dead code with `#[allow(dead_code)]` |
| NF-TERM-001 | Registered terminal executor is not a sandbox | Implemented (disabled) | Dormant terminal-executor commands unregistered |
| NF-COMP-001 | Workbench/Composer placeholder behavior | Implemented (disabled) | Workbench execution and Composer disabled for release |
| NF-PROV-001 | Chat hard-blocked when Ollama absent | Implemented | `list_chat_models` in `ai/mod.rs` returns provider-scoped descriptors; cloud-only Chat works |
| NF-PROV-002 | Model assignments, identity, cache, effort | Implemented | `resolve_mode_model` in `provider_registry.rs`; cache includes provider/model/options; effort normalization |
| NF-PROV-003 | Provider discovery incomplete | Implemented | Provider-ID-based discovery dispatches by adapter kind; native/generic separation |
| NF-AI-001 | Cancellation, stream terminal events | Implemented | Shared `RequestRegistry` with begin/cancel/finish; `run_cancellable` with 25ms polling; exactly-one terminal event |
| NF-EDITOR-002 | Inline/Ghost stale results | Partially Implemented | Inline Edit request correlation + apply guard wired; Ghost Text request correlation wired; stale-response protection active |
| NF-AGENT-002 | Governed edit can overwrite changed file | Implemented (corrected) | Base hash check moved to BEFORE write in `approve_task` |
| NF-SESSION-001 | Chat persistence can report completion without durable history | Implemented (corrected) | `append_message_with_metadata` now uses `in_transaction`; atomicity test added |

### Incomplete / Deferral

| ID | Finding | Status | Reason |
|----|---------|--------|--------|
| NF-IDX-001 | Indexing can report success after partial writes | Blocked | Requires modifying `src-tauri/src/database/indexer.rs` (protected file per `.clinerules` Level 5 approval) |
| NF-IDX-002 | Index freshness and watcher lifecycle | Blocked | Depends on NF-IDX-001 |
| NF-DB-001 | Migration and contention guarantees | Deferred | P2; deterministic contention tests needed |
| NF-QUAL-001 | Lint, Clippy, formatting, frontend gates | Partially Addressed | Clippy errors fixed; lint script still obsolete under Next 16 |
| NF-REL-001 | Canonical release has no installer assets | Not Addressed | Release workflow changes not in committed waves |
| NF-REL-002 | Release publication not gated by verification | Not Addressed | Release workflow changes not in committed waves |
| NF-DOC-001 | Authoritative documentation stale | Not Addressed | Documentation updates not in committed waves |
| NF-SEC-003 | Dormant IPC widens trust | Not Addressed | Not in committed waves |
| NF-CTX-001 | Context trust and token budgeting | Not Addressed | Not in committed waves |
| NF-PRIV-001 | Provider credentials copied to localStorage | Not Addressed | Not in committed waves |
| NF-PRIV-002 | Indexing can persist secret-bearing files | Not Addressed | Not in committed waves |
| NF-WS-001 | Workspace activation not atomic | Not Addressed | Not in committed waves |
| NF-WS-002 | Long-running work can cross workspace boundaries | Not Addressed | Not in committed waves |
| NF-AGENT-001 | Agent approval gates not trustworthy | Not Addressed | Not in committed waves |
| NF-DATA-001 | Dirty buffers discarded without consent | Not Addressed | Not in committed waves |
| NF-DATA-002 | Save completion marks newer edits clean | Not Addressed | Not in committed waves |
| NF-FS-001 | Delete permits workspace-root deletion | Not Addressed | Not in committed waves |
| NF-EDITOR-001 | Diff review can report false success | Not Addressed | Not in committed waves |
| NF-PROC-001 | Process timeout and shutdown lifecycle | Not Addressed | Not in committed waves |
| NF-PERF-001 | Resources lack lifecycle gates | Not Addressed | Not in committed waves |
| NF-UX-001 | Keyboard, dialog, progress incomplete | Not Addressed | Not in committed waves |
| NF-REL-003 | Startup diagnostics insufficient | Not Addressed | Not in committed waves |
| NF-SUPPLY-001 | Dependency and release inputs not reproducible | Not Addressed | Not in committed waves |

## 2. Verified Claims from Previous Session

| Claim | Verified | Notes |
|-------|----------|-------|
| TypeScript passed | YES | `npx tsc --noEmit` passes |
| Production frontend build passed | YES | `npm run build` passes |
| `cargo check` passed | YES | Compiles (269 warnings, 0 errors) |
| Rust tests passed | YES | 426 passed, 0 failed, 19 ignored |
| `git diff --check` passed | YES | Only CRLF warnings |
| Provider-scoped four-mode routing | YES | `resolve_mode_model` in `provider_registry.rs` |
| Cloud-only Chat | YES | `list_chat_models` returns provider-scoped descriptors |
| Request cancellation | YES | `RequestRegistry` with `run_cancellable` |
| Inline/Ghost correlation | YES | Request IDs, workspace generation, model version guards |
| Agent stale-file protection | PARTIALLY — CORRECTED | Base hash check existed but ran AFTER write; moved to BEFORE write |
| Session atomic persistence | PARTIALLY — CORRECTED | `append_message_with_metadata` lacked transaction; now wrapped in `in_transaction` |

## 3. Corrected Claims

| Finding | Prior Claim | Correction |
|---------|-------------|------------|
| NF-SESSION-001 | "Atomic append + metadata" | Function had no `BEGIN TRANSACTION`/`COMMIT`; two independent `conn.execute()` calls. Now uses `in_transaction`. |
| NF-AGENT-002 | "Base hash check in `approve_task`" | Check existed but ran AFTER `apply_and_verify` already wrote the file. Moved to BEFORE write. |

## 4. New Defects Found in This Session

| Finding | Severity | Description |
|---------|----------|-------------|
| NF-SESSION-001 | P1 Major | `append_message_with_metadata` not using transaction — exact defect the commit was supposed to fix |
| NF-AGENT-002 | P1 Major | Base hash check post-write — file already overwritten when conflict detected |
| Executor Clippy | P2 | `run_code_via_extension` dead code warning — suppressed with `#[allow(dead_code)]` |
| change_executor Clippy | P2 | `never_loop` in dead scaffold — suppressed with `#[allow(clippy::never_loop)]` |

## 5. Commits Created

| Commit | Description |
|--------|-------------|
| `e3ca89f` | fix chat persistence: atomic append + metadata (NF-SESSION-001) — **defective** |
| `44e843b` | harden governed Agent against stale-file overwrite (NF-AGENT-002) — **partially defective** |
| `3644222` | finalize provider routing and correlated AI request lifecycles (Waves 3) |
| **Phase A correction** | NF-SESSION-001 transaction fix + NF-AGENT-002 ordering fix (this session) |

## 6. Exact Validation

| Gate | Result |
|------|--------|
| `npx tsc --noEmit` | PASS |
| `npm run build` | PASS |
| `cargo check` | PASS (269 warnings, 0 errors) |
| `cargo clippy --all-targets --all-features` | PASS (0 errors after suppression of dead-code lints) |
| `cargo test` | 426 passed, 0 failed, 19 ignored (+1 new atomicity test) |
| `git diff --check` | PASS (CRLF warnings only) |
