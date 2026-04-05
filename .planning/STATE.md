# Project State: Event Sourcing

**Last updated:** 2026-04-05
**Updated by:** planner (Phase 1 plan created)

---

## Project Reference

**Core Value:** The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

**Current Focus:** Phase 1 — Workspace Setup and Core Types

---

## Current Position

| Field | Value |
|-------|-------|
| Phase | 1 |
| Phase Name | Workspace Setup and Core Types |
| Plan | 01-PLAN.md (8 nodes) |
| Status | Planned |
| Milestone | v1 |

**Progress:**
```
Phase:  [1  ] [2  ] [3  ] [4  ] [5  ] [6  ] [7  ] [8  ] [9  ]
        [----] [----] [----] [----] [----] [----] [----] [----] [----]
        0%                                                       100%
```

---

## Performance Metrics

| Metric | Value |
|--------|-------|
| Phases total | 9 |
| Phases complete | 0 |
| Plans complete | 1 |
| Requirements mapped | 25/25 |

---

## Accumulated Context

### Key Decisions

- **Async trait pattern:** Use `async-trait` crate from day one for `dyn LogStore` compatibility; native `async fn in trait` (stable since Rust 1.75) cannot be used as `dyn Trait` yet
- **Global sequence ID:** Must be in the event model from day one — retroactively adding it breaks multi-stream ordering determinism
- **Optimistic concurrency placement:** Atomic compare-and-swap at storage layer, not read-then-write in application code — cannot be retrofitted
- **LogStore interface width:** Keep to 5-6 methods (append, read_stream, read_all, current_sequence) — all orchestration in EventLog, not in the trait
- **ProjectionDefinition operation vocabulary:** Fixed set of operations to be defined during Phase 4 planning; scope creep risk flagged by research
- **rusqlite vs sqlx:** rusqlite only for logstore-sqlite — both link libsqlite3-sys and cannot coexist
- **Builder API first, proc macro deferred:** ProjectionDefinition via builder API in v1; proc macro derive deferred to v2 (MACRO-01, MACRO-02)

### Research Flags (for planning phases)

- **Phase 4 (Projection Engine — Single-Stream):** The JSON-serializable `ProjectionDefinition` operation vocabulary is novel in the Rust ecosystem. Needs a design spike during planning — no reference implementation to copy.
- **Phase 7 (Observer Infrastructure):** The retry/backoff/dead-letter concurrency model (per-observer async task vs sequential dispatch vs channel-based queue) has multiple valid patterns. Flag for deliberate design during Phase 7 planning.

### Active Todos

- [x] Plan Phase 1
- [ ] Execute Phase 1

### Blockers

None

---

## Session Continuity

### How to Resume

1. Read `ROADMAP.md` for phase structure and success criteria
2. Read this file for current position and accumulated decisions
3. Run `/gsd:execute-phase` to begin implementation

### Context Summary

Rust event sourcing library built around the Dynamic Consistency Boundary pattern. Workspace of 4 crates: `event-sourcing` (core), `event-sourcing-logstore-inmemory`, `event-sourcing-logstore-sqlite`, `event-sourcing-commands`. No aggregates by design — consistency boundaries are dynamic, backed by the projection engine. The `ProjectionDefinition` struct is the lingua franca: serde-serializable, used for both read models and constraint validation. Stack: tokio 1.x, serde/serde_json, rusqlite 0.38 (bundled), tokio-rusqlite 0.5, async-trait 0.1, thiserror 2.0, uuid 1.x (v7). Research confidence: HIGH overall.

---
*State initialized: 2026-04-04*
