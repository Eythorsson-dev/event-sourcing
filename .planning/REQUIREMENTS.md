# Requirements: Event Sourcing

**Defined:** 2026-04-04
**Core Value:** The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Event Log

- [ ] **LOG-01**: Events are immutable once appended to the log
- [ ] **LOG-02**: Each stream maintains its own sequence numbers for ordering
- [ ] **LOG-03**: Global sequence ID across all streams, monotonically increasing, assigned at append time
- [ ] **LOG-04**: Append rejects with concurrency conflict error if stream has advanced past expected version
- [ ] **LOG-05**: Read events by stream ID — full stream or range by sequence number
- [ ] **LOG-06**: Successful append returns the new sequence ID for use in consistent reads
- [ ] **LOG-07**: Errors distinguish concurrency conflict from storage failure (typed error enum)
- [ ] **LOG-08**: Optional inline catch-up on reads — if projection is behind the requested sequence ID, update inline before returning

### Projection Engine

- [ ] **PROJ-01**: Single-stream projection via left-fold over ordered events
- [ ] **PROJ-02**: Multi-stream projections joining events from multiple streams
- [ ] **PROJ-03**: Projections support nested lists and objects in output shape
- [ ] **PROJ-04**: `ProjectionDefinition` struct serializes to/from JSON
- [ ] **PROJ-05**: Single projection engine used for both read models and constraint validation
- [ ] **PROJ-06**: Builder API for constructing `ProjectionDefinition` programmatically (internal engine, used by macro and directly by advanced users)
- [ ] **PROJ-07**: `projection!` DSL macro that generates read model struct, `Projection` trait impl, and `ProjectionDefinition` — the primary user-facing interface
- [ ] **PROJ-08**: Proc-macro crate (`event-sourcing-macros`) with human-readable compile errors for the DSL

### Constraints

- [ ] **CONS-01**: Single-stream constraints validated via projection engine before append
- [ ] **CONS-02**: Multi-stream constraints — invariants spanning multiple event streams
- [ ] **CONS-03**: Constraints are attached to event streams and apply to all appends on that stream

### Observers

- [ ] **OBSV-01**: Observer trait for reacting to appended events
- [ ] **OBSV-02**: Three-state observer result type: Ok, Retry(reason), Failed(error)
- [ ] **OBSV-03**: Projection observer — built-in observer that materializes read models to storage
- [ ] **OBSV-04**: Retry mechanism with exponential backoff, max retry count, and dead-letter handling

### Storage

- [ ] **STOR-01**: `LogStore` trait abstraction — user-implementable for any database
- [x] **STOR-02**: In-memory log store as separate crate (`event-sourcing-logstore-inmemory`)
- [ ] **STOR-03**: SQLite log store as separate crate (`event-sourcing-logstore-sqlite`)

### Commands

- [ ] **CMD-01**: Command pattern as separate optional crate (`event-sourcing-commands`)

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Projection Macros

(Moved to v1 — PROJ-07, PROJ-08)

### Query Language

- **QUERY-01**: Runtime query language for defining projections without Rust code
- **QUERY-02**: Views — persistent runtime projections defined via query language

### Event Evolution

- **EVOL-01**: Event upcasting / schema versioning patterns
- **EVOL-02**: Copy-transform migration strategy support

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Aggregate root abstraction | Library is aggregate-free by design; uses dynamic consistency boundaries |
| Sagas / process managers | Application-level orchestration, not library infrastructure |
| Built-in message bus | Crosses library/framework boundary; observer trait is the hook |
| Networking / transport | This is a library, not a framework |
| Authentication / authorization | Application-specific, not the library's concern |
| Distributed cluster / replication | Database concern, not library concern |
| Schema migration tooling | Depends on storage backend; users handle with their DB tools |
| GDPR / encryption helpers | Orthogonal; can be a separate crate later |
| Aggregate snapshotting | No aggregates to snapshot; users can cache materialized state |
| Test DSL / given-when-then | Not the core library's job; document testing patterns instead |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| LOG-01 | Phase 1 | Pending |
| LOG-02 | Phase 1 | Pending |
| LOG-03 | Phase 1 | Pending |
| LOG-04 | Phase 3 | Pending |
| LOG-05 | Phase 3 | Pending |
| LOG-06 | Phase 3 | Pending |
| LOG-07 | Phase 1 | Pending |
| LOG-08 | Phase 5 | Pending |
| PROJ-01 | Phase 4 | Pending |
| PROJ-02 | Phase 5 | Pending |
| PROJ-03 | Phase 4 | Pending |
| PROJ-04 | Phase 4 | Pending |
| PROJ-05 | Phase 5 | Pending |
| PROJ-06 | Phase 4 | Pending |
| PROJ-07 | Phase 4 | Pending |
| PROJ-08 | Phase 4 | Pending |
| CONS-01 | Phase 6 | Pending |
| CONS-02 | Phase 6 | Pending |
| CONS-03 | Phase 6 | Pending |
| OBSV-01 | Phase 7 | Pending |
| OBSV-02 | Phase 7 | Pending |
| OBSV-03 | Phase 7 | Pending |
| OBSV-04 | Phase 7 | Pending |
| STOR-01 | Phase 1 | Pending |
| STOR-02 | Phase 2 | Complete |
| STOR-03 | Phase 8 | Pending |
| CMD-01 | Phase 9 | Pending |

**Coverage:**
- v1 requirements: 27 total
- Mapped to phases: 27
- Unmapped: 0

---
*Requirements defined: 2026-04-04*
*Last updated: 2026-04-04 after roadmap creation — all 25 requirements mapped*
