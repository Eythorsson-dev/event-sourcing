---
phase: 04
plan: 03
subsystem: event-sourcing-core / event-sourcing-logstore-inmemory
tags: [schema-tracking, event-log, schema-store, rust, async-trait]
dependency_graph:
  requires: [04-01, 04-02]
  provides: [EventSchemaStore trait, NoOpEventSchemaStore, InMemoryEventSchemaStore, EventLog<S E> expansion, validate_schemas]
  affects: [event-sourcing, event-sourcing-logstore-inmemory]
tech_stack:
  added: [async-trait = "0.1"]
  patterns:
    - "EventLog<S, E=NoOpEventSchemaStore> — two type parameters with default for backward compat"
    - "with_schema_store(store, schema_store) for explicit schema tracking; new(store) preserved for compat"
    - "append-only store via HashMap::entry().or_insert_with()"
    - "Arc<tokio::sync::RwLock<HashMap>> pattern for shared mutable state in async context"
key_files:
  created:
    - event-sourcing/src/schema_store.rs
  modified:
    - event-sourcing/src/error.rs
    - event-sourcing/src/event_log.rs
    - event-sourcing/src/lib.rs
    - event-sourcing-logstore-inmemory/src/lib.rs
    - event-sourcing-logstore-inmemory/Cargo.toml
    - event-sourcing/Cargo.toml
    - Cargo.toml (workspace)
    - event-sourcing/tests/event_log_integration.rs
decisions:
  - "with_schema_store() instead of two-arg new() — single-arg new(store) must remain for backward compat with existing EventLog<S> call sites"
  - "NoOpEventSchemaStore derives Clone, Copy, Default — needed for EventLog<S, NoOpEventSchemaStore> to satisfy Clone bound"
  - "validate_schemas integration tests placed in event_log_integration.rs (not lib unit tests) to avoid circular dep between event-sourcing and event-sourcing-logstore-inmemory"
  - "NullLogStore mock added inside event_log.rs #[cfg(test)] for unit tests that need a LogStore but not real storage"
metrics:
  duration: "~45 minutes"
  completed: "2026-04-24"
  tasks: 5
  files: 8
---

# Phase 04 Plan 03: EventSchemaStore Infrastructure Summary

EventSchemaStore trait, NoOpEventSchemaStore, InMemoryEventSchemaStore, EventLog<S, E=NoOpEventSchemaStore> expansion, and validate_schemas() startup check — all wired together with full test coverage.

## What Was Built

### Task 1 — schema_store.rs (commit bc0a602)

Created `event-sourcing/src/schema_store.rs` with:

- `EventSchemaError` — `thiserror`-derived error with `StorageFailure(String)` variant
- `EventSchemaStore` trait — three async methods: `record_if_new`, `fetch_all`, `fetch_one`; `#[async_trait]` for `dyn`-compatible dispatch
- `NoOpEventSchemaStore` — zero-size struct implementing `EventSchemaStore` as a no-op; derives `Clone, Copy, Default`
- 4 unit tests: `noop_store_record_returns_ok`, `noop_store_fetch_all_returns_empty`, `noop_store_fetch_one_returns_none`, `noop_store_is_send_sync`

Added `async-trait = "0.1"` to workspace and event-sourcing crate dependencies.

### Task 2 — Error types (commit 88be932)

Extended `event-sourcing/src/error.rs`:

- `AppendError::SchemaConflict { event_type, expected, actual }` — new variant for append-time conflict
- `SchemaConflictError::Conflict { event_type, expected, actual }` — new top-level error for `validate_schemas()`
- `EventLogError::SchemaConflict` — mirrors `AppendError::SchemaConflict` at the user-facing layer; match arm added to `append()` mapping

### Task 3 — EventLog<S, E> expansion (commit 663ac09)

Refactored `event-sourcing/src/event_log.rs`:

- `EventLog<S: LogStore, E: EventSchemaStore = NoOpEventSchemaStore>` — second type parameter with default
- `EventLog<S, E>::with_schema_store(store, schema_store)` — two-arg constructor for explicit schema tracking
- `EventLog<S, NoOpEventSchemaStore>::new(store)` — single-arg backward-compatible constructor (existing call sites unchanged)
- `validate_schemas(code_schemas: impl IntoIterator<Item = EventSchemaDef>)` — startup check: fetches all persisted schemas, checks each code schema for conflicts, returns `SchemaConflictError::Conflict` on first mismatch
- `NullLogStore` mock in `#[cfg(test)]` for unit tests that can't use `InMemoryLogStore` (circular dep)

### Task 4 — InMemoryEventSchemaStore (commit e21ee70)

Added to `event-sourcing-logstore-inmemory/src/lib.rs`:

