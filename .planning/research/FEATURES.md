# Feature Landscape

**Domain:** Rust event sourcing library (constrained event log + projection engine)
**Researched:** 2026-04-04
**Confidence:** MEDIUM-HIGH — cross-referenced across EventStoreDB, Marten, Axon, cqrs-es, eventually-rs, and community sources

---

## Table Stakes

Features users expect from any event sourcing library. Missing or broken = users leave or roll their own.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Append-only event log | Core primitive — without it, no event sourcing | Low | Events must be immutable once written |
| Per-stream sequence numbers | Every library provides this; required for ordering and concurrency | Low | Global position vs per-stream position both useful |
| Optimistic concurrency on stream append | Race conditions without it; data corruption in concurrent scenarios | Low-Med | Expected version check at append time; configurable (strict vs any) |
| Read stream by ID | Load events for a given entity/stream | Low | Must support reading all events or a range (from/to version) |
| Storage abstraction (pluggable backend) | Users have diverse DB requirements; in-memory required for testing | Medium | Trait-based in Rust; cqrs-es, eventually-rs, and eventstore all do this |
| In-memory store for testing | A test-doubles mechanism; without it, tests require a real DB | Low | Every production-grade library ships this |
| Typed events | Type safety is core Rust expectation; stringly-typed events are unusable | Medium | Serde-based serialization is the de facto standard in Rust |
| Basic projection / state folding | Rebuilding aggregate/read-model state from events is the core use case | Medium | Left-fold over ordered events is universal pattern |
| Error handling on append | Conflicts, validation failures, storage errors must be represented | Low | Rust Result types; should distinguish conflict vs storage failure |
| Observer / subscription notification | React to new events — required for derived state, side effects | Medium | All serious libraries expose this (Marten async daemon, EventStoreDB persistent subs, cqrs-es query processor) |

---

## Differentiators

Features that set a library apart. Not expected by default, but meaningfully valuable when present.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Unified projection engine for read models AND constraints | One system instead of two — read models and invariant checks use the same engine | High | This library's core design thesis; Marten separates these concerns; most libraries do too |
| Dynamic consistency boundaries (no aggregates) | More flexible than fixed aggregate root types; reduce boilerplate; enables runtime-defined streams | High | Axon, cqrs-es, eventually-rs all mandate aggregates; this library deliberately avoids them |
| Multi-stream projections in v1 | Most libraries (cqrs-es, eventually-rs) only support single-stream projections; multi-stream joins are deferred or not supported | High | Marten has multi-stream projections but it's a late addition; having it from v1 is differentiated |
| JSON-serializable projection definitions | Enables a future query language / runtime projection definitions without code changes | High | Novel in the Rust ecosystem; enables tooling, introspection, migration |
| Inline catch-up reads (consistency on read) | Read-your-writes guarantee; most libraries expose eventual consistency only | Medium | Caller-controlled; powerful for UX that needs immediate consistency after write |
| Sequence ID returned from append | Enables consistent reads — caller can gate reads on seeing their own write | Low-Med | Some libraries return version; fewer expose it for use in read-side gating |
| Constraints validated via projection engine | Invariant checking is a first-class operation, not an ad-hoc query before append | High | Most libraries put constraint logic in aggregate methods; this externalizes it |
| Multi-stream constraints (invariants over multiple streams) | E.g., global uniqueness checks that span entities | High | Extremely rare in library-level; usually hand-rolled by users |
| Three-state observer result (Ok / Retry / Failed) | Explicit retry semantic separate from failure; better than boolean success | Low | Most observers are fire-and-forget or two-state; this is a meaningful refinement |
| Workspace crate structure (separate storage crates) | Users take only what they need; no forced dependencies | Low | Best-practice Rust workspace pattern; cqrs-es bundles less; provides cleaner separation |
| Optional command layer as separate crate | Core stays minimal; commands are one of many patterns on top | Medium | Forces good design; users aren't locked into command/handler pattern |

---

## Anti-Features

Features to deliberately NOT build. Including them would add coupling, complexity, or design pollution without proportional value.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Aggregate root as a required abstraction | Mandating aggregates forces a design pattern on users; constrains consistency boundary flexibility | Use projection-based constraints and dynamic streams as the consistency primitive |
| Built-in saga / process manager | Sagas are application-level orchestration, not infrastructure; every domain needs different semantics | Document patterns users can implement; consider a separate crate later if demand exists |
| Aggregate snapshotting | Snapshots are an optimization for aggregate-replay cost; this library has no aggregates to replay | If projection performance becomes an issue, users can cache materialized state themselves |
| Built-in message bus / event dispatch to external systems | Crosses the library/framework boundary; ties users to a specific transport | Observer trait provides the hook; users wire to their own bus |
| Authentication / authorization | Out of scope; highly application-specific | Not the library's concern |
| Networking / transport layer | This is a library, not a framework or server | Users compose with their own HTTP/gRPC stack |
| Runtime query language (in v1) | Premature; JSON serialization lays the groundwork without the complexity | JSON projection definitions are the foundation; query language is future work |
| Views / persistent runtime projections (in v1) | Depends on query language; deferred appropriately | Build after query language lands |
| Schema migration tooling | Heavy operational concern; depends on storage backend | Users handle with their database tooling |
| Event upcasting / versioning middleware | Valuable but high complexity; can be layered on top by users | Document patterns (upcasting, weak schema, copy-transform); ship as optional crate later |
| Distributed cluster / replication | Out of scope for a library; that's a database concern | Delegate to the storage backend (e.g., SQLite WAL, Postgres streaming replication) |
| GDPR / encryption helpers | Useful but orthogonal; would make the core opinionated | Leave to users; can be added in a separate crate |
| Test DSL / given-when-then framework | Useful but not the core library's job; don't bake in opinionated test idioms | Document testing patterns; users write against the storage trait with in-memory backend |

