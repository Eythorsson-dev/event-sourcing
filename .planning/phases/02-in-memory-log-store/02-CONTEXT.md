# Phase 2: In-Memory Log Store - Context

**Gathered:** 2026-04-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement `InMemoryLogStore` as a separate crate (`event-sourcing-logstore-inmemory`) that satisfies the `LogStore` trait defined in Phase 1. Serves as the test harness for all subsequent phases. No persistence — in-memory only.

</domain>

<decisions>
## Implementation Decisions

### dyn LogStore Compatibility
- **D-01:** Drop the `dyn LogStore` requirement. No `async-trait` dependency. The Phase 1 trait design using native `async fn in trait` is kept as-is. Callers use generic `S: LogStore` bounds only.
- **D-02:** Phase 2 success criteria #5 ("pass as `dyn LogStore` without unsafe code") is explicitly relaxed. This was incompatible with Phase 1's native `async fn in trait` design.

### Clone and Sharing Semantics
- **D-03:** `InMemoryLogStore` uses Arc-backed interior storage (`Arc<RwLock<State>>`). `clone()` returns another handle to the **same** underlying store — not a copy. Tests can pass clones to multiple components and observe shared state without wrapping in `Arc` themselves.

### Non-Existent Stream Reads
- **D-04:** `read_stream` on a stream that has never received events returns `Ok(empty_stream)` — not an error. In an append-only log, "stream not found" and "stream is empty" are semantically identical (streams cannot be emptied or deleted). Callers check for emptiness, not errors.

### Claude's Discretion
- Internal concurrency primitive choice (`tokio::sync::RwLock` vs `std::sync::RwLock`)
- Internal data structure for event storage (e.g., `HashMap<StreamId, Vec<StoredEvent>>`)
- Global sequence counter strategy (inside lock vs `AtomicU64`)
- Concrete type used for `EventStream` associated type
- Test coverage scope and structure

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 1 Decisions (LogStore Trait Contract)
- `.planning/phases/01-workspace-setup-and-core-types/01-CONTEXT.md` — Authoritative decisions for `LogStore` trait shape (D-10 through D-18): method signatures, associated `EventStream` type, error types, static dispatch model, crate wiring

### Project Context
- `.planning/REQUIREMENTS.md` — STOR-02 is the sole requirement for this phase
- `.planning/PROJECT.md` — Core constraints: Rust library, crate separation, no aggregates
- `.planning/STATE.md` — Accumulated key decisions (async trait pattern, LogStore interface width)

### Technology Stack
- `CLAUDE.md` — Stack versions: tokio 1.x (`tokio::sync::RwLock` preferred in async crates), futures-core for `Stream` trait, thiserror 2.0 for error types

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — Phase 1 has not been executed yet. The `LogStore` trait and core types (`StoredEvent`, `StreamId`, `GlobalSequenceId`, `StreamSequenceId`, `AppendError`, `StoreError`) will exist after Phase 1 completes.

### Established Patterns
- Phase 1 sets all patterns this phase must follow: crate wiring, type imports, trait signatures.
- No re-exports from core — explicit imports: `use event_sourcing::LogStore;` etc.

### Integration Points
- Implements `LogStore` trait from the `event-sourcing` core crate
- Used as the storage backend in tests for Phase 3 (EventLog), Phase 4 (Projection Engine), and all subsequent phases

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 02-in-memory-log-store*
*Context gathered: 2026-04-05*
