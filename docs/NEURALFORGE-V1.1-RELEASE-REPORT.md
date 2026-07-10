# NEURALFORGE V1.1 RELEASE REPORT

Date: 2026-07-10 · Validated commit: `49cb2a6` · Sprint 10 (Release Candidate Validation)

## 1. Feature inventory (Sprints 1–9, all delivered and tested)

| Sprint | Capability |
|---|---|
| 1 | **Requirement Intelligence** — validated requirement contracts (title/intent/acceptance criteria) gate all agent file-edit work; weak requirements rejected before any model call; versioned with history |
| 2 | **Traceability Ledger + Evidence** — append-only, hash-chained (SHA-256) audit ledger with `verify_chain()` tamper detection; structured evidence records with deterministic insertion ordering; correlation IDs threading requirement → task → evidence → verdict |
| 3 | **Task DAG Planning** — multi-task decomposition with cycle/orphan/duplicate detection, topological execution, failure-blocks-dependents-not-siblings semantics |
| 4 | **Promotion Governance** — one PromotionController for both the agent flow and the self-bootstrap flow; promotion requires passing evidence; refusals are as auditable as approvals |
| 5 | **Worker Intelligence** — worker profiles, capability matching (capability coverage dominates, reliability tie-breaks), reliability *derived* from real promotion verdicts |
| 6 | **Integration proof** — north-star scenario ("add input validation to a save function, with a test") end-to-end with no mocks; found and fixed two non-determinism bugs |
| 7 | **Production hardening** — transaction boundaries around all multi-statement governance sequences, migration-safety and corruption-handling tests, 1000-entry chain volume test; remediation: panic-safe read-back, fully atomic task outcomes, explicit `PRAGMA foreign_keys = ON` |
| 8 | **Autonomous Reliability Layer** — failure classification, bounded human-gated retries with lineage (`retry_of`), DAG retry-recovery (blocked dependents reopen when a retry succeeds), execution confidence scoring, evidence-completeness validation, structured task reports |
| 9 | **UX + Developer Interface** — typed TS bindings for all backend commands; Governance panel (ledger browser + chain verify), Workers panel (CRUD + match preview), task report drill-in (confidence/evidence/verdicts/retry) in the Agent panel; `docs/SETUP.md` |

## 2. Architecture summary

Tauri desktop app: Rust backend (single SQLite `index.db` per workspace, app-wide serialized connection, FK-enforced, journaled) + Next.js/Tailwind frontend over typed `invoke` bindings. Safety invariants held since Phase 5/7 and verified byte-identical through all ten sprints: executor snapshot/apply/verify/rollback, LLM planner, workspace containment on every write path, local-only git operations (never pushes). The governed pipeline shape — requirement → task → execute → evidence → promotion, human approval on every execution — is unchanged since its introduction; every later layer wraps rather than modifies it.

## 3. Validation results (Sprint 10)

**Task 1 — Clean install: PASS.** Fresh `git clone` of `49cb2a6` into a scratch directory → `npm install` → `next build` (compiled) → cold `cargo test`: **154/0/7**. One benign, environment-specific npm `allow-scripts` notice for `sharp` (local npm policy, not a product or docs gap). `docs/SETUP.md` required no corrections.

**Task 2 — Fresh database: PASS.** New workspace: schema creates cleanly, all 7 governance tables empty, empty chain verifies, and the first-ever requirement→task→execution→evidence→promotion cycle succeeds with a real file change, PROMOTED verdict, full correlation chain, and valid hash chain. (Test: `release_fresh_database_first_pipeline_cycle_from_zero`, default suite.)

**Task 3 — Existing-database upgrade: PASS.** A database aged through every layer (requirement version bump, completed task, real rollback → blocked dependent → successful retry → reopened dependent, worker with derived reliability, +100 ledger entries) closed and reopened **three times** (schema + all migrations re-run each time): row counts identical across all 8 tables every round, chain verifies every round, and every read API works over the aged rows — task report (retry lineage visible, record complete), DAG reload + runnable set (reopened dependent still runnable), capability matching with the preserved derived score, correlation queries. (Test: `release_aged_database_survives_reopen_and_all_read_apis_work`, default suite.)

