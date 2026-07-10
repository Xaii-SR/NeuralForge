# NeuralForge Project State

This file changes often as work progresses. Permanent operating rules live
in `.clinerules`, not here — do not duplicate rules into this file.

Current repository:

C:\Users\saiah\NeuralForge

Current commit:

22c5230 — "test: Sprint 10 release candidate validation checkpoint"

Current branch:

master

## Architecture

Verified from repository files (`package.json`, `src-tauri/Cargo.toml`) and
`docs/NEURALFORGE-V1.1-RELEASE-REPORT.md` (committed at this checkpoint):

- **Desktop shell:** Tauri 2.11.3
- **Backend:** Rust, edition 2021, rust-version 1.77.2. Modular crate:
  `agent/` (executor, planner, memory), `governance/` (requirements,
  ledger, evidence, promotion, validator), `planning/` (DAG), `intelligence/`
  (worker registry, matcher, reliability/retry layer), `bootstrap/`
  (self-improvement via local git branches, never pushes), `database/`
  (schema + indexer/search/resolver), `ai/` (Ollama routing, cache,
  benchmarks), `extensions/`, `filesystem/`, `terminal/`.
- **Persistence:** single `index.db` per opened workspace (rusqlite,
  bundled SQLite), `PRAGMA foreign_keys = ON` explicit, additive-only
  schema (`CREATE TABLE IF NOT EXISTS` / non-fatal `ALTER TABLE ADD
  COLUMN`), multi-statement governance writes wrapped in transactions.
- **Frontend:** Next.js 16.2.10, React 19, TypeScript 5.5, Tailwind,
  Monaco editor, xterm terminal. Panel-based UI (`components/*Panel.tsx`)
  driven from `app/page.tsx`; typed `invoke()` bindings in `lib/*.ts`.
- **Governed pipeline (core safety property):** requirement → task →
  execute → evidence → promotion, with human approval required before
  any execution. This shape has been unchanged since its introduction.

## Completed Systems

From repository evidence (`docs/NEURALFORGE-V1.1-RELEASE-REPORT.md` §1,
cross-checked against `src-tauri/src/` module presence and a fresh
`cargo test` run at this checkpoint):

- Requirement Intelligence (validated requirement contracts gate all
  file-edit work; versioned with history)
- Traceability Ledger + Evidence (SHA-256 hash-chained, `verify_chain()`
  tamper detection; deterministic evidence ordering)
- Task DAG Planning (cycle/orphan detection, topological execution,
  failure-blocks-dependents-not-siblings)
- Promotion Governance (single PromotionController for both the agent
  flow and self-bootstrap flow)
- Worker Intelligence (capability matching, reliability derived from
  real promotion verdicts)
- Autonomous Reliability Layer (failure classification, bounded
  human-gated retry with lineage, DAG retry-recovery, confidence
  scoring, evidence-completeness checks, structured task reports)
- UX/Developer Interface (Governance panel, Workers panel, task report
  drill-in, typed bindings for all backend commands, `docs/SETUP.md`)
- Production hardening (transaction boundaries, migration-safety tests,
  panic-safe read paths, explicit FK enforcement)

All of the above validated by real-executor / real-`cargo check` tests
(no mocks) per the Sprint 10 release report methodology.

## Missing Systems

Discovered gaps relative to a Cursor-level target (repository evidence,
not aspiration):

- Only one worker/agent type (Coder) exists; capability-routing
  machinery is built but has nothing else to route to.
- No DAG graph visualization — DAG state surfaces as lists/bindings only.
- No automatic retry-on-failure — retry is on-demand by design.
- No true multi-file simultaneous diff review UX (DAGs decompose into
  sequential single-file tasks).
- Chat-time automatic relevant-file context injection is partial
  (search/resolver exist; not proven as an automatic chat-time feature).
- `docs/governance/Sprint-Gate-Protocol-v1.0.md` remains an explicit
  placeholder — real content has not been supplied by the human operator.

## Current Issues

- **Failing tests:** none. Fresh `cargo test` at this checkpoint:
  **156 passed / 0 failed / 9 ignored** (7 live-Ollama-gated + 2
  explicit release-validation scenarios, both pass when run as
  documented in `docs/SETUP.md`).
- **Warnings:** one dead-code warning, `MODEL_FAILED` constant in
  `src-tauri/src/core/events.rs` (unused).
- **Open investigation (resolved):**
  - **Issue:** Intermittent Rust test failure during parallel `cargo test` execution.
  - **Affected tests:** `broken_rust_change_is_automatically_rolled_back`,
    `apply_to_non_rust_file_succeeds_without_verification`
  - **Root Cause:** Windows temporary directory collision caused by
    timestamp-only directory naming using `SystemTime::now().as_nanos()`.
  - **Resolution:** Added `std::thread::current().id()` to temp workspace
    directory generation to guarantee unique temporary paths across
    concurrent test threads.
  - **Classification:** Level 1 — test infrastructure hardening.
  - **Production Impact:** None.
  - **Validation:** `cargo test` 156/156 pass; executor parallel stress
    50/50 passed.
- **Uncommitted, standing exclusions** (known, not blockers): `blueprint.md`
  (stray edit), `next-env.d.ts` (Next.js-generated, regenerates on
  build), `OPENHANDS_HANDOFF.md` (pre-existing handoff note unrelated
  to this workstream).

## Next Recommended Actions

Based on the repository audit at this checkpoint, in priority order:

1. Resolve `docs/governance/Sprint-Gate-Protocol-v1.0.md` with real
   human-supplied content (it must not be AI-generated, per the file's
   own placeholder text).
2. Close cheap technical debt: generated TypeScript types (removes the
   hand-written-binding drift risk), remove the dead `MODEL_FAILED`
   constant.
3. Add a second worker/agent type so the existing Sprint 5 matching and
   reliability machinery becomes load-bearing.
4. Scope Cursor-parity features (multi-file diff review, DAG
   visualization, automatic chat context injection) as individually
   planned Level 3/4 changes per `.clinerules` — not a single large
   rewrite.