---

## Feature Dependencies

```
Storage Trait
  └─> In-Memory Log Store        (requires: Storage Trait)
  └─> SQLite Log Store           (requires: Storage Trait)

Event Log (append + read)
  └─> Optimistic Concurrency     (requires: Event Log)
  └─> Sequence ID on append      (requires: Event Log)

Projection Engine
  └─> Read Models                (requires: Projection Engine, Event Log)
  └─> Constraint Validation      (requires: Projection Engine, Event Log)
  └─> Multi-Stream Projections   (requires: Projection Engine, Event Log)
  └─> Multi-Stream Constraints   (requires: Multi-Stream Projections, Constraint Validation)
  └─> JSON Serializable Defs     (requires: Projection Engine)
      └─> Future Query Language  (requires: JSON Serializable Defs)
      └─> Future Runtime Views   (requires: JSON Serializable Defs, Query Language)

Observer Trait
  └─> Projection Observer        (requires: Observer Trait, Projection Engine)
  └─> Retry/Success/Fail Result  (requires: Observer Trait)

Inline Catch-Up Reads
  └─> Requires: Sequence ID on append, Projection Engine

Commands Crate
  └─> Requires: Event Log, Projection Engine (for constraints), Observer Trait
```

---

## MVP Recommendation

The project's active requirements are well-scoped. Prioritize in this order:

**Phase 1 — Core primitives (non-negotiable table stakes):**
1. Storage Trait abstraction
2. In-memory log store (enables all testing)
3. Event log append with optimistic concurrency
4. Sequence ID returned from append
5. Read stream by ID

**Phase 2 — Projection engine (the differentiator):**
6. Projection definition types and folding
7. Single-stream projections
8. Constraint validation via projections
9. Multi-stream projections
10. Multi-stream constraints (flows naturally from #9)
11. JSON serializable projection definitions

**Phase 3 — Observer layer:**
12. Observer trait (Ok/Retry/Failed)
13. Projection observer (built-in observer that saves read models)
14. Inline catch-up on reads

**Phase 4 — Storage backends:**
15. SQLite log store

**Phase 5 — Optional command layer:**
16. Commands crate (separate, optional)

**Defer:**
- Query language: JSON definitions are the foundation; defer runtime queries
- Event upcasting: Document patterns; ship as separate crate when schema evolution becomes a real user need
- Saga/process manager: Out of scope; document patterns

---

## Ecosystem Gap Analysis

This library fills a genuine gap in the Rust event sourcing space. Current Rust options:

| Library | Status | Aggregates Required | Multi-Stream Projections | Constraint via Projection | JSON Defs |
|---------|--------|---------------------|--------------------------|--------------------------|-----------|
| cqrs-es | Active | Yes (mandatory) | No | No | No |
| eventually-rs | Active, pre-1.0 | Yes (mandatory) | No | No | No |
| event_sourcing.rs (prima) | Active | Yes (mandatory) | No | No | No |
| eventstore (ESDB client) | Active | No (it's a DB client) | Via server-side JS | No | No |
| **this library** | **Greenfield** | **No (by design)** | **Yes (v1)** | **Yes (v1)** | **Yes (v1)** |

The aggregate-free, projection-first design is the primary differentiator in the Rust ecosystem. Multi-stream constraints in v1 is rare even outside Rust — Marten added multi-stream projections late; most libraries still don't support multi-stream invariant checks.

---

## Sources

- [CQRS and Event Sourcing using Rust (doc.rust-cqrs.org)](https://doc.rust-cqrs.org/) — cqrs-es features and design
- [eventually-rs GitHub](https://github.com/get-eventually/eventually-rs) — Rust event sourcing abstractions
- [Marten Projections docs](https://martendb.io/events/projections/) — projection lifecycle (live/inline/async), multi-stream
- [Marten Multi-Stream Projections](https://martendb.io/events/projections/multi-stream-projections) — cross-stream aggregation patterns (updated 2026-01-30)
- [EventStoreDB (Kurrent) Guide](https://www.kurrent.io/guide-to-event-stores) — EventStoreDB feature overview
- [Axon Framework 5](https://www.axoniq.io/framework) — Stateful event handlers, reactive async, sagas
- [Optimistic Concurrency — Event-Driven.io](https://event-driven.io/en/optimistic_concurrency_for_pessimistic_times/) — Concurrency control patterns
- [Simple patterns for events schema versioning — Event-Driven.io](https://event-driven.io/en/simple_events_versioning_patterns/) — Upcasting, weak schema, copy-transform strategies
- [Projections in Event Sourcing — CodeOpinion](https://codeopinion.com/projections-in-event-sourcing-build-any-model-you-want/) — Projection patterns
- [Testing business logic in Event Sourcing — Event-Driven.io](https://event-driven.io/en/testing_event_sourcing/) — Given/when/then testing patterns
- [Event Sourcing Projection Deduplication — DomainCentric](https://domaincentric.net/blog/event-sourcing-projection-patterns-deduplication-strategies) — Idempotency in projections
- [Snapshots in Event Sourcing — Kurrent](https://www.kurrent.io/blog/snapshots-in-event-sourcing) — Snapshotting as optimization
- [Python eventsourcing library](https://eventsourcing.readthedocs.io/en/stable/topics/introduction.html) — Encryption, compression, propagation features
- [primait/event_sourcing.rs](https://github.com/primait/event_sourcing.rs) — Rust event sourcing framework comparison
