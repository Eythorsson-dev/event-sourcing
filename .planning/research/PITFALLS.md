# Pitfalls Research

**Domain:** Rust event sourcing library (no aggregates, DCB, projection-based constraints)
**Researched:** 2026-04-04
**Confidence:** HIGH (Rust-specific), MEDIUM-HIGH (ES domain patterns)

## Critical Pitfalls

### Pitfall 1: Leaky Storage Abstraction

**What goes wrong:**
The `LogStore` trait accumulates database-specific concepts (transactions, connection pools, isolation levels) until it's no longer implementable without a specific database in mind.

**Why it happens:**
Each new feature (constraints, projections, catch-up reads) adds "just one more method" to the trait. SQLite needs WAL mode, Postgres needs advisory locks, in-memory needs nothing — the trait becomes a lowest-common-denominator mess.

**How to avoid:**
Keep `LogStore` to pure persistence operations: append (with expected sequence for optimistic concurrency), read by stream, read all. Constraint validation and projection execution live in `EventLog` (the orchestrator), not in the storage trait. The storage trait never knows about projections.

**Warning signs:**
- `LogStore` trait has more than 5-6 methods
- Storage implementations need `#[cfg(...)]` blocks
- Tests require a specific storage backend to pass

**Phase to address:**
Phase 1 (core types + LogStore trait)

---

### Pitfall 2: Async Trait Object Safety

**What goes wrong:**
Native `async fn in trait` (stable since Rust 1.75) cannot be used with `dyn Trait`. If `LogStore` needs dynamic dispatch (e.g., selecting backend at runtime), the trait won't compile without `async-trait` or boxing.

**Why it happens:**
Developers start with concrete generics (`impl LogStore`), then discover they need `dyn LogStore` for testing or runtime backend selection. By then, the trait is designed around native async and refactoring is painful.

**How to avoid:**
Decide upfront whether `dyn LogStore` is needed. If yes, use `async-trait` on the trait definition from the start. If only generic dispatch is needed (`fn foo<S: LogStore>(store: S)`), native async works fine. The architecture research recommends `async-trait` for `dyn` compatibility.

**Warning signs:**
- "cannot make into object" compiler errors
- Workarounds involving `Box<dyn Future>` in trait methods
- Different trait definitions for tests vs production

**Phase to address:**
Phase 1 (core types + LogStore trait)

---

### Pitfall 3: Projection Engine Becomes a Database

**What goes wrong:**
The projection engine accumulates query capabilities (filtering, sorting, pagination, aggregation) until it's reimplementing a query engine. The JSON-serializable `ProjectionDefinition` grows to express arbitrary computation.

**Why it happens:**
Each new projection use case needs "just one more operation." Multi-stream joins, nested objects, and nested lists are already complex. Adding computed fields, conditional logic, and aggregations makes the definition language Turing-complete.

**How to avoid:**
Define a fixed set of projection operations upfront: set field, append to list, remove from list, increment/decrement, nested object set. The JSON definition expresses data mapping, not computation. Complex logic belongs in custom observer implementations, not in the projection definition language. The future query language is a separate concern.

**Warning signs:**
- `ProjectionDefinition` has more than 10-12 operation types
- Operations reference other operations' outputs
- Users request `if/else` or `match` in projection definitions

**Phase to address:**
Phase 3-4 (Projection Engine)

---

### Pitfall 4: Optimistic Concurrency Race Window

**What goes wrong:**
Between reading the current sequence ID (for constraint validation) and appending the new events, another writer appends to the same stream. The constraint was validated against stale state, and the append succeeds with violated invariants.

**Why it happens:**
The validate-then-write cycle is not atomic. In-memory this is easy (hold a lock), but with real databases the gap exists between the SELECT and INSERT.

**How to avoid:**
The `LogStore.append()` must accept an `expected_sequence_id` parameter and fail atomically if the stream has advanced past it. This is compare-and-swap at the storage level. SQLite achieves this with a transaction + WHERE clause. The constraint validation reads the current state, then append passes the sequence ID it read against — if it changed, the append fails and the caller retries.

**Warning signs:**
- Constraints pass but invalid state appears in read models
- Tests pass with single-threaded execution but fail under concurrency
- No `expected_sequence_id` parameter on append

**Phase to address:**
Phase 2 (Event Log + Optimistic Concurrency)

---

### Pitfall 5: Multi-Stream Join Ordering Ambiguity

**What goes wrong:**
When projecting across multiple streams, events from different streams may have the same sequence ID or no comparable ordering. The projection produces different results depending on which stream's events are processed first.

**Why it happens:**
Each stream has its own sequence numbering. Without a global ordering (like a global sequence ID or wall-clock timestamp), interleaving events from multiple streams is non-deterministic.

**How to avoid:**
Maintain a global sequence ID across all streams (monotonically increasing, assigned at append time). Multi-stream projections process events in global sequence order, not per-stream order. The `LogStore` trait should support `read_all(from_global_sequence)` in addition to `read_stream(stream_id, from_sequence)`.

**Warning signs:**
- Multi-stream projections produce different results on replay
- Tests pass with sequential appends but fail with interleaved appends
- No global ordering concept in the event model

**Phase to address:**
Phase 1 (Event model must include global sequence ID from the start)

---

### Pitfall 6: Observer Retry Storms

**What goes wrong:**
An observer returns `Retry` repeatedly, creating an infinite retry loop that blocks other observers and consumes resources. Failed projections never advance, and the system effectively stalls.

**Why it happens:**
No backoff strategy, no max retry limit, and no dead-letter mechanism. The retry is immediate and unbounded.

