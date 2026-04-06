# Roadmap: Event Sourcing

**Milestone:** v1
**Granularity:** Fine
**Coverage:** 27/27 requirements mapped
**Created:** 2026-04-04

---

## Phases

- [ ] **Phase 1: Workspace Setup and Core Types** - Cargo workspace scaffold, core type definitions, and the LogStore trait abstraction
- [ ] **Phase 2: In-Memory Log Store** - Working in-memory LogStore implementation that serves as test harness for all subsequent phases
- [ ] **Phase 3: Event Log and Optimistic Concurrency** - EventLog orchestrator with append cycle, per-stream sequencing, and atomic optimistic concurrency
- [ ] **Phase 4: Projection Engine — Single-Stream** - Single-stream projection fold, JSON-serializable ProjectionDefinition, and builder API
- [ ] **Phase 5: Projection Engine — Multi-Stream and Catch-Up** - Multi-stream joins, unified engine for read models and constraints, and inline catch-up reads
- [ ] **Phase 6: Constraint Validation** - Constraint types wired into the append cycle, single-stream and multi-stream invariants
- [ ] **Phase 7: Observer Infrastructure** - Observer trait, three-state result, retry/backoff registry, and built-in ProjectionObserver
- [ ] **Phase 8: SQLite Log Store** - Production-grade SQLite LogStore implementation with WAL mode and schema migrations
- [ ] **Phase 9: Commands Crate** - Optional command pattern layer as a separate crate demonstrating the command→projection→append cycle

---

## Phase Details

### Phase 1: Workspace Setup and Core Types
**Goal**: A caller can define typed events and ask the library's types what stream they belong to, what sequence they are at, and what went wrong — without touching any I/O
**Depends on**: Nothing (first phase)
**Requirements**: STOR-01, LOG-01, LOG-02, LOG-03, LOG-07
**Success Criteria** (what must be TRUE):
  1. Cargo workspace compiles with `event-sourcing` core crate and placeholder sibling crates
  2. A `StoredEvent` value carries a global sequence ID, a stream-scoped sequence number, and typed payload without any I/O
  3. `AppendError` distinguishes `ConcurrencyConflict` from `StorageFailure` via a typed enum that a caller can match on
  4. The `LogStore` trait is declared with `async-trait`, compiles against `dyn LogStore`, and has no domain logic in its method signatures
  5. In-memory unit tests can import core types from `event-sourcing` and the blank storage trait from the same crate
**Plans:** 2 plans

Plans:
- [x] 01-01-PLAN.md — Cargo workspace scaffold with 4 crates and centralized dependency management
- [x] 01-02-PLAN.md — Core types (StreamId, sequences, StoredEvent, errors) and LogStore trait

### Phase 2: In-Memory Log Store
**Goal**: A caller can append events to named streams and read them back in order using an in-memory store — providing the test harness all future phases depend on
**Depends on**: Phase 1
**Requirements**: STOR-02
**Success Criteria** (what must be TRUE):
  1. `InMemoryLogStore` implements `LogStore` and compiles as a separate crate (`event-sourcing-logstore-inmemory`)
  2. A caller can append events to a stream and read them back in insertion order
  3. A caller can read a stream by range (from sequence N to M) and receive only the requested slice
  4. The global sequence ID is monotonically increasing across streams — two appends to different streams return different, ordered IDs
  5. A caller using `InMemoryLogStore` in a test can pass it as `dyn LogStore` without unsafe code
**Plans:** 1 plan

Plans:
- [x] 02-01-PLAN.md — InMemoryLogStore implementation with full LogStore trait coverage and comprehensive tests

### Phase 3: Event Log and Optimistic Concurrency
**Goal**: A caller can append to an event stream with an expected version and receive a conflict error if a concurrent writer advanced the stream first — atomically, with no race window
**Depends on**: Phase 2
**Requirements**: LOG-04, LOG-05, LOG-06
**Success Criteria** (what must be TRUE):
  1. `EventLog::append` accepts an `AppendCondition` specifying the expected stream version and rejects with `ConcurrencyConflict` if the stream has advanced
  2. A successful append returns the new global sequence ID to the caller
  3. Two concurrent appends to the same stream with the same expected version result in exactly one success and one `ConcurrencyConflict` — the race window is at the storage layer, not application code
  4. A caller can read all events for a stream by stream ID, receiving them in stream-sequence order
  5. A caller can read a partial stream range by supplying start and end sequence numbers
