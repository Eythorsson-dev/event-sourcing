# Project Research Summary

**Project:** Rust Event Sourcing Library (workspace of crates)
**Domain:** Infrastructure library — event log + projection-based consistency engine
**Researched:** 2026-04-04
**Confidence:** HIGH

## Executive Summary

This project is a Rust event sourcing library built around the Dynamic Consistency Boundary (DCB) pattern. Unlike every existing Rust event sourcing library (cqrs-es, eventually-rs, event_sourcing.rs), this library deliberately avoids the aggregate root abstraction. Instead, consistency boundaries are defined per-operation by running a projection over a filtered event set before appending. The single `ProjectionEngine` serves both read model materialization and constraint validation — a design that eliminates the usual divergence between query systems and invariant checkers. The JSON-serializable `ProjectionDefinition` type is the lingua franca across both paths, enabling future query language and runtime introspection without code changes.

The recommended approach is a Cargo workspace with strict layering: a core crate (`event-sourcing`) containing only traits and orchestration logic, thin storage implementation crates (`logstore-inmemory`, `logstore-sqlite`), and an optional command layer crate. The technology choices are unambiguous: tokio 1.x as the async runtime (async-std was discontinued in March 2025), serde/serde_json for serialization, rusqlite 0.38 with the `bundled` feature and WAL mode for the SQLite backend, async-trait for `dyn LogStore` object safety, and thiserror for structured library errors. These are all high-confidence choices with strong ecosystem backing.

The primary risks are architectural: the `LogStore` trait must stay lean (pure persistence, no domain logic); the `ProjectionEngine` must resist scope creep into a query language; the event model must include a global sequence ID from day one to enable deterministic multi-stream joins; and optimistic concurrency must be implemented as an atomic compare-and-swap at the storage layer, not as a read-then-write in application code. These are not hypothetical risks — each has a documented failure mode in the pitfalls research that is painful to retrofit once the system is built.

---

## Key Findings

### Recommended Stack

The stack is a clean, minimal set of well-established crates. The core crate has no I/O dependencies at all — tokio is a dev-dependency only for tests, and serde is the sole required runtime dependency. Storage crates introduce tokio, rusqlite, tokio-rusqlite, and rusqlite_migration. The critical incompatibility to be aware of: rusqlite and sqlx both link `libsqlite3-sys` and cannot coexist in the same crate or workspace binary — `logstore-sqlite` must commit to rusqlite exclusively.

**Core technologies:**
- **serde 1.0.220+ / serde_json 1.0.149:** Event and `ProjectionDefinition` serialization — de facto standard, zero realistic alternative
- **tokio 1.x (^1.48):** Async runtime for storage trait implementations — dominant runtime, async-std discontinued March 2025
- **thiserror 2.0:** Structured, matchable library error types — anyhow is for applications, not libraries
- **rusqlite 0.38 (bundled):** SQLite storage backend — eliminates system SQLite version mismatch; WAL mode required
- **tokio-rusqlite 0.5:** Async wrapper for rusqlite — necessary to avoid blocking the tokio executor
- **async-trait 0.1:** `dyn Trait` compatibility for async storage trait — native `async fn in trait` (stable Rust 1.75+) cannot be used as `dyn Trait` yet
- **uuid 1.x (v7):** Time-ordered, lexicographically sortable event and stream IDs

### Expected Features

The library fills a documented gap in the Rust ecosystem. All existing Rust event sourcing libraries mandate aggregate roots. This library's aggregate-free, projection-first design is differentiated not only in Rust but in the broader event sourcing landscape — even Marten (C#/.NET) added multi-stream projections only as a late addition and still doesn't support multi-stream invariant checks.

**Must have (table stakes):**
- Append-only event log with per-stream sequence numbers
- Optimistic concurrency on append (expected version check, atomic at storage layer)
- Read stream by ID (full range and partial range)
- Storage abstraction trait with in-memory implementation for testing
- Typed events via serde (stringly-typed events are a non-starter in Rust)
- Basic projection / state folding from ordered events
- Observer / subscription notifications for derived state and side effects

