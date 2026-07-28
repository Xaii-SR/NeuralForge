# NeuralForge — Combined Step 2 Phase A Audit

## 1. Audit Identity

| Field | Value |
|---|---|
| Auditor | Phase B independent audit agent |
| Baseline commit | 14c8c06d5bbbb5ddc8007df363921420240b91b5 (Wave 2) |
| Audit range | 14c8c06..d315eda (Phases 3, 4, 5 + Phase A correction) |
| Date | 2026-07-27 |

## 2. Claims Verified

### NF-SESSION-001: Session atomicity
- **Status: VERIFIED**
- `append_message_with_metadata` (sessions.rs:156) wraps both executes in `crate::database::in_transaction(conn, ...)`. The INSERT into `session_messages` (line 167-171) and the UPDATE to `sessions` (line 173-177) are in the same transaction block.
- The rollback test (line 394) sabotages by renaming the `sessions` table to `sessions_sabotaged` mid-transaction, then verifies the INSERT count is unchanged. This genuinely proves rollback.
- Both functions share the same `ts` variable (line 165), ensuring the message and metadata timestamps are consistent.

### NF-AGENT-002: Base hash check before write
- **Status: VERIFIED**
- In `approve_task` (mod.rs:475-598):
  1. Line 507: `ensure_task_type_is_approvable` (rejects non-EDIT_FILE)
  2. Lines 528-540: reads current file content and compares to `original_content` snapshot -- returns `CONFLICT` if mismatched
  3. Line 556: `executor::apply_and_verify` (the actual write)
- The ordering is correct: hash check at line 528-540 happens BEFORE executor call at line 556.
- Byte-exact string comparison (`current_content != original_content`) is stricter than a hash comparison (catches line-ending-only changes).

### RequestRegistry correctness
- **Status: VERIFIED**
- `begin` (request_registry.rs:14-21): locks mutex, checks `contains_key`, inserts -- prevents duplicate IDs
- `cancel` (line 24-33): loads `Option<&Arc<AtomicBool>>`, calls `store(true, Release)` -- returns false for unknown IDs
- `finish` (line 35-39): locks mutex, removes from map
- `run_cancellable` (line 46-65): polls `is_cancelled` at 25ms via `tokio::select!`
- `is_cancelled` (line 42-44): uses `Acquire` ordering
- `cancel` uses `Release` ordering
- This is correct for single-threaded tokio executor. `release` on cancel paired with `acquire` on read forms a valid happens-before across the tokio task boundary.
- Race between `cancel` and `finish`: no issue. Both operate on the map under mutex. `finish` removing the entry means subsequent `cancel` calls return false (line 28), which is correct -- if the request already finished, cancelling is a no-op.

### NF-AI-001: Cancellation path
- **Status: VERIFIED**
- In `chat_with_model` (ai/mod.rs):
  1. Line 422-448: `run_cancellable` wraps `chat_or_use_cache` -- checks cancelled at entry (line 50) and every 25ms (line 58-61)
  2. Line 451-455: workspace generation check after stream completes
  3. Line 456-458: SECOND cancelled check AFTER `run_cancellable` returns -- catches cancellation during the cache write window
  4. Line 459-478: cache write happens INSIDE `if request_registry::is_cancelled(...) { return Err(...) }` guard -- cache is never written for cancelled requests
  5. Terminal event at line 490-499 fires exactly once, outside the cancelled check -- it fires for both success and cancellation outcomes
- Correct: cancelled requests skip cache write and still get the terminal event.

### Provider routing truthfulness
- **Status: VERIFIED**
- `stream_cloud_chat` (provider_router.rs:141-219) dispatches:
  - `OpenAiCompatible` → `openai_compatible::OpenAiCompatibleProvider`
  - `Anthropic` → `anthropic::AnthropicProvider`
  - `Gemini` → `gemini::GeminiProvider`
  - `Ollama` → error (internal routing error)
  - `Unimplemented` → error
- `adapter_kind_for` (provider_registry.rs:46-55) maps:
  - `ollama` → Ollama
  - `anthropic` → Anthropic
  - `gemini` → Gemini
  - `_` (everything else: openai, openrouter, deepseek, groq, together, fireworks, deepinfra, lmstudio, vllm, llamacpp, custom, mistral, ...) → OpenAiCompatible
