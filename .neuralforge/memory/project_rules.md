# Project Rules

- Follow `blueprint.md` as the single source of truth. Phases are built and
  gated one at a time — build, test, runtime-check, update docs, commit —
  never skip a gate, never build ahead of the current phase.
- Commit after each completed component, not just at phase boundaries.
- New dependencies: prefer official/mature crates, note why in `decisions.md`
  when the choice isn't obvious.
