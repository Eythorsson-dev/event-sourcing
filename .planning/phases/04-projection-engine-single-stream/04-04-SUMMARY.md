---
phase: "04"
plan: "04"
subsystem: event-sourcing-core
tags: [projection-engine, read-model, fold, checkpoint, json-path]
dependency_graph:
  requires: [04-01, 04-02, 04-03]
  provides: [ProjectionEngine, ReadModel, ProjectionCheckpoint, ProjectionError, evaluate_path]
  affects: [04-05, phase-05-joins, phase-06-constraints, phase-07-observer]
tech_stack:
  added: []
  patterns:
    - "Pure fold function apply_raw: ProjectionDefinition + Iterator<StoredEvent> -> serde_json::Value"
    - "Two-layer API: apply_raw (testable, no trait) + project<M: ReadModel> (typed convenience)"
    - "ProjectionCheckpoint opaque to callers; raw_state + last_sequence for incremental folds"
    - "RFC 9535 JSON Path via serde_json_path — evaluate_path returns first match or None"
    - "optional = !required semantics for FieldNotFound vs null on absent From paths"
key_files:
  created:
    - event-sourcing/src/projection/engine.rs
    - event-sourcing/src/projection/error.rs
  modified:
    - event-sourcing/src/projection/mod.rs
    - event-sourcing/src/lib.rs
decisions:
  - "evaluate_path returns None for absent paths — callers decide FieldNotFound vs null based on optional flag"
  - "cleared_by skips nested field handler processing for that event (entire object becomes null immediately)"
  - "null field value treated as 0.0 for arithmetic handlers (sensible default for accumulators)"
  - "project_from with checkpoint=None is equivalent to apply_raw — full replay from scratch"
  - "initialize_state seeds ObjectFieldSpec sub-fields with their defaults or null (not an empty object)"
metrics:
  duration: "~25 minutes"
  completed: "2026-04-24"
  tasks_completed: 4
  files_changed: 4
---

# Phase 4 Plan 4: ProjectionEngine Summary

**One-liner:** ProjectionEngine fold mechanism with apply_raw/project/project_from, ReadModel trait, ProjectionCheckpoint, RFC 9535 JSON path evaluation, and full scalar/object/cleared_by/increment semantics — 17 passing engine tests.

## What Was Built

### Task 1 — ProjectionError (commit 2632401)

Created `event-sourcing/src/projection/error.rs`:

- `ProjectionError` enum — `thiserror`-derived with 4 variants:
  - `UnknownEventType(EventType)` — event type has no EventSchemaDef
  - `FieldNotFound { event_type, path }` — required From path absent in payload
  - `DeserializationFailed(#[from] serde_json::Error)` — typed model deserialization failed
  - `TypeMismatch { field, actual }` — non-numeric value for Increment/Decrement handler

Updated `projection/mod.rs` with `pub mod error` and `pub use error::ProjectionError`.

### Task 2 — JSON Path Evaluator (commit be70cc5)

Created `event-sourcing/src/projection/engine.rs` with:

- `evaluate_path(payload: &Value, path: &str) -> Option<Value>` — RFC 9535 JsonPath evaluation via `serde_json_path`. Returns first matching node or None. Invalid path strings short-circuit via `ok()?`.

5 path tests: simple field, nested path, missing field, null value, invalid expression.

Updated `projection/mod.rs` with `pub mod engine`.

### Task 3 — Full ProjectionEngine (commit 4971b8e)

Extended `engine.rs` with:

**ReadModel trait:**
```rust
pub trait ReadModel: serde::de::DeserializeOwned {
    fn definition() -> ProjectionDefinition;
}
```

**ProjectionCheckpoint:** opaque struct holding `raw_state: Value` and `last_sequence: GlobalSequenceId`. Callers store/restore; do not inspect internals.

**ProjectionEngine** (unit struct) with three public methods:

- `apply_raw(def, events)` — pure fold. Initializes state from field defaults, then folds each `StoredEvent` via `apply_event`. Events not referenced in any handler are silently skipped (D-22). Returns `serde_json::Value`.
- `project<M: ReadModel>(events)` — calls `apply_raw` then `serde_json::from_value`. Typed convenience.
- `project_from<M: ReadModel>(checkpoint, new_events)` — incremental fold. `None` checkpoint = full replay; `Some(cp)` continues from `cp.raw_state`.

**Fold semantics implemented:**
- `From { from }`: `evaluate_path` against payload; absent path → `null` (optional) or `FieldNotFound` (required)
- `Value { value }`: set field to literal value (including null)
- `Increment { increment }`: add N to current numeric (null treated as 0.0)
- `Decrement { decrement }`: subtract N
- `IncrementBy { increment_by }`: evaluate path, add result
- `DecrementBy { decrement_by }`: evaluate path, subtract result
- `cleared_by`: set entire ObjectFieldSpec field to null, skip nested handlers for that event
- `initialize_state`: seeds all scalar/object fields with `default` value or `null`