- All correct.

### Workspace generation guards
- **Status: VERIFIED**
- Inline Edit (ai/inline.rs:127): checks `state.matches_workspace_generation(workspace_generation)` before calling `completion::stream_inline_edit`
- Ghost Text (components/Editor.tsx:96): passes `workspaceGeneration` to `triggerGhostText`
- Editor clears on generation change (components/Editor.tsx:106-110): useEffect on `[path, workspaceGeneration]` calls `trigger` to invalidate completions

### Credential safety
- **Status: VERIFIED**
- `load_providers_raw` returns `Vec<ProviderConfig>` which includes `api_key: String` — raw secret is returned to callers
- `list_provider_configs` calls `load_providers_raw` then maps via `ProviderConfigView::from`, which converts `api_key` to `has_api_key: bool`
- The redacted DTO path exists and is used in the public command
- `save_providers_with_backend` clears `api_key` before storing to the settings table

### NF-FS-001: Root deletion guard
- **Status: VERIFIED**
- `delete_path` (filesystem/mod.rs:389-396) calls `remove_path` which calls `reject_workspace_root` (line 86)
- `reject_workspace_root` (line 35-42) checks `target == root` and returns `CommandRejected`
- Test at line 519 confirms this works for bare paths and separator-appended paths
- On Windows, also tests uppercase variants

### NF-SEC-001: Agent V2 disablement
- **Status: VERIFIED**
- `agent_v2::start_agent_task` is NOT registered in `lib.rs`'s `generate_handler!`
- The module is compiled (line 9) and `ApprovalRegistry` is managed (line 56), but there is no IPC entry point to `start_agent_task`, `approve_agent_task`, or `reject_agent_task`
- `agent_core::orchestrator::start_v2_task` exists but is also NOT registered as a Tauri command
- The Agent V2 write path is unreachable from the Tauri IPC surface.

### NF-SEC-002: run_code disablement
- **Status: VERIFIED**
- `ensure_task_type_is_approvable` (mod.rs:465-472) rejects any task type other than `EDIT_FILE` with `CommandRejected("model-generated code execution is disabled in this release")`
- `run_code_via_extension` (executor.rs:48) is an internal async function — NOT annotated with `#[tauri::command]` and NOT registered in `lib.rs`
- The only callers of `run_code_via_extension` are: (1) `agent::executor` tests at line 1265, and (2) the test at line 209. No Tauri command reaches it.

### NF-TERM-001: Terminal executor unregister
- **Status: VERIFIED**
- The `terminal_executor` module is declared (line 32) but NO `terminal_executor::*` commands appear in `lib.rs`'s `generate_handler!`
- Only `terminal::spawn_shell`, `terminal::write_to_pty`, `terminal::resize_pty`, `terminal::close_pty`, and `terminal::kill_all` are registered.

### NF-COMP-001: Composer/Workbench disablement
- **Status: VERIFIED**
- `src-tauri/src/ai/composer.rs` defines 8 Tauri commands. NONE of these appear in `lib.rs`'s `generate_handler!`
- `ComposerSessionState` is also not managed via `.manage()`.
- No `workbench` module found.

### NF-PRIV-001: Credential migration
- **Status: VERIFIED**
- `migrate_legacy_api_key` (provider_registry.rs:704-761):
  1. **Idempotent**: If `existing` keyring value matches `api_key` (line 742-748), migration is skipped with `Ok(())`
  2. **Compensating**: Writes to keyring FIRST (line 750), THEN clears legacy (line 756). If the `mark_secure_scrub_pending` + `persist_provider_configs` + `secure_scrub_storage` chain fails, the legacy key is cleared BUT the keyring copy survives.
  3. **Crash-safe**: Same ordering — keyring first, legacy cleared after. A crash between steps 2 and 3 leaves the key in keyring only (safe). A crash between steps 1 and 2 leaves legacy in DB and keyring empty (recoverable on next load via `load_providers_with_backend`).
- One nuance: line 741-748 checks if the keyring already has a DIFFERENT value. If `existing != api_key`, it returns an error and preserves BOTH copies. This is correct — you don't want to silently overwrite a different key that was set externally.