**Should have (differentiators):**
- Unified projection engine for both read models AND constraints (single engine, no divergence)
- Dynamic consistency boundaries — no mandatory aggregate types
- Multi-stream projections in v1 (rare even outside Rust)
- JSON-serializable `ProjectionDefinition` (unique in Rust ecosystem; foundation for future tooling)
- Inline catch-up reads for read-your-writes consistency
- Three-state observer result: `Ok / Retry / Failed` (explicit retry separate from failure)
- Sequence ID returned from append (enables consistent read gating)
- Multi-stream constraint validation (spans entity boundaries)

**Defer (v2+):**
- Runtime query language (JSON definitions are the foundation; defer the interpreter)
- Persistent runtime views (depends on query language)
- Event upcasting / schema versioning middleware
- Proc macro derive for `ProjectionDefinition` (builder API ships first)
- Saga / process manager support

### Architecture Approach

The architecture is strictly layered using the Trait-First, Impl-Later pattern. `EventLog` is the orchestrator that sequences constraint validation (via `ProjectionEngine`), atomic write (via `LogStore`), and observer dispatch (via `ObserverRegistry`). The `LogStore` trait is a pure persistence interface — no domain logic, no projection knowledge. `ProjectionDefinition` is the central serializable descriptor passed to all projection-related operations. Global sequence IDs (monotonically increasing, assigned at append time) are the ordering anchor for multi-stream joins and catch-up reads. The `commands` crate is an optional, separate layer that depends on core but knows nothing about storage implementations.

**Major components:**
1. **`LogStore` (trait)** — pure persistence: append with optimistic concurrency, read by stream, read by global sequence; implemented separately in `logstore-inmemory` and `logstore-sqlite`
2. **`EventLog`** — orchestrates the append cycle: run constraints, atomic write, dispatch observers; the only component that holds all three collaborators
3. **`ProjectionEngine`** — folds a `ProjectionDefinition` over event streams; handles single-stream, multi-stream joins, and catch-up from a checkpoint sequence ID
4. **`ProjectionDefinition`** — serde-serializable descriptor of sources, join specs, and output shape; the lingua franca across all projection paths
5. **`ConstraintSet`** — one or more `Constraint<Output>` types, each bundling a `ProjectionDefinition` with a validation function
6. **`ObserverRegistry`** — dispatches `Observer` impls post-commit with configurable retry/backoff policy
7. **`ProjectionObserver`** — built-in `Observer` that materializes read models using the `ProjectionEngine`

### Critical Pitfalls

1. **Leaky storage abstraction** — `LogStore` accumulates domain concepts (projections, constraints) over time, becoming unimplementable without specific DB knowledge. Prevention: keep the trait to 5-6 methods (append, read_stream, read_all, current_sequence); all orchestration lives in `EventLog`. Address in Phase 1.

2. **Async trait object safety** — Native `async fn in trait` cannot be used with `dyn Trait`. Starting with concrete generics and discovering the need for `dyn LogStore` later causes painful refactoring. Prevention: decide on `dyn LogStore` upfront and apply `async-trait` from the start. Address in Phase 1.

3. **Optimistic concurrency race window** — Constraint validation reads current state, then another writer appends before the atomic write. Prevention: `LogStore::append` must accept and atomically enforce an `expected_sequence_id`; this is not optional and cannot be retrofitted easily. Address in Phase 2.

4. **Multi-stream join ordering ambiguity** — Without a global sequence ID, replaying events from multiple streams is non-deterministic and produces different results on each run. Prevention: global sequence ID must be in the event model from day one — this is a "never skip" item. Address in Phase 1.

5. **Projection engine scope creep** — Each use case adds one more operation type to `ProjectionDefinition` until it becomes a query engine. Prevention: define a fixed operation vocabulary (set field, append to list, remove from list, increment/decrement) and document that complex logic belongs in `Observer` implementations. Address in Phase 3.

