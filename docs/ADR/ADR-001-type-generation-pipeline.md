# ADR-001: Rust ↔ TypeScript Type Generation Pipeline

**Status:** Accepted  
**Date:** 2026-07-10  
**Author:** Cline (AI Agent)  

## Current Problem

NeuralForge previously used manually maintained TypeScript interfaces and Tauri invoke wrappers in `lib/*.ts`. Every backend Rust struct change required a corresponding manual edit across 6 files (`agent.ts`, `ai.ts`, `governance.ts`, `fs.ts`, `bootstrap.ts`, `extensions.ts`).

## Risk

Backend Rust changes could silently drift from frontend expectations. The `NEURALFORGE_AUDIT_REPORT.md` (Sprint 10) flagged this as a **MEDIUM** risk — the only medium-risk item other than the intermittent test failure (now resolved).

## Decision

Introduce Rust-generated TypeScript definitions using **Specta** (`specta` crate v1.0.5 with `export`, `serde`, `typescript` features).

## Current Implementation

Generated types are introduced through:

`lib/generated_types.d.ts`

The generation is driven by adding `#[derive(Type)]` to all exported backend structs (26 Rust source files modified). Each derive is a purely additive annotation — no struct fields or behavior were changed.

## Important

Current hand-written `lib/*.ts` wrappers remain temporarily in place. No invoke wrapper was replaced, renamed, or removed.

## Reason

Avoid unnecessary frontend migration risk during stabilization. The generated types file serves as a **validation source** — a reference that confirms every Rust struct has a corresponding TypeScript type, without touching the production-tested IPC surface.

## Future Direction

Evaluate migration from manual `invoke()` wrappers to fully generated `tauri-specta` bindings as a separate, independently planned effort (see ADR-002).

## Consequences

- **Positive:** Backend type changes that would silent-drift are now caught — any struct change without a corresponding `Type` derive causes a compile error.
- **Positive:** New frontend code can reference `generated_types.d.ts` types for stronger typing during development.
- **Negative:** Two sources of truth for types exist until Phase 2 migration (hand-written + generated). This is temporary and mitigated by the generated file being the authoritative reference.
- **Negative:** Added `specta` as a dependency (9 new crate packages), increasing compile time slightly.

## Change Classification

**Level 3** — Shared system change affecting the Rust ↔ TypeScript contract boundary.