**Plans:** 1 plan

Plans:
- [ ] 03-01-PLAN.md — EventLog<S> orchestrator with EventLogError, append/read delegation, and comprehensive tests

### Phase 03.1: DCB Model Revision — StoredEvent, StreamId, and Sequence ID Accuracy (INSERTED)

**Goal:** [Urgent work - to be planned]
**Requirements**: TBD
**Depends on:** Phase 3
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd:plan-phase 03.1 to break down)

### Phase 4: Projection Engine — Single-Stream
**Goal**: A caller can describe a single-stream projection using a DSL macro that generates the read model struct, Projection trait impl, and JSON-serializable definition — then run it over a stored stream and get back a typed result
**Depends on**: Phase 3
**Requirements**: PROJ-01, PROJ-03, PROJ-04, PROJ-06, PROJ-07, PROJ-08
**Success Criteria** (what must be TRUE):
  1. A caller constructs a `ProjectionDefinition` via a builder API without writing raw struct literals
  2. A `ProjectionDefinition` serializes to JSON and deserializes back to an equivalent struct without data loss
  3. `ProjectionEngine::fold` runs a single-stream projection over an ordered event set and returns a typed output value
  4. The output shape supports nested lists and objects — not just flat key-value maps
  5. A `ProjectionDefinition` with an unknown field in its JSON deserializes with a clear error, not silently corrupt state
**Plans**: TBD
**UI hint**: no

### Phase 5: Projection Engine — Multi-Stream and Catch-Up
**Goal**: A caller can join events from multiple streams in a single projection run and read a consistent result gated on a specific global sequence ID — using the same engine as single-stream projections
**Depends on**: Phase 4
**Requirements**: PROJ-02, PROJ-05, LOG-08
**Success Criteria** (what must be TRUE):
  1. A `ProjectionDefinition` can declare multiple source streams and `ProjectionEngine` folds their events in global-sequence order
  2. A multi-stream projection over streams that received interleaved appends produces a deterministic result regardless of which stream was written first
  3. The same `ProjectionEngine` instance handles both read-model queries and constraint checks — there is no second engine for constraints
  4. A caller can request a read gated on a sequence ID; if the projection's checkpoint is behind, the engine catches up inline before returning
  5. A read with catch-up disabled returns immediately with whatever state the projection holds, without blocking
**Plans**: TBD

### Phase 6: Constraint Validation
**Goal**: A caller can attach invariant checks to an event stream so that every append is validated against a projection result before the write commits — including invariants that span multiple streams
**Depends on**: Phase 5
**Requirements**: CONS-01, CONS-02, CONS-03
**Success Criteria** (what must be TRUE):
  1. A `Constraint` bundles a `ProjectionDefinition` with a validation function and can be registered on an event stream
  2. `EventLog::append` runs all registered constraints before the atomic write; a failing constraint aborts the append with a typed error distinct from `ConcurrencyConflict`
  3. Constraints attached to a stream apply to every append on that stream — a caller cannot bypass them by appending directly to the store
  4. A `Constraint` backed by a multi-stream `ProjectionDefinition` is validated the same way as a single-stream constraint — no special-casing
  5. After a constraint check passes, the subsequent write uses the same expected-version value read during constraint evaluation, preventing a race window between check and write
**Plans**: TBD

### Phase 7: Observer Infrastructure
**Goal**: A caller can register observers that react to committed appends, receive a three-state result per observer invocation, and have failing observers retried with backoff before being routed to a dead-letter sink
**Depends on**: Phase 6
**Requirements**: OBSV-01, OBSV-02, OBSV-03, OBSV-04
**Success Criteria** (what must be TRUE):
  1. An `Observer` impl receives every successfully committed event and returns `ObserverResult::Ok`, `Retry(reason)`, or `Failed(error)`
  2. An observer returning `Retry` is re-invoked with exponential backoff; after exceeding the configured max retry count it is routed to the dead-letter handler rather than retried indefinitely
  3. `ProjectionObserver` is a built-in `Observer` that materializes a read model to storage using `ProjectionEngine` — no custom implementation needed for the common case
  4. A read-model query can be issued immediately after an append and, if the `ProjectionObserver` checkpoint is behind, the engine catches up inline before returning (read-your-writes consistency)
  5. Multiple observers registered on the same `EventLog` each receive every event independently; one observer's failure does not suppress delivery to others