6. **Observer retry storms** — An observer returning `Retry` indefinitely stalls the system. Prevention: `ObserverRegistry` must enforce exponential backoff, a max retry count, and a dead-letter mechanism. Address in Phase 5.

---

## Implications for Roadmap

Based on combined research, the dependency graph is clear and non-negotiable. The build order from ARCHITECTURE.md and FEATURES.md align: core types first, then orchestration, then the projection engine (the differentiator), then constraints, then observers, then storage backends, then the optional command layer.

### Phase 1: Core Types and Storage Abstraction

**Rationale:** Everything else depends on the event model types and the `LogStore` trait. Global sequence ID must be in the model from day one (Pitfall 5 — multi-stream ordering); `dyn LogStore` compatibility must be decided now (Pitfall 2 — async trait objects). `logstore-inmemory` must ship in this phase so all subsequent phases have a test harness.
**Delivers:** `SequenceId`, `StreamId`, `StoredEvent`, `EventQuery`, `AppendCondition`, `AppendError`; `LogStore` trait (async, with `async-trait`); `logstore-inmemory` implementation
**Addresses:** Storage abstraction, typed events, in-memory store for testing (all table stakes)
**Avoids:** Leaky storage abstraction (design the lean trait here), async trait object safety, multi-stream ordering ambiguity

### Phase 2: Event Log and Optimistic Concurrency

**Rationale:** `EventLog` as orchestrator is the backbone that all subsequent phases wire into. Optimistic concurrency must be implemented atomically at the storage layer here — it cannot be added later without redesigning the `LogStore` interface (Pitfall 4 — race window).
**Delivers:** `EventLog` struct with append orchestration; `AppendCondition` enforcement; `SequenceId` returned from append; concurrent append tests
**Addresses:** Append-only event log, per-stream sequence numbers, optimistic concurrency (table stakes); sequence ID for consistent reads (differentiator)
**Avoids:** Optimistic concurrency race window

### Phase 3: Projection Engine

**Rationale:** The projection engine is the primary differentiator. Single-stream fold comes first (unblocks constraint validation and observer materialization in later phases); multi-stream join comes second. JSON-serializable `ProjectionDefinition` is built alongside the engine since retrofitting serde later is painful. Builder API ships first, proc macro deferred (Pitfall 7 — macro ergonomics).
**Delivers:** `ProjectionDefinition` (serde-serializable), `ProjectionEngine` with single-stream fold and multi-stream join, catch-up reads from a checkpoint sequence ID
**Addresses:** Basic projection / state folding, multi-stream projections, JSON-serializable definitions (differentiators)
**Avoids:** Projection engine scope creep (fixed operation vocabulary defined here), macro ergonomics trap (builder API only)

### Phase 4: Constraint Validation

**Rationale:** Constraints are projections with a check function attached — they can only be built after the `ProjectionEngine` is complete. This is the "constraint validation via projection engine" differentiator and includes multi-stream constraints (extremely rare in libraries).
**Delivers:** `Constraint<Output>`, `ConstraintSet`, constraint validation wired into `EventLog::append` before the atomic write
**Addresses:** Constraint validation via projection engine, multi-stream constraints (differentiators)
**Avoids:** Separate engines for read models and constraints (anti-pattern confirmed in architecture research)

### Phase 5: Observer Infrastructure

**Rationale:** Observers are post-commit side effects and depend on a working append cycle. The `ProjectionObserver` (built-in read model materializer) requires both the `ProjectionEngine` and the `Observer` trait, making this the correct phase for both.
**Delivers:** `ObserverResult` enum (`Ok/Retry/Failed`), `Observer` trait, `ObserverRegistry` with backoff + max retry + dead-letter, `ProjectionObserver`, inline catch-up reads for read-your-writes
**Addresses:** Observer/subscription notifications (table stakes); three-state observer result, inline catch-up reads, `ProjectionObserver` (differentiators)
**Avoids:** Observer retry storms (max retry + backoff required here)

### Phase 6: SQLite Log Store

