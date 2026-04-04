# Architecture Patterns

**Domain:** Rust event sourcing library (no aggregates, projection-based constraints, DCB)
**Researched:** 2026-04-04
**Overall confidence:** HIGH (core structure), MEDIUM (DCB/projection-constraint integration specifics)

---

## Recommended Architecture

This library follows the Dynamic Consistency Boundary (DCB) pattern: consistency is not tied to fixed aggregate roots. Instead, it is enforced per-operation by running a projection over a filtered event set immediately before appending. The projection engine is the single engine for both read models and constraint validation.

### High-Level Component Map

```
┌─────────────────────────────────────────────────────────────┐
│  event-sourcing (core crate)                                 │
│                                                             │
│  ┌───────────────┐    ┌──────────────────────────────────┐  │
│  │  EventLog     │───▶│  LogStore (trait)                │  │
│  │  (append,     │    │  - append(events, condition)     │  │
│  │   read)       │    │  - read(query) -> Stream         │  │
│  └───────┬───────┘    └──────────────────────────────────┘  │
│          │                                                  │
│          │ on append                                        │
│          ▼                                                  │
│  ┌───────────────┐    ┌──────────────────────────────────┐  │
│  │  Observer     │    │  ProjectionEngine                │  │
│  │  Registry     │───▶│  - run(definition, events)       │  │
│  │  (dispatch)   │    │  - catch_up(definition, seq_id)  │  │
│  └───────────────┘    └──────────────────────────────────┘  │
│                                ▲                            │
│  ┌─────────────────────────────┤──────────────────────────┐ │
│  │  ProjectionDefinition       │                          │ │
│  │  (Serialize/Deserialize)    │                          │ │
│  │  - stream filters           │                          │ │
│  │  - join specs               │                          │ │
│  │  - output shape             │                          │ │
│  └─────────────────────────────┘                          │ │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  ConstraintSet                                       │  │
│  │  - attached to a stream                              │  │
│  │  - each constraint = ProjectionDefinition + check fn │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

┌────────────────────────┐  ┌─────────────────────────────┐
│  logstore-inmemory     │  │  logstore-sqlite             │
│  impl LogStore         │  │  impl LogStore               │
└────────────────────────┘  └─────────────────────────────┘

┌────────────────────────────────────────────────────────────┐
│  event-sourcing-commands (optional, separate crate)         │
│  - CommandHandler trait                                     │
│  - reads projections, calls EventLog::append               │
└────────────────────────────────────────────────────────────┘
```

---

## Component Boundaries

| Component | Responsibility | Communicates With | In Crate |
|-----------|---------------|-------------------|----------|
| `EventLog` | Orchestrates append: validates constraints, persists events, notifies observers | `LogStore`, `ObserverRegistry`, `ProjectionEngine` | `event-sourcing` |
| `LogStore` (trait) | Durable persistence of events; optimistic concurrency enforcement | `EventLog` calls it | `event-sourcing` (trait), `logstore-*` (impls) |
| `ProjectionEngine` | Runs a `ProjectionDefinition` against an event stream, returns a typed output | `LogStore` (reads events), `EventLog` (catch-up on reads) | `event-sourcing` |
| `ProjectionDefinition` | Serializable descriptor: which streams, how to join them, what shape to produce | Used by `ProjectionEngine`, `ConstraintSet` | `event-sourcing` |
| `ConstraintSet` | One or more constraints attached to a stream; each constraint bundles a `ProjectionDefinition` with a validation function | `EventLog` during append | `event-sourcing` |
| `Observer` (trait) | Reacts to appended events; returns `Ok / Retry / Failed` | Called by `ObserverRegistry` after commit | `event-sourcing` |
| `ObserverRegistry` | Holds registered observers; dispatches after successful append | `EventLog` triggers it | `event-sourcing` |
| `ProjectionObserver` | Built-in `Observer` impl that materializes read models to storage | Implements `Observer`, wraps `ProjectionEngine` | `event-sourcing` |
| `logstore-inmemory` | `LogStore` impl backed by `HashMap`/`Vec` in memory; for tests and dev | Depends on `event-sourcing` traits only | `logstore-inmemory` |
| `logstore-sqlite` | `LogStore` impl backed by SQLite via `rusqlite` or `sqlx` | Depends on `event-sourcing` traits only | `logstore-sqlite` |
| `commands` (optional) | `CommandHandler` trait: read projections, run business logic, call `EventLog::append` | Depends on `event-sourcing`; no knowledge of storage impls | `event-sourcing-commands` |