**How to avoid:**
Observer registry must enforce: exponential backoff, maximum retry count, and a dead-letter/poison-event mechanism. After max retries, the event is logged as failed and the observer advances past it. The `ObserverResult::Retry` should optionally include a backoff hint.

**Warning signs:**
- CPU spikes correlated with specific events
- Observer lag grows unboundedly
- No metrics on retry counts

**Phase to address:**
Phase 5 (Observer layer)

---

### Pitfall 7: Rust Macro Ergonomics Trap

**What goes wrong:**
Proc macros for projection definitions produce incomprehensible error messages. Users can't figure out what's wrong with their projection definition because the compiler error points at generated code, not their source.

**Why it happens:**
Proc macros using `syn` generate code at compile time. If the macro doesn't propagate span information correctly, errors point to the macro invocation site with no context. Complex macros with many code paths make this exponentially worse.

**How to avoid:**
Start with a builder API (`ProjectionDefinition::new().stream("orders").field(...)`) that works without macros. Add the macro as syntactic sugar later, once the underlying types are stable. This also means the JSON serialization path is tested independently of the macro. Use `syn::Error` to produce human-readable diagnostics from the macro.

**Warning signs:**
- "expected X, found Y" errors pointing at a `#[derive(...)]` line
- Users copying examples verbatim because they can't debug modifications
- Bug reports about the macro outnumber bug reports about the library

**Phase to address:**
Phase 3-4 (Projection definitions — start with builder, add macro later)

---

### Pitfall 8: SQLite Locking Under Concurrent Access

**What goes wrong:**
SQLite's default journal mode uses file-level locking. Multiple concurrent writers (even from different async tasks in the same process) cause `SQLITE_BUSY` errors and dropped appends.

**Why it happens:**
SQLite is a single-writer database by default. `tokio-rusqlite` moves operations to a background thread, but multiple tasks calling append concurrently still contend for the write lock.

**How to avoid:**
Enable WAL (Write-Ahead Logging) mode at connection time: `PRAGMA journal_mode=WAL`. This allows concurrent reads with a single writer. For the write path, funnel all appends through a single writer task (channel-based) or use `PRAGMA busy_timeout` to retry on contention.

**Warning signs:**
- `SQLITE_BUSY` errors under load
- Tests fail intermittently with "database is locked"
- Performance degrades sharply with concurrent appends

**Phase to address:**
Phase 6+ (SQLite logstore implementation)

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Skip global sequence ID | Simpler event model | Multi-stream joins are non-deterministic | Never — add from day 1 |
| Use `String` for event types | Quick prototyping | No compile-time type safety, typos cause silent failures | Never — use typed enums |
| In-memory store with `Vec<Event>` | Simple implementation | O(n) reads, no index support | MVP/testing only — document the limitation |
| Skip `expected_sequence_id` on append | Simpler append path | Constraints can be violated under concurrency | Never — core correctness requirement |
| Hardcode retry policy | Faster observer implementation | Users can't tune for their use case | MVP only — make configurable in next iteration |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Full-stream replay for every projection update | Latency grows linearly with stream length | Checkpoint/snapshot projection state periodically | ~10k events per stream |
| Unbounded in-memory event storage | Memory grows forever | Document as testing-only; add optional capacity limits | Depends on event size, typically ~100k events |
| Synchronous constraint validation on every append | Write latency includes full projection rebuild | Cache constraint projections; only replay from last checkpoint | ~1k events in constraint-relevant streams |
| Serializing entire projection state on every update | I/O dominates write path | Incremental updates to projection storage | Projections with nested lists > 100 items |

## "Looks Done But Isn't" Checklist

- [ ] **Optimistic concurrency:** Often missing concurrent writer tests — verify with `tokio::spawn` + barrier to force true races
- [ ] **Multi-stream projections:** Often missing cross-stream ordering — verify replay determinism with randomized append order
- [ ] **Observer retry:** Often missing max-retry/dead-letter — verify an always-failing observer doesn't block the system
- [ ] **Projection catch-up:** Often missing "catch up from empty" case — verify a new projection can build from event 0
- [ ] **SQLite WAL mode:** Often missing in test setup — verify concurrent read+write doesn't produce SQLITE_BUSY
- [ ] **Event serialization roundtrip:** Often missing for all event variants — verify every enum variant survives serialize→deserialize

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Leaky storage abstraction | Phase 1 | LogStore trait has ≤6 methods; two impls (inmemory, sqlite) share no DB-specific concepts |
| Async trait object safety | Phase 1 | `dyn LogStore` compiles and passes tests |
| Projection engine scope creep | Phase 3-4 | ProjectionDefinition has a fixed operation set; no conditional logic |
| Optimistic concurrency race | Phase 2 | Concurrent append test with constraint validation passes |
| Multi-stream join ordering | Phase 1 | Event model includes global sequence; multi-stream replay is deterministic |
| Observer retry storms | Phase 5 | Always-failing observer test completes without hanging |
| Macro ergonomics | Phase 3-4 | Builder API works without macros; macro errors are readable |
| SQLite locking | Phase 6+ | Concurrent append+read test passes without SQLITE_BUSY |

## Sources

- Rust async trait limitations: Rust RFC 3498, async-trait crate docs
- SQLite WAL mode: SQLite documentation (sqlite.org/wal.html)
- DCB pattern: Axon Framework 5 docs, Sara Pellegrini & Milan Savic talks
- Optimistic concurrency: EventStoreDB documentation, Greg Young's writings
- Event sourcing pitfalls: Michiel Rook (michielrook.nl), event-driven.io blog

---
*Pitfalls research for: Rust event sourcing library*
*Researched: 2026-04-04*
