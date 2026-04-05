# Phase 1: Workspace Setup and Core Types - Context

**Gathered:** 2026-04-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Cargo workspace scaffold, core type definitions (`StoredEvent`, `AppendError`, `StreamId`, sequence ID types), and the `LogStore` trait abstraction. No I/O, no implementations — just types and contracts that all downstream phases build on.

</domain>

<decisions>
## Implementation Decisions

### Stream Identity
- **D-01:** `StreamId(String)` newtype with explicit constructor `StreamId::new()`. No `From<&str>` / `Into` conversions — construction is always explicit.
- **D-02:** `StreamId::new()` validates non-empty. No other format constraints — users decide their own naming conventions.
- **D-03:** Single `StreamId` type for all streams. Users create their own domain wrapper types (e.g., `UserId`) with a `stream_id() -> StreamId` method for compile-time safety in their code. The library doesn't enforce stream categories.
- **D-04:** No per-event unique ID (no UUID). Events are identified by `(StreamId, StreamSequenceId)` or by `GlobalSequenceId`.

### Sequence Numbers
- **D-05:** `GlobalSequenceId(u64)` and `StreamSequenceId(u64)` as separate newtype wrappers. Prevents accidentally mixing global vs stream-scoped sequences at compile time.
- **D-06:** Both sequence IDs start from 1. Zero means "no events seen" / "beginning of time" — useful for catch-up reads and initial expected version.

### StoredEvent Shape
- **D-07:** `serde_json::Value` for the event payload. Streams are heterogeneous (multiple event types per stream), so a generic `T` doesn't work. Compile-time type safety comes from the deserialization layer (projections, user code), not from the storage model.
- **D-08:** `event_type: String` metadata on every `StoredEvent`. Enables filtering and routing without deserializing the payload — essential for projections that subscribe to specific event types.
- **D-09:** `std::time::SystemTime` timestamp set by the store at append time. No chrono dependency — users convert if they need richer formatting.

### LogStore Trait
- **D-10:** Five methods on the trait: `append`, `read_stream`, `read_all`, `current_sequence`, `stream_version`.
- **D-11:** Static dispatch via generics (`S: LogStore`). No `dyn LogStore` requirement, no `async-trait` dependency. Native `async fn in trait` (stable since Rust 1.75).
- **D-12:** Associated type `type EventStream: Stream<Item = Result<StoredEvent, StoreError>>` for read methods. Uses `futures_core::Stream` trait (lightweight dep).
- **D-13:** Read methods return `Result<Self::EventStream, StoreError>` — separating "couldn't start reading" (connection failure) from "failed mid-read" (stream yields `Err`).

### Error Types
- **D-14:** `AppendError` enum distinguishes `ConcurrencyConflict` from `StorageFailure`. Callers can match on the variant. Uses `thiserror` for derive.
- **D-15:** Separate `StoreError` for read operations (connection failures, mid-stream errors).

### Crate Wiring
- **D-16:** In-memory store stays in a separate crate (`event-sourcing-logstore-inmemory`), not in core. Core defines contracts only — zero implementations.
- **D-17:** `[workspace.dependencies]` in root `Cargo.toml` for shared dependency versions (`serde`, `serde_json`, `thiserror`, `futures-core`).
- **D-18:** No re-exports from core. Explicit imports from each crate: `use event_sourcing_logstore_inmemory::InMemoryLogStore`.

### Claude's Discretion
- Internal module structure within the core crate (how types are organized into modules)
- Exact derive traits on newtypes (Debug, Clone, PartialEq, Eq, Hash, etc.)
- Whether `StreamId::new()` returns `Result` or panics on empty string

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

No external specs — requirements fully captured in decisions above.

### Project Context
- `.planning/PROJECT.md` — Core value, constraints, key decisions
- `.planning/REQUIREMENTS.md` — LOG-01, LOG-02, LOG-03, LOG-07, STOR-01 are the requirements for this phase
- `.planning/research/ARCHITECTURE.md` — Component map, trait boundaries, dependency rules

### Technology Stack
- `CLAUDE.md` — Recommended stack section with version pins and compatibility notes

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — greenfield project, no existing code.

### Established Patterns
- None yet — this phase establishes the patterns all subsequent phases follow.

### Integration Points
- `LogStore` trait is the primary integration point — every storage crate implements it.
- `StoredEvent` is consumed by every downstream component (projections, constraints, observers, commands).
- Sequence ID types are used throughout for concurrency control and catch-up reads.

</code_context>

<specifics>
## Specific Ideas

- Users wrap `StreamId` in their own domain types for compile-time stream category safety — the library stays unopinionated about this.
- The `dyn LogStore` escape hatch can be added later as an `AnyLogStore` wrapper if dynamic dispatch is needed — it's additive and doesn't affect the core trait design.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-workspace-setup-and-core-types*
*Context gathered: 2026-04-05*