**Dependency rule:** Only `EventLog` holds references to `LogStore`. `commands` crate depends on `event-sourcing` core but never on `logstore-*` directly — storage is injected.

---

## Data Flow

### Append (write path with constraint validation)

```
Caller
  │
  │  append(stream_id, events, expected_version?)
  ▼
EventLog
  │
  ├─1─▶ ProjectionEngine.run(constraint.definition, read current events)
  │       └─▶ LogStore.read(stream filter)   [reads existing events for constraint]
  │       └─▶ returns ProjectionOutput
  │
  ├─2─▶ constraint.check(ProjectionOutput) → Ok | Violated(reason)
  │       [if Violated → return Err, abort]
  │
  ├─3─▶ LogStore.append(events, AppendCondition { expected_version })
  │       [storage enforces optimistic concurrency, returns SequenceId]
  │
  ├─4─▶ ObserverRegistry.dispatch(appended_events)
  │       └─▶ Observer::on_event(event) → ObserverResult { Ok | Retry | Failed }
  │             [retry loop controlled by registry; failures are surfaced, not panicked]
  │
  └─▶ returns Ok(SequenceId) to caller
```

### Read with optional catch-up

```
Caller
  │
  │  read_projection(definition, min_seq_id?)
  ▼
ProjectionEngine
  │
  ├─ if min_seq_id is Some:
  │    check current projection seq_id
  │    if behind → run catch-up: LogStore.read(events since last_seq)
  │                              apply to projection state
  │
  └─▶ return ProjectionOutput (typed)
```

### Multi-stream join flow

```
ProjectionDefinition {
    sources: [StreamFilter("orders.*"), StreamFilter("payments.*")],
    join_key: "order_id",
    output_shape: { ... }
}
                │
                ▼
ProjectionEngine
  ├─▶ LogStore.read(filter: orders.*)   → event stream A
  ├─▶ LogStore.read(filter: payments.*) → event stream B
  ├─▶ merge by sequence position (global ordering)
  └─▶ fold into output shape, correlating on join_key
```

### Observer dispatch (post-commit side effects)

```
ObserverRegistry
  ├─▶ ProjectionObserver (built-in, materializes read models)
  │     └─▶ ProjectionEngine.apply(new_events) → updated read model state
  │     └─▶ LogStore (or separate store) persists read model
  │
  └─▶ UserObserver (external, e.g. send email, update search index)
        └─▶ returns ObserverResult::Ok | Retry | Failed
```

---

## Key Traits (Rust signatures — indicative, not final)

```rust
// Storage backend — the only external dependency for core behavior
pub trait LogStore: Send + Sync {
    type Event: Serialize + DeserializeOwned;
    async fn append(
        &self,
        stream_id: StreamId,
        events: Vec<Self::Event>,
        condition: AppendCondition,
    ) -> Result<SequenceId, AppendError>;

    async fn read(
        &self,
        query: EventQuery,
    ) -> impl Stream<Item = Result<StoredEvent<Self::Event>, ReadError>>;

    async fn current_sequence(&self) -> Result<SequenceId, ReadError>;
}

// Three-state observer result — not a simple Result<(), E>
pub enum ObserverResult {
    Ok,
    Retry,
    Failed(Box<dyn std::error::Error + Send + Sync>),
}

pub trait Observer: Send + Sync {
    type Event;
    async fn on_events(&self, events: &[StoredEvent<Self::Event>]) -> ObserverResult;
}

// Serializable projection definition — the centerpiece
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionDefinition {
    pub sources: Vec<StreamFilter>,
    pub join: Option<JoinSpec>,
    pub output: OutputShape,
}

// Constraint = projection + validation function
pub struct Constraint<Output> {
    pub definition: ProjectionDefinition,
    pub check: Box<dyn Fn(&Output) -> Result<(), ConstraintViolation> + Send + Sync>,
}
```

---

## Patterns to Follow

### Pattern 1: Trait-First, Impl-Later

Define `LogStore`, `Observer`, and associated types in the core crate as pure traits. Storage implementations know nothing about the projection engine or observer registry. This keeps `logstore-inmemory` and `logstore-sqlite` thin adapters.

### Pattern 2: ProjectionDefinition as the Lingua Franca

All runtime behavior that touches projections — constraint validation, read model materialization, catch-up reads — passes through a `ProjectionDefinition`. This is the single serializable descriptor that can later be persisted, transmitted, or constructed from a JSON payload. Do not create parallel code paths that bypass it.

