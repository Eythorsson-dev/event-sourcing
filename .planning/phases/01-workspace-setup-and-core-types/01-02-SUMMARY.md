---
plan: "01-02"
phase: "01"
status: "complete"
completed_at: "2026-04-05T17:27:00Z"
---

# Summary: Plan 01-02 — Core Types and LogStore Trait

## What Was Built

Defined all core types (StreamId, sequence IDs, StoredEvent, NewEvent, errors) and the LogStore trait for the event-sourcing crate. Zero I/O — pure type and contract definitions that every downstream phase imports.

## Key Files Created

### key-files.created
- `event-sourcing/src/types.rs` — StreamId (validated, non-empty), GlobalSequenceId, StreamSequenceId (separate newtypes), NewEvent, InvalidStreamId
- `event-sourcing/src/event.rs` — StoredEvent with 6 fields: global_sequence, stream_id, stream_sequence, event_type, payload (serde_json::Value), timestamp (SystemTime)
- `event-sourcing/src/error.rs` — AppendError (ConcurrencyConflict, StorageFailure), StoreError (Storage), AppendCondition (ExpectedVersion, Any)
- `event-sourcing/src/store.rs` — LogStore trait with 5 async methods, associated EventStream type via futures_core::Stream
- `event-sourcing/src/lib.rs` — module declarations and crate-root re-exports of all public types

## Success Criteria

| # | Criterion | Status |
|---|-----------|--------|
| 1 | StreamId::new("") returns Err(InvalidStreamId) | ✓ test passes |
| 2 | GlobalSequenceId and StreamSequenceId are distinct newtypes | ✓ compile-time distinct |
| 3 | StoredEvent carries 6 fields including serde_json::Value payload | ✓ |
| 4 | AppendError distinguishes ConcurrencyConflict from StorageFailure | ✓ matchable enum variants |
| 5 | LogStore trait has 5 methods with native async fn (no async-trait) | ✓ |
| 6 | All types importable from crate root | ✓ `core_types_importable` test passes |
| 7 | `cargo test -p event-sourcing` passes 13 tests | ✓ |

## Deviations

- `#[allow(async_fn_in_trait)]` added on `LogStore` — intentional per D-11 (native async fn, no async-trait); suppresses the Rust lint warning about dyn-incompatibility.

## Self-Check: PASSED
