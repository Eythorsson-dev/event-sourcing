---
status: complete
phase: 04-projection-engine-single-stream
source:
  - 04-01-SUMMARY.md
  - 04-02-SUMMARY.md
  - 04-03-SUMMARY.md
  - 04-04-SUMMARY.md
  - 04-05-SUMMARY.md
  - 04-06-SUMMARY.md
started: "2026-04-24T00:00:00Z"
updated: "2026-04-24T12:00:00Z"
---

## Current Test

[testing complete]

## Tests

### 1. Full workspace test suite passes
expected: Running `cargo test --workspace --features macros` completes with all 139 tests passing and 0 failures. The output ends with "test result: ok. N passed; 0 failed".
result: pass

### 2. ProjectionDefinition builder API produces correct JSON
expected: |
  Using the builder API to construct a ProjectionDefinition with a scalar field and an object field,
  then calling `serde_json::to_string(&def)` produces valid JSON. Calling `serde_json::from_str` on that
  JSON string and comparing to the original produces an equal struct (round-trip without data loss).
  Unknown fields in JSON (e.g., `"extra": true`) cause deserialization to return `Err`.
result: pass

### 3. ProjectionEngine folds events into a typed result
expected: |
  Given a sequence of `StoredEvent` values and a `ProjectionDefinition` with scalar handlers,
  calling `ProjectionEngine::apply_raw(&def, events.iter())` returns a `serde_json::Value` JSON object
  reflecting the last-write-wins fold semantics. Calling `ProjectionEngine::project::<M>(events.iter())`
  (where M implements ReadModel) deserializes the result into the typed struct correctly.
result: pass

### 4. Incremental projection from checkpoint
expected: |
  Replaying 2 events to get a checkpoint, then calling `ProjectionEngine::project_from(Some(checkpoint), &[event3])`
  produces the same result as a full replay of all 3 events from scratch (no checkpoint).
  The checkpoint is opaque — callers cannot inspect its internals.
result: pass

### 5. #[derive(Event)] macro generates Event trait impl
expected: |
  A struct annotated with `#[derive(DeriveEvent)]` (with `event-sourcing` in scope via `--features macros`)
  automatically implements `event_type() -> &'static str` (the struct name) and `schema() -> EventSchemaDef`
  with FieldDef entries matching the struct's field types. The macro compiles without errors for all
  supported field types: String, i32/i64, f32/f64, Option<T>.
result: pass

### 6. projection! DSL macro generates working read model end-to-end
expected: |
  Writing a `projection! { projection CustomerView { query tag.starts_with("customer:") as c ... } }` block
  generates a `CustomerView` struct and `impl ReadModel for CustomerView`. Running
  `ProjectionEngine::project::<CustomerView>(events.iter())` over a sequence of tagged StoredEvents
  returns a `CustomerView` instance with the expected field values (scalar last-wins, object cleared by
  a clearing event, optional field set to None by a Value(null) handler).
result: pass

### 7. Schema conflict detection at startup
expected: |
  Given an `EventLog` created via `EventLog::with_schema_store(store, schema_store)`, calling
  `event_log.validate_schemas([schema_v1, schema_v2])` where schema_v1 matches the persisted schema but
  schema_v2 has a field type changed returns `Err(SchemaConflictError::Conflict { ... })` identifying
  the conflicting event type. When all code schemas match stored schemas, `validate_schemas` returns `Ok(())`.
result: pass

### 8. deny_unknown_fields protects deserialization boundaries
expected: |
  Passing JSON with an unknown top-level key to `serde_json::from_str::<ProjectionDefinition>` returns
  `Err(...)`. Same for `ScalarFieldSpec`, `ObjectFieldSpec`, and `EventSchemaDef` — unknown fields are
  rejected, not silently ignored.
result: pass

## Summary

total: 8
passed: 8
issues: 0
pending: 0
skipped: 0

## Gaps

[none yet]
