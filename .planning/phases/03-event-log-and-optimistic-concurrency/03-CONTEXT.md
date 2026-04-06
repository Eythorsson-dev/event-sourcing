# Phase 3: Event Log and Optimistic Concurrency - Context

**Gathered:** 2026-04-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Build `EventLog<S>` as the primary user-facing orchestrator in the `event-sourcing` core crate. It wraps a `LogStore`, provides append and read APIs, and sets the extensibility surface for Phase 6 (constraints) and Phase 7 (observers). Optimistic concurrency enforcement stays in the storage layer — `EventLog` delegates rather than re-enforcing.

</domain>

<decisions>
## Implementation Decisions

### Error Type

- **D-01:** `EventLog::append` returns `EventLogError`, a new enum distinct from `AppendError`. Phase 3 variants: `ConcurrencyConflict { stream_id, expected, actual }` and `StorageFailure(..)`. Phase 6 adds `ConstraintViolation` to `EventLogError` — not to `AppendError`.
- **D-02:** `AppendError` stays storage-layer-only (on `LogStore::append`). `EventLogError` wraps the storage error variants internally. Clean layer separation: storage errors vs. event-log-level errors.

### Read API

- **D-03:** `EventLog` exposes `read_stream` and `read_all` as thin wrappers with the same signatures as `LogStore::read_stream` / `read_all`. Callers always read via `EventLog`, not the underlying store directly.
- **D-04:** This sets the pattern for Phase 7 (observers with read-your-writes consistency / inline catch-up), where reads through `EventLog` will gain catch-up logic. Callers going through the store directly would bypass that.

### Append API

- **D-05:** `EventLog::append` signature mirrors `LogStore::append`: `(stream_id: &StreamId, events: Vec<NewEvent>, condition: AppendCondition) -> Result<GlobalSequenceId, EventLogError>`. No named helpers (`append_any`, `append_with_version`) — `AppendCondition` is explicit and self-documenting.
- **D-06:** Concurrency enforcement stays at the storage layer. `EventLog::append` delegates directly — no secondary version check at the `EventLog` level.

### Struct Shape

- **D-07:** `EventLog<S: LogStore>` — generic over the store, owns it. Static dispatch. No `Arc` at the `EventLog` level; sharing is handled by the store's own `Clone` (e.g., `InMemoryLogStore` is `Arc`-backed).
- **D-08:** `EventLog` lives in the `event-sourcing` core crate alongside `LogStore`.

### Claude's Discretion

- Module layout within the core crate for `EventLog` and `EventLogError`
- Whether `EventLog` derives `Clone` (it would naturally if `S: Clone`)
- Test structure and coverage scope

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Contract (LogStore trait)
- `.planning/phases/01-workspace-setup-and-core-types/01-CONTEXT.md` — LogStore trait decisions (D-10 through D-18): method signatures, `AppendCondition`, `AppendError`, static dispatch model, `EventStream` associated type
- `.planning/phases/02-in-memory-log-store/02-CONTEXT.md` — D-01/D-02: no `dyn LogStore`, D-03: `Arc`-backed clone semantics, D-04: non-existent stream reads return empty

### Requirements
- `.planning/REQUIREMENTS.md` — LOG-04, LOG-05, LOG-06 are the requirements for this phase
- `.planning/PROJECT.md` — Key decisions: concurrency enforcement at storage layer, LogStore interface width = 5 methods

### Technology Stack
- `CLAUDE.md` — Stack versions; tokio 1.x, thiserror 2.0, futures-core for `Stream` trait

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `event-sourcing/src/error.rs` — `AppendError`, `AppendCondition`, `StoreError` already defined. `EventLogError` will be added here (or a new `event_log.rs` module).
- `event-sourcing/src/store.rs` — `LogStore` trait with all 5 methods; `EventLog::append` / `read_stream` / `read_all` mirror these signatures.
- `event-sourcing-logstore-inmemory/src/lib.rs` — `InMemoryLogStore` is the test harness for Phase 3. Its optimistic concurrency tests (`test_append_expected_version_conflict`, `test_append_expected_version_zero_on_new_stream`) establish the storage-layer guarantee that `EventLog` delegates to.

### Established Patterns
- Static dispatch throughout: `S: LogStore` generics, native `async fn in trait`, no `async-trait`.
- `AppendCondition::Any` / `AppendCondition::ExpectedVersion(StreamSequenceId)` is the concurrency API — `EventLog` exposes it unchanged.
- Sequence IDs start at 1; `ZERO` = no events. `current_sequence()` returns `ZERO` on empty store.

### Integration Points
- `EventLog<S>` takes ownership of `S: LogStore` — `InMemoryLogStore::new()` passed directly in tests.
- Phase 6 adds constraint registration to `EventLog`; `EventLogError` gets `ConstraintViolation` variant.
- Phase 7 adds observer registration and catch-up logic to `EventLog::read_stream` / `read_all`.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-event-log-and-optimistic-concurrency*
*Context gathered: 2026-04-06*