**Rationale:** The in-memory store unblocked all development; the SQLite store makes the library production-usable. WAL mode is required from the first commit to avoid `SQLITE_BUSY` under concurrent access (Pitfall 8). rusqlite with `bundled` feature ensures zero system dependency.
**Delivers:** `logstore-sqlite` implementing `LogStore`; WAL mode enabled at connection time; optimistic concurrency via SQLite transactions; `rusqlite_migration` for schema management
**Addresses:** Pluggable storage backend (table stakes)
**Avoids:** SQLite locking under concurrent access

### Phase 7: Commands Crate (Optional Layer)

**Rationale:** The commands crate is deliberately last — it is a thin composition layer over core primitives and has no knowledge of storage. Its value is in demonstrating the pattern, not in providing infrastructure. It should not gate production readiness.
**Delivers:** `event-sourcing-commands` crate with `CommandHandler` trait; documented pattern: read projection → validate business rules → `EventLog::append`
**Addresses:** Optional command layer (differentiator)
**Uses:** All core primitives; no storage coupling

### Phase Ordering Rationale

- Phase 1 must precede everything because all phases depend on core types and the storage trait. The in-memory implementation in Phase 1 is the test harness for Phases 2-7.
- Phase 2 (optimistic concurrency) must precede Phase 4 (constraints) because constraints depend on the atomic `expected_sequence_id` enforcement being in place.
- Phase 3 (projection engine) must precede Phases 4 and 5 because both constraints and `ProjectionObserver` delegate to it.
- Phase 6 (SQLite) is deliberately late — in-memory is sufficient for correctness testing; SQLite is a production concern that does not block feature development.
- Phase 7 (commands) is explicitly last and optional — it is a pattern demonstration, not infrastructure.
- This ordering matches both the feature dependency graph in FEATURES.md and the suggested build order in ARCHITECTURE.md, providing high confidence.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 3 (Projection Engine):** The JSON-serializable `ProjectionDefinition` operation vocabulary is novel in the Rust ecosystem. The fixed set of operations (what to include, what to exclude) needs deliberate design — no reference implementation exists to copy. Needs design-time research on the operation model.
- **Phase 5 (Observer Infrastructure):** The retry/backoff/dead-letter design for `ObserverRegistry` has several valid patterns (channel-based, in-process queue, async task per observer). The right approach depends on the performance requirements that aren't fully specified yet.

Phases with well-documented patterns (skip research-phase):
- **Phase 1:** Rust workspace structure, async trait patterns, and in-memory collections are well-documented standard patterns.
- **Phase 2:** Optimistic concurrency via expected version is a thoroughly documented pattern across EventStoreDB, Marten, and cqrs-es literature.
- **Phase 6:** rusqlite WAL mode and `tokio-rusqlite` async wrapping are well-documented with high-confidence sources.
- **Phase 7:** The command pattern is a thin composition layer — no novel design needed.

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All core crates verified with official sources; version pins confirmed; async-std discontinuation corroborated by multiple independent sources |
| Features | MEDIUM-HIGH | Cross-referenced against 5 libraries (cqrs-es, eventually-rs, Marten, EventStoreDB, Axon); ecosystem gap analysis is solid; some differentiator complexity estimates are approximate |
| Architecture | HIGH (core), MEDIUM (DCB specifics) | DCB pattern primary site (dcb.events) returned 403 during research; content confirmed via secondary sources (Axon docs, PHP reference impl); overall pattern confidence is high |
| Pitfalls | HIGH (Rust-specific), MEDIUM-HIGH (ES domain) | Rust async and SQLite pitfalls are well-documented with authoritative sources; ES domain pitfalls draw on community consensus across multiple blogs and frameworks |

**Overall confidence:** HIGH

### Gaps to Address