- `InMemoryEventSchemaStore` — `Arc<RwLock<HashMap<String, EventSchemaDef>>>` backing; `Clone` shares state
- `record_if_new` uses `HashMap::entry().or_insert_with()` — strictly append-only
- 3 unit tests: `record_if_new_stores_schema`, `record_if_new_is_append_only`, `fetch_all_returns_all_stored`
- 2 integration tests in `event_log_integration.rs`: `validate_schemas_ok_when_schemas_match`, `validate_schemas_err_when_schema_differs`

### Task 5 — Re-exports (commit a975433)

Added to `event-sourcing/src/lib.rs`:

- `pub use schema_store::{EventSchemaError, EventSchemaStore, NoOpEventSchemaStore};`
- `pub use error::SchemaConflictError;`
- Applied `cargo fmt --all`

## Test Coverage

| Suite | Count | New | Status |
|-------|-------|-----|--------|
| event-sourcing --lib | 68 | +6 | All pass |
| event_log_integration | 19 | +2 | All pass |
| event-sourcing-logstore-inmemory --lib | 23 | +3 | All pass |
| **Total** | **110** | **+11** | **All pass** |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Non-exhaustive AppendError match in EventLog::append**
- **Found during:** Task 2
- **Issue:** Adding `AppendError::SchemaConflict` made the match in `EventLog::append` non-exhaustive — compiler error E0004
- **Fix:** Added `EventLogError::SchemaConflict` variant and corresponding match arm
- **Files modified:** `event-sourcing/src/event_log.rs`
- **Commit:** 88be932

**2. [Rule 1 - Bug] Partial move of code_schema in validate_schemas**
- **Found during:** Task 3
- **Issue:** `event_type: code_schema.event_type` partially moved `code_schema` before `actual: code_schema`
- **Fix:** Changed to `code_schema.event_type.clone()`
- **Files modified:** `event-sourcing/src/event_log.rs`
- **Commit:** 663ac09

**3. [Rule 2 - Missing functionality] NoOpEventSchemaStore missing Clone**
- **Found during:** Task 3
- **Issue:** `EventLog` derives `Clone`; with `E = NoOpEventSchemaStore` as default, `NoOpEventSchemaStore` must be `Clone` too — integration tests failed with E0599
- **Fix:** Added `#[derive(Debug, Clone, Copy, Default)]` to `NoOpEventSchemaStore`
- **Files modified:** `event-sourcing/src/schema_store.rs`
- **Commit:** a975433 (via fmt pass)

**4. [Rule 2 - Missing functionality] Two-arg new() breaks backward compat**
- **Found during:** Task 3
- **Issue:** Plan specified `new(store, schema_store)` but integration tests call `EventLog::new(store)` — would break existing callers
- **Fix:** Renamed two-arg constructor to `with_schema_store(store, schema_store)`; kept `new(store)` as backward-compatible constructor on `EventLog<S, NoOpEventSchemaStore>`
- **Files modified:** `event-sourcing/src/event_log.rs`
- **Commit:** 663ac09

**5. [Rule 3 - Blocking] validate_schemas tests need InMemoryLogStore in lib tests (circular dep)**
- **Found during:** Task 3
- **Issue:** Inside `event-sourcing/src/event_log.rs` tests, `InMemoryLogStore` from the dev-dep doesn't satisfy `LogStore` trait bound because the inmemory crate depends on event-sourcing (circular)
- **Fix:** Created minimal `NullLogStore` mock within `#[cfg(test)]` for tests that only need a `LogStore` stub; moved `validate_schemas_ok_when_schemas_match` and `validate_schemas_err_when_schema_differs` to the integration test file (has both deps)
- **Files modified:** `event-sourcing/src/event_log.rs`, `event-sourcing/tests/event_log_integration.rs`
- **Commit:** 663ac09 + e21ee70

## Known Stubs

None. All new functionality is fully wired:
- `record_if_new` persists schemas (not a no-op in `InMemoryEventSchemaStore`)
- `validate_schemas` compares persisted vs code schemas and returns typed errors
- `fetch_all` / `fetch_one` return real data from the in-memory store

## Threat Flags

No new network endpoints, auth paths, or trust boundaries introduced. The schema store operates entirely in-process. The `InMemoryEventSchemaStore` append-only guarantee (T-04.03-01) is enforced by `or_insert_with` — tested by `record_if_new_is_append_only`.

## Self-Check: PASSED

| Item | Status |
|------|--------|
| event-sourcing/src/schema_store.rs | FOUND |
| event-sourcing/src/error.rs | FOUND |
| event-sourcing/src/event_log.rs | FOUND |
| event-sourcing/src/lib.rs | FOUND |
| event-sourcing-logstore-inmemory/src/lib.rs | FOUND |
| 04-03-SUMMARY.md | FOUND |
| Commit bc0a602 (Task 1) | FOUND |
| Commit 88be932 (Task 2) | FOUND |
| Commit 663ac09 (Task 3) | FOUND |
| Commit e21ee70 (Task 4) | FOUND |
| Commit a975433 (Task 5) | FOUND |
| All 110 workspace tests | PASSED |
