# Event Sourcing

## What This Is

An unopinionated Rust event sourcing library built around a constrained event log and a projection engine. Instead of aggregates, it uses dynamic consistency boundaries with read models, optimistic concurrency, and sequence-ID-based consistency guarantees. Designed as a workspace of crates — core library, storage trait implementations, and an optional command layer.

## Core Value

The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

## Requirements

### Validated

- [x] Cargo workspace with core crate and placeholder sibling crates — Validated in Phase 01: workspace-setup-and-core-types
- [x] Core types: StreamId (validated), GlobalSequenceId, StreamSequenceId, StoredEvent, NewEvent — Validated in Phase 01: workspace-setup-and-core-types
- [x] Typed error enums: AppendError (ConcurrencyConflict, StorageFailure), StoreError — Validated in Phase 01: workspace-setup-and-core-types
- [x] LogStore trait with 5 async methods and associated EventStream type — Validated in Phase 01: workspace-setup-and-core-types
- [x] In-memory log store (separate crate) — Validated in Phase 02: in-memory-log-store
- [x] Trait-based storage abstraction (user-implementable for any database) — Validated in Phase 02: in-memory-log-store
- [x] Declarative projection definitions that serialize to JSON (`ProjectionDefinition`) — Validated in Phase 04: projection-engine-single-stream
- [x] Projections support nested objects in output shape — Validated in Phase 04: projection-engine-single-stream
- [x] `projection!` DSL macro generates typed read model structs and `ReadModel` impls — Validated in Phase 04: projection-engine-single-stream
- [x] `#[derive(Event)]` macro generates `Event` trait impl with schema from struct field types — Validated in Phase 04: projection-engine-single-stream
- [x] Schema conflict detection at startup via `EventLog::validate_schemas` — Validated in Phase 04: projection-engine-single-stream

### Active

- [ ] Event log with append that validates constraints before persisting
- [ ] Optimistic concurrency on event streams
- [ ] Sequence ID returned from append, usable for consistent reads
- [ ] Observer trait for reacting to appended events (with retry/success/fail result type)
- [ ] Projection observer — a built-in observer that saves read models to the database
- [ ] Projections support joining multiple streams
- [ ] Projections support list fields in output shape (keyed collections with mutation/removal)
- [ ] Constraints/invariants attached to event streams, validated via projection engine
- [ ] Multi-stream constraints (constraints backed by multi-stream projections)
- [ ] Optional inline catch-up on reads — if a projection is behind the requested sequence ID, update before returning
- [ ] SQLite log store (separate crate)

### Out of Scope

- Runtime query language for projections — future work, JSON projection definitions lay the groundwork
- Views (persistent runtime projections) — deferred until query language exists
- Authentication/authorization — not the library's concern
- Networking/transport — this is a library, not a framework
- Specific domain modeling patterns — the library is unopinionated

## Current State

Phase 04 complete — `ProjectionEngine` implemented with full single-stream fold semantics, JSON-serializable `ProjectionDefinition`, builder API, `projection!` DSL macro, `#[derive(Event)]` macro, schema conflict detection, and 139 workspace tests passing. Next: Phase 4.1 (list fields) then Phase 5 (multi-stream joins).

## Context

- Rust workspace with multiple crates
- Crate structure:
  - `event-sourcing` — core: event log, observers, projections, constraints, traits
  - `event-sourcing-logstore-inmemory` — in-memory storage implementation
  - `event-sourcing-logstore-sqlite` — SQLite storage implementation
  - `event-sourcing-commands` — optional command pattern layer (separate crate)
  - `event-sourcing-macros` — proc-macro crate with `#[derive(Event)]` and `projection!` DSL
- No aggregates by design — consistency boundaries are dynamic, not tied to fixed types
- Constraints are global to an event stream — they apply to all appends, not per-command
- The projection engine is used both for building read models and for validating constraints
- `ProjectionDefinition` is a JSON-serializable struct; the `projection!` macro is the primary authoring surface, builder API is the secondary
- `HandlerSpec` uses untagged serde with per-variant inner structs — critical for correct single-key JSON format
- `FieldSpec::Object` must be tried before `FieldSpec::Scalar` in untagged deserialization (Object has a required `"type"` discriminator)
- Observer results use a three-state return type: Ok, Retry, Failed

## Constraints

- **Language**: Rust — library crate, not a binary
- **Type Safety**: Strong compile-time type safety for events, projections, and constraints
- **Unopinionated**: No forced patterns — users compose building blocks as they see fit
- **Crate Separation**: Storage implementations and commands are separate crates, not features

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| No aggregates, read models only | Dynamic consistency boundaries are more flexible than fixed aggregate roots | — Pending |
| Constraints validated via projection engine | Single engine for both read models and invariant checks, avoids two systems | — Pending |
| Multi-stream constraints in v1 | If projection engine handles joins, multi-stream constraints come naturally | — Pending |
| Commands as separate crate | Core stays minimal and unopinionated; commands are one pattern on top | — Pending |
| Projection definitions serialize to JSON | Enables future query language and runtime projection definitions | — Validated Phase 04 |
| Storage trait, not concrete implementations | Users can implement for any database; inmemory and sqlite ship as separate crates | — Pending |
| Inline catch-up on reads is optional | Caller decides if they need consistency guarantee per query | — Pending |
| `projection!` macro as primary DSL; builder as secondary | Macro gives ergonomic single-source-of-truth; builder exposed for programmatic construction | — Validated Phase 04 |
| proc-macro crate independent of core crate | Avoids circular dependency; generated tokens reference `event_sourcing::` paths at call site | — Validated Phase 04 |
| HandlerSpec untagged serde with per-variant inner structs | Struct variants produce nested JSON (`{"from":{"from":"$.x"}}`); untagged inner structs produce single-key objects (`{"from":"$.x"}`) | — Validated Phase 04 |
| FieldSpec tries Object before Scalar in untagged deserialization | ObjectFieldSpec has required `"type"` discriminator; trying Scalar first would match Object JSON incorrectly | — Validated Phase 04 |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd:transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-24 after Phase 04 completion*