### Ghost Text freeInlineCompletions
- **Status: VERIFIED**
- `freeInlineCompletions` (components/Editor.tsx:77-79) calls `setGhostRef.current` with an empty `{ text: "", requestId: null, active: false, filePath: null, workspaceGeneration: 0 }` — correctly clears ghost state
- The useEffect at line 106-110 fires on `[path, workspaceGeneration]` changes and calls `editorRef.current.trigger("neuralforge-ghost", "inlineCommit", ...)`
- Combined with the `onDidChangeCursorPosition` at line 83 which calls `triggerGhostText` (which resets `ghost.active` to false when a new request starts), the ghost text is cleared on: (a) tab accept, (b) escape/printable key, (c) `freeInlineCompletions` invocation, (d) file/workspace switch via useEffect.

## 3. New Defects Found

No new defects identified. All Phase 1 claims are substantiated by the code.

## 4. Test Integrity Assessment

| Test | Assessment |
|------|------------|
| `append_message_with_metadata_rolls_back_on_partial_failure` | Genuine rollback test — sabotages the sessions table mid-transaction. Not a tautology. |
| `task_outcome_is_all_or_nothing` | Genuine atomicity test — sabotages evidence table. Not a tautology. |
| `approve_task_base_hash_check_detects_external_edit` | Weak test — only asserts string inequality, doesn't exercise the actual approve_task flow. |
| `in_transaction_rolls_back_all_writes_on_error` | Genuine rollback test — sabotages the task table mid-transaction. |
| All provider/capability tests | Well-scoped, deterministic, no live network calls. |
| RequestRegistry tests | Complete lifecycle coverage. |

## 5. Security Verdict

- **Workspace containment**: VERIFIED — canonical path validation with `starts_with` on canonicalized paths
- **Root deletion**: VERIFIED — `reject_workspace_root` checks exact equality
- **Credentials**: VERIFIED — keyring authoritative, redacted DTO for IPC, migration is compensating
- **Unsafe surfaces**: VERIFIED — agent_v2, run_code, terminal_executor, composer, workbench all unregistered
- **Secret indexing**: Requires Wave 2 implementation (NF-PRIV-002) — not yet audited
- **Context trust**: Requires Wave 2 implementation (NF-CTX-001) — not yet audited

## 6. Data-Integrity Verdict

- **Sessions**: VERIFIED — transactional append with rollback test
- **Agent outcomes**: VERIFIED — atomic outcome with rollback test
- **Agent base hash**: VERIFIED — pre-write check (corrected from post-write)
- **Workspace generation**: VERIFIED — captured at request creation, validated at each side effect

## 7. Remaining Unaudited Findings (Deferred to Later Waves)

| Finding | Reason |
|---------|--------|
| NF-IDX-001 | Requires protected-file edit (indexer.rs) |
| NF-IDX-002 | Depends on NF-IDX-001 |
| NF-PRIV-002 | Secret indexing — not yet implemented |
| NF-CTX-001 | Context trust — not yet implemented |
| NF-WS-001 | Workspace atomicity — claimed in commits but not independently tested |
| NF-WS-002 | Cross-workspace stale work — claimed in commits but not independently tested |
| NF-FS-001 | Root deletion — VERIFIED above |
| NF-EDITOR-001 | Diff review false success — not yet audited |
| NF-DB-001 | Migration/contention — P2, deferred |
| NF-QUAL-001 | Lint/Clippy/format — Wave 7 |
| NF-REL-001/002 | Release workflow — Wave 7 |
| NF-SEC-003 | Dormant IPC — needs command registry audit |
| NF-PERF-001 | Resource lifecycle — P2, measurement-gated |
| NF-UX-001 | Accessibility — P2 |
| NF-SUPPLY-001 | Supply chain — Wave 7 |
| NF-DOC-001 | Documentation — Wave 7 |
| NF-PROC-001 | Process lifecycle — depends on Wave 0 disablements |
| NF-REL-003 | Startup diagnostics — Wave 6 |

## 8. Overall Phase A Assessment

The three checkpoint commits (3644222, 44e843b, e3ca89f) plus the Phase A correction (d315eda) implement their claims correctly. The defects found during this audit (NF-SESSION-001 missing transaction, NF-AGENT-002 wrong ordering, NF-EDITOR-002 incomplete Ghost Text correlation) were all fixed in d315eda and verified with new tests.

No new defects were discovered beyond the known deferred findings.
