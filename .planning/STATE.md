---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: ready_to_plan
last_updated: "2026-04-24T18:30:30.549Z"
last_activity: 2026-04-24
progress:
  total_phases: 17
  completed_phases: 6
  total_plans: 14
  completed_plans: 9
  percent: 35
---

# Project State: Event Sourcing

**Last updated:** 2026-04-24
**Last activity:** 2026-04-24

---

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-24)

**Core Value:** The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

**Current Focus:** Phase 4.1 — Projection Engine — List Fields

---

## Current Position

| Field | Value |
|-------|-------|
| Phase | 4.1 |
| Phase Name | Projection Engine — List Fields |
| Plan | Not started |
| Status | Ready to plan |
| Milestone | v1 |

**Progress:**

[####################] 100%
Phase: 4.1
Plan: Not started

---

## Accumulated Context

### Key Decisions

- **Async trait pattern:** Use `async-trait` crate from day one for `dyn LogStore` compatibility; native `async fn in trait` (stable since Rust 1.75) cannot be used as `dyn Trait` yet
- **Global sequence ID:** Must be in the event model from day one — retroactively adding it breaks multi-stream ordering determinism
- **Optimistic concurrency placement:** Atomic compare-and-swap at storage layer, not read-then-write in application code — cannot be retrofitted
- **rusqlite vs sqlx:** rusqlite only for logstore-sqlite — both link libsqlite3-sys and cannot coexist
- **`projection!` macro is primary DSL; builder is secondary:** Macro gives ergonomic single-source-of-truth; builder exposed for programmatic construction — validated Phase 04
- **proc-macro crate independent of core crate:** Avoids circular dep; generated tokens reference `event_sourcing::` paths at call site — validated Phase 04
- **HandlerSpec untagged serde with per-variant inner structs:** Struct variants produce nested JSON; untagged inner structs produce single-key objects (e.g. `{"from":"$.x"}`) — validated Phase 04
- **FieldSpec tries Object before Scalar:** ObjectFieldSpec has required `"type"` discriminator; trying Scalar first would match Object JSON incorrectly — validated Phase 04

### Research Flags (for planning phases)

- **Phase 4.1 (List Fields):** Open questions on list keys (OQ-DSL-02), mutation model (OQ-DSL-03), and removal semantics (OQ-DSL-01) must be resolved during discussion before success criteria can be defined.
- **Phase 7 (Observer Infrastructure):** The retry/backoff/dead-letter concurrency model (per-observer async task vs sequential dispatch vs channel-based queue) has multiple valid patterns. Flag for deliberate design during Phase 7 planning.

### Roadmap Evolution

- Phase 4.1 inserted: projection! macro syntax revision (cleaner DSL — no inner keyword, commas, unified `:` for objects, compile-time cleared_by enforcement)
- Phase 4.2 inserted: List fields in projections (keyed collections with mutation/removal) — was 4.1
- Phase 5 dependency updated: now depends on Phase 4.2
- Phase 10 added: Examples directory
- Phase 11 added: Update readme
- Phase 12 added: Getting started docs + deploy to crates.io
- Phase 13 added: GDPR and value obfuscation

### Blockers

None

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260406-9cg | Fix GitHub Actions pipeline to run tests | 2026-04-06 | bbf0c93 | [260406-9cg-fix-github-actions-pipeline-to-run-tests](./quick/260406-9cg-fix-github-actions-pipeline-to-run-tests/) |

---

## Session Continuity

Last session: 2026-04-24
Stopped at: Phase 04 complete (6/6 plans, 139 tests passing, UAT 8/8 passed, security verified), ready to discuss Phase 4.1
Resume file: None

### Context Summary

Rust event sourcing library built around the Dynamic Consistency Boundary pattern. Workspace of 5 crates: `event-sourcing` (core), `event-sourcing-logstore-inmemory`, `event-sourcing-logstore-sqlite`, `event-sourcing-commands`, `event-sourcing-macros`. Phase 4 complete: `ProjectionEngine` with full single-stream fold (scalar, object, cleared_by, increment/decrement handlers), JSON-serializable `ProjectionDefinition`, builder API, `projection!` DSL macro, `#[derive(Event)]` proc macro, schema conflict detection via `validate_schemas`. 139 workspace tests passing. Next: Phase 4.1 (list fields with keyed collections) requires discussion before planning.

---
*State initialized: 2026-04-04*
