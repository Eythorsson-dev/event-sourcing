---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: Phase 4.2 context gathered — absorbed into Phase 5 + Phase 5.1
last_updated: "2026-04-26T00:00:00.000Z"
last_activity: 2026-04-26
progress:
  total_phases: 16
  completed_phases: 6
  total_plans: 16
  completed_plans: 9
  percent: 35
---

# Project State: Event Sourcing

**Last updated:** 2026-04-26
**Last activity:** 2026-04-26

---

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-24)

**Core Value:** The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

**Current Focus:** Phase 5 — Multi-Stream Joins + Nested Objects

---

## Current Position

| Field | Value |
|-------|-------|
| Phase | 5 |
| Phase Name | Projection Engine — Multi-Stream and Catch-Up |
| Plan | Not started |
| Status | Ready to discuss (JSON join representation must be discussed first) |
| Milestone | v1 |

**Progress:**

Phase: 5
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

- **Phase 5 (Multi-Stream Joins):** JSON representation of join mechanism in `ProjectionDefinition` must be discussed before planning. Start Phase 5 discuss-phase with this as the first agenda item. Key design decisions captured in `.planning/phases/04.2-projection-engine-list-fields-inserted/04.2-CONTEXT.md`.
- **Phase 7 (Observer Infrastructure):** The retry/backoff/dead-letter concurrency model (per-observer async task vs sequential dispatch vs channel-based queue) has multiple valid patterns. Flag for deliberate design during Phase 7 planning.

### Roadmap Evolution

- Phase 4.1 inserted: projection! macro syntax revision (cleaner DSL — no inner keyword, commas, unified `:` for objects, compile-time cleared_by enforcement)
- Phase 4.2 inserted then absorbed: list fields design discussion — scope merged into Phase 5 (joins) and Phase 5.1 (list fields)
- Phase 5 goal expanded: now includes joins at root + nested object level, join resolution via tags, `removed_by` rename
- Phase 5.1 inserted: List fields — keyed collections, `field[]:` DSL, tag-based keys, `removed_by` removal
- Phase 5.2 inserted: ESQL macro refactor (was Phase 5.1, moved after list fields per 2026-04-26 session)
- Phase 10 added: Examples directory
- Phase 11 added: Update readme
- Phase 12 added: Getting started docs + deploy to crates.io
- Phase 13 added: GDPR and value obfuscation
- Phase 15 added: Projection usage patterns and async trigger design — lazy/on-query vs event bus vs processor

### Blockers

None

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260406-9cg | Fix GitHub Actions pipeline to run tests | 2026-04-06 | bbf0c93 | [260406-9cg-fix-github-actions-pipeline-to-run-tests](./quick/260406-9cg-fix-github-actions-pipeline-to-run-tests/) |

---

## Session Continuity

Last session: 2026-04-26
Stopped at: Phase 4.2 context discussion complete — scope absorbed, roadmap restructured. Ready to discuss Phase 5 (JSON join representation is the mandatory first topic).
Resume file: .planning/phases/04.2-projection-engine-list-fields-inserted/04.2-CONTEXT.md

### Context Summary

Rust event sourcing library built around the Dynamic Consistency Boundary pattern. Workspace of 5 crates: `event-sourcing` (core), `event-sourcing-logstore-inmemory`, `event-sourcing-logstore-sqlite`, `event-sourcing-commands`, `event-sourcing-macros`. Phases 1–4.1 complete: `ProjectionEngine` with full single-stream fold (scalar, object, cleared_by, increment/decrement handlers), `projection!` DSL macro (Phase 4.1 syntax), `#[derive(Event)]`, 139 tests passing. Phase 4.2 was a design discussion — resolved list field and join design decisions, restructured roadmap: Phase 5 (multi-stream joins + nested objects), Phase 5.1 (list fields), Phase 5.2 (ESQL). Next: discuss Phase 5, starting with JSON join representation in `ProjectionDefinition`.

---
*State initialized: 2026-04-04*