### Pattern 3: Global Sequence ID as the Consistency Anchor

Every stored event receives a global, monotonically increasing `SequenceId` (separate from per-stream version). Catch-up reads use this: "give me the read model as of at least sequence 1042." This enables the caller to express "I wrote at sequence 1042, I need a read that reflects that write" without polling.

**Confidence:** HIGH — this pattern is confirmed by multiple event sourcing implementations (Marten, eventsourcing.readthedocs.io, DCB spec).

### Pattern 4: Optimistic Concurrency at the Storage Layer

The `LogStore::append` receives an `AppendCondition` that includes the expected per-stream version (or "any"). The storage backend is responsible for atomically checking and rejecting. The core crate does not re-implement this — it trusts the backend's atomicity guarantee. This is standard across all major event sourcing systems.

### Pattern 5: Constraints Run Before Append, Observers Run After

The append cycle is: validate constraints (read-only, projection engine) → atomic write (storage) → notify observers (side effects). This ordering is not negotiable: constraints must see committed state; observers must see the just-written events.

### Pattern 6: Observer Retry Is Caller-Controlled

The `ObserverResult::Retry` signal tells the registry to re-invoke the observer. The retry policy (max attempts, backoff) is configured on the registry, not inside the observer. This separates the domain concern (is this event processed?) from the infrastructure concern (how long do we retry?).

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Embedding Domain Logic in LogStore Implementations

**What:** Putting constraint validation, projection logic, or observer dispatch inside `LogStore` impls.
**Why bad:** Storage backends become opaque black boxes; users cannot swap backends without losing behavior; testing becomes storage-dependent.
**Instead:** `LogStore` is a pure persistence interface. All orchestration lives in `EventLog`.

### Anti-Pattern 2: Separate Engines for Read Models and Constraints

**What:** Having a "read model projection engine" and a separate "constraint checker" that duplicate fold/join logic.
**Why bad:** Two systems diverge over time; complex multi-stream joins must be implemented twice; JSON-serializable definitions only matter for one path.
**Instead:** `ProjectionEngine` is used for both. Constraints are just projections with a check function attached.

### Anti-Pattern 3: Aggregate Root as a Hidden Concept

**What:** Sneaking aggregate-like types in (e.g., `StreamState` that is always loaded before append, with fixed identity = stream ID).
**Why bad:** Defeats the DCB design; consistency boundary becomes hardcoded to stream identity again; multi-stream constraints cannot work naturally.
**Instead:** Consistency boundary is defined by the `ProjectionDefinition` passed to the constraint. A single-stream constraint is just a projection whose `sources` has one `StreamFilter`.

### Anti-Pattern 4: Sync-Only Storage Traits

**What:** Defining `LogStore` without `async` methods to avoid `async_trait` or RPITIT complexity.
**Why bad:** All real storage backends (SQLite with async driver, Postgres, etc.) are async. Forcing sync blocks the executor.
**Instead:** Use `async fn` in traits (stable in Rust 1.75+ via RPITIT). For object-safe cases, use the `async-trait` crate until RPITIT is fully stabilized for dyn dispatch.

### Anti-Pattern 5: Baking Retry Logic into Observer Trait

**What:** Giving `Observer` a `max_retries` method or building retry loops inside implementations.
**Why bad:** Every observer author reimplements retry differently; no uniform policy; testing retry behavior requires mocking time.
**Instead:** `ObserverResult::Retry` is a signal. The registry owns the retry loop and policy configuration.

---

## Suggested Build Order

Dependencies flow strictly: core traits first, then engines, then storage impls, then optional layers.