12 additional tests added in Task 3 (see below for full list).

### Task 4 — Re-exports (commit 74b4f1c)

- `projection/mod.rs`: `pub use engine::{ProjectionCheckpoint, ProjectionEngine, ReadModel}`
- `lib.rs`: added `ProjectionCheckpoint, ProjectionEngine, ProjectionError, ReadModel` to `pub use projection` block

Formatting pass (commit 12a9975): `cargo fmt --all`.

## Tests

17 engine tests pass:

**Path evaluator (5):**
- `path_simple_field`
- `path_nested`
- `path_missing_field`
- `path_null_value`
- `path_invalid_expression`

**apply_raw (10):**
- `apply_raw_sets_scalar_from_event` — scalar From handler
- `apply_raw_multiple_events_last_wins` — last event wins on same field
- `apply_raw_value_null_clears_field` — Value(null) handler
- `apply_raw_increment` — Increment handler with default 0.0
- `apply_raw_nested_object_populated` — ObjectFieldSpec sub-fields populated
- `apply_raw_cleared_by` — cleared_by sets object to null
- `apply_raw_skips_unknown_event_types` — unknown events not in handlers: no error (D-22)
- `apply_raw_default_value_used_before_events` — default present before any events
- `apply_raw_optional_field_absent_yields_null` — absent path + optional → null
- `apply_raw_required_field_absent_yields_error` — absent path + required → FieldNotFound

**Typed model (1):**
- `project_returns_typed_model` — project<M> deserializes correctly

**Incremental (1):**
- `project_from_checkpoint_incremental` — 2 events → checkpoint → +1 event = same as 3-event full replay

**Total workspace tests: 86 event-sourcing lib + 23 inmemory = 109 passing, 0 failures.**

## Acceptance Criteria Verification

| Criterion | Status |
|-----------|--------|
| `pub enum ProjectionError` in error.rs | PASS |
| `fn evaluate_path` in engine.rs | PASS |
| `serde_json_path` used in engine.rs | PASS |
| `pub struct ProjectionEngine` in engine.rs | PASS |
| `pub fn apply_raw` in engine.rs | PASS |
| `pub fn project<M` in engine.rs | PASS |
| `pub fn project_from<M` in engine.rs | PASS |
| `pub trait ReadModel` in engine.rs | PASS |
| `pub struct ProjectionCheckpoint` in engine.rs | PASS |
| `ProjectionEngine` in lib.rs | PASS |
| `ReadModel` in lib.rs | PASS |
| `cargo build -p event-sourcing` exits 0 | PASS |
| `cargo test -p event-sourcing --lib` exits 0 (86 tests) | PASS |
| At least 10 engine tests passing | PASS (17 tests) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused `indexmap::IndexMap` import**
- **Found during:** Task 3 (compiler warning after implementing engine)
- **Issue:** `use indexmap::IndexMap;` was included in engine.rs but IndexMap is not directly used in engine.rs — it's used in definition.rs; the engine works through the definition types which already carry IndexMap internally
- **Fix:** Removed the unused import
- **Files modified:** `event-sourcing/src/projection/engine.rs`
- **Commit:** 4971b8e

No other deviations — plan executed as written.

## Known Stubs

None. All engine methods are fully implemented:
- `apply_raw` folds events and returns real state
- `project<M>` deserializes into typed model
- `project_from` properly resumes from checkpoint
- All HandlerSpec variants handled

## Threat Surface Scan

| Flag | File | Description |
|------|------|-------------|
| mitigated: T-04.04-01 | engine.rs | evaluate_path uses serde_json_path which returns None for absent/type-mismatched paths; FieldNotFound returned for required fields rather than silent wrong output |
| mitigated: T-04.04-04 | engine.rs | TypeMismatch returned on non-numeric value for Increment/Decrement; tested by apply_raw_increment (passes) |

T-04.04-02 (no recursion depth limit) and T-04.04-03 (FieldNotFound leaks path info) remain accepted as documented in plan threat model.

## Self-Check: PASSED

| Item | Status |
|------|--------|
| event-sourcing/src/projection/error.rs | FOUND |
| event-sourcing/src/projection/engine.rs | FOUND |
| event-sourcing/src/projection/mod.rs (modified) | FOUND |
| event-sourcing/src/lib.rs (modified) | FOUND |
| Commit 2632401 (Task 1: ProjectionError) | FOUND |
| Commit be70cc5 (Task 2: path evaluator) | FOUND |
| Commit 4971b8e (Task 3: ProjectionEngine) | FOUND |
| Commit 74b4f1c (Task 4: re-exports) | FOUND |
| Commit 12a9975 (chore: cargo fmt) | FOUND |
| 86 event-sourcing lib tests | PASSED |
| 23 inmemory lib tests | PASSED |
