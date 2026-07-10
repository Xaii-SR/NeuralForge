# ADR-002: Generated Bindings Migration Strategy

**Status:** Proposed  
**Date:** 2026-07-10  
**Author:** Cline (AI Agent)  

## Decision

Do **NOT** immediately remove existing `lib/*.ts` wrappers.

## Reason

The current system is production validated. Every `lib/*.ts` wrapper has been tested end-to-end through real Tauri IPC calls. Replacing all wrappers simultaneously introduces unnecessary risk:

- `invoke()` calls rely on exact `snake_case` parameter names
- Tauri v2 `rename_all` behavior must be confirmed for specta-generated bindings
- Frontend components depend on the current import API surface — changing signatures would ripple across `app/page.tsx`, `components/*Panel.tsx`, and `hooks/*.ts`

## Migration Strategy

### Phase 1: Generated Types as Validation Source (COMPLETE)

- `#[derive(Type)]` added to all exported backend structs
- `lib/generated_types.d.ts` created as parallel type reference
- Hand-written wrappers unchanged
- Validation: `cargo check`, `cargo test`, `tsc --noEmit`

### Phase 2: Convert Low-Risk Wrappers Individually

- Convert one `lib/*.ts` file at a time
- Start with simplest files (`lib/fs.ts` — 1 interface, 7 invoke calls)
- Each conversion produces a separate commit
- Validate after each conversion: `cargo test`, `tsc --noEmit`, `next build`

### Phase 3: Remove Duplicated Interfaces

- Once all invoke wrappers are specta-generated, delete hand-written `export interface` blocks from `lib/*.ts`
- Re-export generated types through a clean `lib/index.ts` API surface
- Validate same gates

### Phase 4: Fully Adopt Generated Command Bindings

- Remove all hand-written `invoke()` import wrappers
- Frontend imports come exclusively from generated bindings
- Single source of truth: Rust struct ↔ TypeScript type

## Requirements

Every migration step requires:

- `cargo test`
- `cargo check`
- `tsc --noEmit`
- `next build`

## Status

Proposed — pending approval to begin Phase 2.