**Task 4 — Long-running agent: PASS.** 60 sequential full pipeline cycles (54 markdown + 6 real cargo-check cycles) in one connection: every cycle COMPLETED and PROMOTED; chain valid at the end (≥240 events); evidence `insertion_sequence` strictly monotonic across all 60; **no leaked files** (the only new workspace entries are cargo's own `Cargo.lock` + `target/`, created once — verified by name, not just count); no lock errors; **per-cycle timing flat: early average 25ms vs late average 26ms** — no O(n²) degradation. (Test: `release_long_run_60_cycles_stays_correct_and_flat`, `#[ignore]`d from the fast suite; run via `cargo test release_validation -- --ignored`.)

**Task 5 — Repeated north-star: PASS.** The Sprint 6 scenario executed 5 consecutive times in fresh workspaces with real executors and a real `cargo test` per run: 5/5 runs produced the **identical 9-event sequence** (`requirement_created, task_planned ×2, promotion_requested, promotion_approved, task_completed` ×2 nodes), valid chain every run, zero flakes — no survivors of the same-second-timestamp bug class. (Test: `release_north_star_is_deterministic_across_5_runs`, `#[ignore]`d; same explicit invocation.)

**One finding during validation (test-side, not product):** the long-run test's initial file-count assertion misattributed cargo's one-time build artifacts as a leak; corrected to assert by entry name so real per-cycle leaks are still caught. No production behavior was involved.

## 4. Test results

- Rust suite (validated tree): **156 passed / 0 failed / 9 ignored** — the 154-test v1.1 baseline plus the 2 new fast release tests; the 9 ignored are 7 live-Ollama-gated tests plus the 2 explicit release scenarios, both of which pass when run as documented.
- Frontend: `tsc --noEmit` exit 0; `next build` compiled with all pages.
- Frozen-architecture status at sprint end: the only `src-tauri/` diff is **2 lines in `lib.rs`, both under `#[cfg(test)]`** (registering the release-validation test module — the mechanism you pre-approved), plus the new test-only `release_validation.rs`. Zero production Rust lines changed; executor, planner, ledger hashing, evidence semantics, promotion gate, requirement intelligence, worker matching, reliability derivation all untouched.
- The 7 Ollama-gated tests were not run (no live Ollama instance assumed during validation); they remain environment-gated as since Phase 1–7. Running them once against a live instance is recommended before public release.

## 5. Known limitations (standing, all deliberate deferrals)

1. **Phase 5 promotion is post-hoc** — the controller audits/records the executor's verified outcome rather than gating the write beforehand; the executor's snapshot/rollback is the safety net (architectural note since Sprint 4).
2. **`run_code` tasks carry NULL correlation IDs** (gating deferred at Sprint 2).
3. **Single worker type** — only the Coder executes work; routing/reliability machinery is built but its practical value is capped until more worker types exist. New workers default to optimistic 1.0 reliability.
4. **Retry decisions are on-demand**, not auto-triggered on failure (deliberate autonomy boundary).
5. **`regenerate_history` is O(entries × evidence)** with duplicated evidence blocks; fine at current scale, flagged for cleanup.
6. **Dead `MODEL_FAILED` constant** — the build's only compiler warning.
7. **Hand-written TS types** mirror Rust structs; drift risk until `ts-rs`-style generation is adopted.
8. **No DAG graph visualization**; DAG operations surface as lists/bindings.
9. **`Sprint-Gate-Protocol-v1.0.md` is a placeholder** — the real protocol text has never been supplied by the human operator.
10. Hash chain is tamper-*evidence* for a local single-user app, not a distributed/Byzantine-resistant ledger (documented since Sprint 2).

## 6. Upgrade notes

Databases from any prior phase/sprint upgrade in place: all migrations are `CREATE TABLE IF NOT EXISTS` / additive `ALTER TABLE`, re-run idempotently on every open, proven data-preserving under repetition (Task 3). No action required from users beyond opening their workspace.

## 7. Release recommendation

**RELEASE-READY.** All seven acceptance criteria met: clean-clone build+test from the checkpoint commit; first-cycle success on a fresh workspace; zero-loss upgrade with all read APIs functional over aged data; 60-cycle sustained run with flat timing and full integrity; 5/5 deterministic north-star runs; 156/156 runnable tests green plus frontend gates; frozen production code untouched (test-only additions itemized above). Pre-public-release recommendations, non-blocking: one live-Ollama pass over the 7 gated tests, and one human click-through of the Sprint 9 panels in the real Tauri shell.