```
Phase 1: Core Types and Storage Trait
  - SequenceId, StreamId, StoredEvent, EventQuery, AppendCondition, AppendError
  - LogStore trait (async)
  - logstore-inmemory (unblocks all subsequent testing without real storage)

Phase 2: Append Orchestration and Optimistic Concurrency
  - EventLog struct wrapping LogStore
  - AppendCondition enforcement (delegate to LogStore, test with inmemory)
  - SequenceId returned from append

Phase 3: Projection Engine
  - ProjectionDefinition struct (with Serialize/Deserialize via serde)
  - ProjectionEngine: single-stream fold
  - ProjectionEngine: multi-stream join
  - Catch-up reads (apply events since last_seq to a projection)

Phase 4: Constraint Validation
  - Constraint<Output> type
  - ConstraintSet attached to EventLog
  - Constraint check runs inside append, before LogStore.append

Phase 5: Observer Infrastructure
  - ObserverResult enum (Ok, Retry, Failed)
  - Observer trait
  - ObserverRegistry with retry policy
  - ProjectionObserver (built-in observer for read model materialization)
  - Wire into EventLog post-append dispatch

Phase 6: SQLite LogStore
  - logstore-sqlite implementing LogStore
  - Optimistic concurrency via SQLite transactions + version column
  - Sequence ID via ROWID or sequence table

Phase 7: Commands Crate (optional layer)
  - CommandHandler trait
  - Pattern: read projection → check business rules → EventLog::append
  - No storage knowledge; pure composition of core primitives
```

**Critical dependency:** `ProjectionEngine` must be complete before `Constraint` validation and `ProjectionObserver` can be built — they both delegate to it. `logstore-inmemory` must exist before any engine or constraint work to enable testing.

---

## Scalability Considerations

| Concern | Library-level answer | User's responsibility |
|---------|---------------------|-----------------------|
| Catch-up read performance | `ProjectionEngine` accepts a `last_seq_id` to resume from; does not replay from zero | Persist projection checkpoints (sequence watermarks) between runs |
| Multi-stream join size | Projection engine loads all matched events into memory for a join | Users should constrain `StreamFilter` to bounded sets; unbounded joins are a user problem |
| Observer throughput | Registry dispatches observers after each append; no built-in batching | Users implement batching inside their `Observer` if needed |
| Constraint cost | Each constraint replays the filtered event set on every append | Keep constraint projections narrow (filter precisely); avoid full-log scans |
| Concurrent appends | Optimistic concurrency at storage layer; losers get `AppendError::Conflict` | Callers retry with fresh projection read |

---

## Crate Dependency Graph

```
event-sourcing-commands
    └── event-sourcing (core)

logstore-sqlite
    └── event-sourcing (core)

logstore-inmemory
    └── event-sourcing (core)

event-sourcing (core)
    ├── serde (ProjectionDefinition serialization)
    ├── serde_json (JSON round-trip for definitions)
    └── [no storage dependencies — pure traits]
```

`event-sourcing` (core) has zero knowledge of any storage crate. This is enforced structurally: storage crates depend on core, never the reverse.

---

## Sources

- [Dynamic Consistency Boundary pattern — dcb.events](https://dcb.events/) (MEDIUM — site returned 403 during research, content inferred from secondary sources)
- [DCB EventStore PHP reference implementation — github.com/bwaidelich/dcb-eventstore](https://github.com/bwaidelich/dcb-eventstore) (HIGH — inspected directly)
- [DCB in Axon Framework 5 — axoniq.io](https://www.axoniq.io/blog/dcb-in-af-5) (MEDIUM — WebSearch confirmed)
- [Eventually-rs architecture — github.com/get-eventually/eventually-rs](https://github.com/get-eventually/eventually-rs) (HIGH — inspected via WebFetch)
- [Aggregateless Event Sourcing — ricofritzsche.me](https://ricofritzsche.me/aggregateless-event-sourcing/) (MEDIUM — WebSearch confirmed)
- [Projections and Read Models in Event-Driven Architecture — event-driven.io](https://event-driven.io/en/projections_and_read_models_in_event_driven_architecture/) (MEDIUM — search confirmed, 403 on fetch)
- [Optimistic concurrency for pessimistic times — event-driven.io](https://event-driven.io/en/optimistic_concurrency_for_pessimistic_times/) (MEDIUM — WebSearch confirmed)
- [Live projections for read models — kurrent.io](https://www.kurrent.io/blog/live-projections-for-read-models-with-event-sourcing-and-cqrs) (MEDIUM — WebSearch confirmed)
- [Immediate Consistency in Event Sourcing — barryosull.com](https://barryosull.com/blog/immediate-consistency-in-event-sourcing/) (MEDIUM — 403 on fetch, content confirmed via search excerpt)
- [Cargo Workspaces — doc.rust-lang.org](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) (HIGH — official)
- [Re-export dependencies — lurklurk.org/effective-rust](https://lurklurk.org/effective-rust/re-export.html) (HIGH — Effective Rust book)
- [CQRS and Event Sourcing using Rust — doc.rust-cqrs.org](https://doc.rust-cqrs.org/) (HIGH — official cqrs-es docs)