**Plans**: TBD

### Phase 8: SQLite Log Store
**Goal**: A caller can swap the in-memory store for a SQLite-backed store and have all existing behavior — including optimistic concurrency and global sequencing — work identically against a durable file
**Depends on**: Phase 7
**Requirements**: STOR-03
**Success Criteria** (what must be TRUE):
  1. `SqliteLogStore` compiles as a separate crate (`event-sourcing-logstore-sqlite`) and implements `LogStore` without changes to the core crate
  2. The SQLite store opens with WAL mode enabled at connection time; a caller does not need to configure it manually
  3. Optimistic concurrency is enforced atomically inside a SQLite transaction — two concurrent appends with the same expected version produce exactly one success and one `ConcurrencyConflict`
  4. A caller can run all integration tests that previously used `InMemoryLogStore` against `SqliteLogStore` by substituting the store — the test logic is unchanged
  5. Schema migrations are handled by `rusqlite_migration`; a fresh database is ready to use immediately without manual setup
**Plans**: TBD

### Phase 9: Commands Crate
**Goal**: A caller can wire up the read-projection-then-append command cycle using a thin `CommandHandler` abstraction from a separate optional crate, without any mandatory coupling to the core library
**Depends on**: Phase 8
**Requirements**: CMD-01
**Success Criteria** (what must be TRUE):
  1. `event-sourcing-commands` compiles as an independent crate that depends on `event-sourcing` core but has no dependency on any storage crate
  2. A caller implements `CommandHandler` to express: load read model via projection, validate business rules, append events — as three distinct steps in a documented pattern
  3. The commands crate does not re-implement constraint validation or observer dispatch — it delegates entirely to `EventLog::append`
  4. A user who does not want the command pattern can use `EventLog` directly without pulling in the commands crate
**Plans**: TBD

---

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Workspace Setup and Core Types | 0/2 | Not started | - |
| 2. In-Memory Log Store | 0/1 | Not started | - |
| 3. Event Log and Optimistic Concurrency | 0/1 | Not started | - |
| 4. Projection Engine — Single-Stream | 0/? | Not started | - |
| 5. Projection Engine — Multi-Stream and Catch-Up | 0/? | Not started | - |
| 6. Constraint Validation | 0/? | Not started | - |
| 7. Observer Infrastructure | 0/? | Not started | - |
| 8. SQLite Log Store | 0/? | Not started | - |
| 9. Commands Crate | 0/? | Not started | - |

---

## Coverage Map

| Requirement | Phase |
|-------------|-------|
| LOG-01 | Phase 1 |
| LOG-02 | Phase 1 |
| LOG-03 | Phase 1 |
| LOG-04 | Phase 3 |
| LOG-05 | Phase 3 |
| LOG-06 | Phase 3 |
| LOG-07 | Phase 1 |
| LOG-08 | Phase 5 |
| PROJ-01 | Phase 4 |
| PROJ-02 | Phase 5 |
| PROJ-03 | Phase 4 |
| PROJ-04 | Phase 4 |
| PROJ-05 | Phase 5 |
| PROJ-06 | Phase 4 |
| PROJ-07 | Phase 4 |
| PROJ-08 | Phase 4 |
| CONS-01 | Phase 6 |
| CONS-02 | Phase 6 |
| CONS-03 | Phase 6 |
| OBSV-01 | Phase 7 |
| OBSV-02 | Phase 7 |
| OBSV-03 | Phase 7 |
| OBSV-04 | Phase 7 |
| STOR-01 | Phase 1 |
| STOR-02 | Phase 2 |
| STOR-03 | Phase 8 |
| CMD-01 | Phase 9 |

**Total:** 27/27 mapped

### Phase 10: Add an examples directory with numerious examples and use cases to show off the features and the usefullness of the library

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 9
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd:plan-phase 10 to break down)

### Phase 11: Update the readme with details about what problem this library solves, and why the developer should use it. i want to be transparent about known limitations

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 10
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd:plan-phase 11 to break down)

### Phase 12: Write Getting started docs, document the entire library and deploy it to crates.io

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 11
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd:plan-phase 12 to break down)

### Phase 13: GDPR and value obfucation. The event schema should support sensitive fields. when fields are sensitive, the values should be stored in a separate key-value store/table.

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 12
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd:plan-phase 13 to break down)

---
*Created: 2026-04-04*