- **DCB specification:** The canonical DCB spec site (dcb.events) was inaccessible during research. The pattern is well-understood from secondary sources (Axon Framework 5, PHP reference implementation, Sara Pellegrini / Milan Savic talks), but the exact spec should be reviewed when available to ensure no edge cases in constraint evaluation were missed.
- **`ProjectionDefinition` operation vocabulary:** The fixed set of allowed operations (set field, append to list, remove, increment) is reasonable but not externally validated. This is a design decision that needs a deliberate spike during Phase 3 planning.
- **Observer registry concurrency model:** Multiple valid designs exist (per-observer async task, sequential dispatch, channel-based queue). The right choice depends on throughput requirements that are not yet specified. Flag for Phase 5 planning.
- **tokio-rusqlite 0.5 version pin:** Research recommends confirming the exact rusqlite version pin inside tokio-rusqlite's Cargo.toml before locking dependencies, as minor incompatibilities can arise.

---

## Sources

### Primary (HIGH confidence)
- [serde-rs/serde — GitHub](https://github.com/serde-rs/serde) — version, derive macro behavior
- [tokio 1.48 — docs.rs](https://docs.rs/crate/tokio/latest) — runtime features, LTS branch
- [rusqlite 0.38 — crates.io](https://crates.io/crates/rusqlite/) — version, bundled feature
- [Rust Blog: async fn and RPITIT in traits](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits/) — `dyn Trait` limitation
- [async-trait — crates.io](https://crates.io/crates/async-trait) — still needed for dyn dispatch
- [sqlx/rusqlite native lib conflict — GitHub Discussion](https://github.com/launchbadge/sqlx/discussions/3295) — hard incompatibility
- [DCB reference implementation — github.com/bwaidelich/dcb-eventstore](https://github.com/bwaidelich/dcb-eventstore) — DCB pattern structure
- [eventually-rs — GitHub](https://github.com/get-eventually/eventually-rs) — Rust ES architecture comparison
- [Cargo Workspaces — doc.rust-lang.org](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) — workspace structure
- [CQRS and Event Sourcing using Rust — doc.rust-cqrs.org](https://doc.rust-cqrs.org/) — cqrs-es features, aggregate requirement

### Secondary (MEDIUM confidence)
- [Choosing Your Async Champion: Tokio vs async-std in 2025 — Medium](https://medium.com/rustaceans/choosing-your-async-champion-tokio-vs-async-std-in-2025-a142d3899b66) — async-std discontinuation
- [The State of Async Rust: Runtimes — corrode.dev](https://corrode.dev/blog/async/) — runtime landscape
- [Rust ORMs in 2026 — Medium](https://aarambhdevhub.medium.com/rust-orms-in-2026-diesel-vs-sqlx-vs-seaorm-vs-rusqlite-which-one-should-you-actually-use-706d0fe912f3) — rusqlite vs sqlx comparison
- [Rust Error Handling — DEV Community](https://dev.to/leapcell/rust-error-handling-compared-anyhow-vs-thiserror-vs-snafu-2003) — thiserror vs anyhow for libraries
- [Marten Projections docs](https://martendb.io/events/projections/) — multi-stream projections, projection lifecycle
- [Marten Multi-Stream Projections](https://martendb.io/events/projections/multi-stream-projections) — cross-stream aggregation patterns
- [Optimistic Concurrency — event-driven.io](https://event-driven.io/en/optimistic_concurrency_for_pessimistic_times/) — concurrency control patterns
- [DCB in Axon Framework 5 — axoniq.io](https://www.axoniq.io/blog/dcb-in-af-5) — DCB pattern in production framework
- [Aggregateless Event Sourcing — ricofritzsche.me](https://ricofritzsche.me/aggregateless-event-sourcing/) — aggregate-free design patterns
- [Immediate Consistency in Event Sourcing — barryosull.com](https://barryosull.com/blog/immediate-consistency-in-event-sourcing/) — catch-up read patterns
- [Projections in Event Sourcing — CodeOpinion](https://codeopinion.com/projections-in-event-sourcing-build-any-model-you-want/) — projection patterns
- [Simple patterns for events schema versioning — event-driven.io](https://event-driven.io/en/simple_events_versioning_patterns/) — upcasting, weak schema patterns

---
*Research completed: 2026-04-04*
*Ready for roadmap: yes*
