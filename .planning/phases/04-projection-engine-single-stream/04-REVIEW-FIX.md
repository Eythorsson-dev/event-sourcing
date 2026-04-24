---
phase: 04-projection-engine-single-stream
fixed_at: 2026-04-24T00:00:00Z
review_path: .planning/phases/04-projection-engine-single-stream/04-REVIEW.md
iteration: 1
fix_scope: critical_warning
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 04: Code Review Fix Report

**Fixed at:** 2026-04-24
**Source review:** `.planning/phases/04-projection-engine-single-stream/04-REVIEW.md`
**Iteration:** 1

**Summary:**
- Findings in scope: 6
- Fixed: 6
- Skipped: 0

## Fixed Issues

### WR-01: `project_from` returns stale checkpoint when new_events is empty

**Files modified:** `event-sourcing/src/projection/engine.rs`
**Commit:** `c70e47e`
**Applied fix:** Restructured the checkpoint extraction so `last_sequence` is initialized from the incoming checkpoint's `last_sequence` rather than unconditionally starting at `GlobalSequenceId::ZERO`. The `match checkpoint` now destructures both `raw_state` and `last_sequence` together: `Some(cp) => (cp.raw_state, cp.last_sequence)`, so an empty `new_events` iterator leaves the sequence correctly at the checkpoint's position.

---

### WR-02: `cleared_by` and sub-field handler conflict silently drops update

**Files modified:** `event-sourcing/src/projection/definition.rs`
**Commit:** `cfb9b7b`
**Applied fix:** Added a conflict-detection loop at the top of `ObjectFieldSpecBuilder::build()`. Before constructing `ObjectFieldSpec`, the loop iterates over each `cleared_by` event type and checks whether that event type also appears in any sub-field handler's `events` map. If a conflict is found, the method panics with a message identifying both the event type and the conflicting sub-field name.

---

### WR-03: `evaluate_path` silently swallows invalid JSON path strings

**Files modified:** `event-sourcing/src/projection/engine.rs`, `event-sourcing/src/projection/error.rs`
**Commit:** `7588e76`
**Applied fix:** Added `ProjectionError::InvalidPath { path: String }` variant to `ProjectionError`. Changed `evaluate_path` signature from `Option<Value>` to `Result<Option<Value>, ProjectionError>` and replaced `.ok()?` with `.map_err(|_| ProjectionError::InvalidPath { path: path.to_owned() })?`. Updated all three production call sites in `apply_handler` to propagate the error with `?`. Updated the five test call sites to `.unwrap()` the `Ok` result. Changed the `path_invalid_expression` test to assert `Err(ProjectionError::InvalidPath { .. })` instead of `None`.

---

### WR-04: `validate_schemas` swallows storage failure with misleading error

**Files modified:** `event-sourcing/src/error.rs`, `event-sourcing/src/event_log.rs`, `event-sourcing/tests/event_log_integration.rs`
**Commit:** `6c0aca1` (error.rs + event_log.rs), `0a04ca2` (integration test exhaustiveness fix bundled with WR-06)
**Applied fix:** Added `StorageFailure(String)` variant to `SchemaConflictError` with display message `"schema store failure during validation: {0}"`. In `validate_schemas`, replaced the fabricated `Conflict` error mapping with `.map_err(|e| SchemaConflictError::StorageFailure(e.to_string()))`. Updated the non-exhaustive `match` in `event_log_integration.rs` to handle the new variant with a `panic!`.

---

### WR-05: `#[derive(Event)]` maps `bool` to `FieldType::String` silently

**Files modified:** `event-sourcing-macros/src/derive_event.rs`
**Commit:** `e8621eb`
**Applied fix:** Replaced the `"bool" => quote! { event_sourcing::FieldType::String }` match arm with an explicit `return Err(Error::new_spanned(...))` that emits a compile-time error: `"#[derive(Event)] does not support \`bool\` fields. Use \`String\` (\"true\"/\"false\") or a newtype instead."`. This surfaces the type mismatch at macro expansion time rather than silently recording a `bool` field as `String` in the schema.

---

### WR-06: `ObjectFieldSpecBuilder` does not validate JSON paths on sub-field handlers

**Files modified:** `event-sourcing/src/projection/definition.rs`
**Commit:** `0a04ca2`
**Applied fix:** Added doc comments to both `ScalarFieldSpec` and `ObjectFieldSpec` structs documenting the path validation gap: path strings in `HandlerSpec::From`, `HandlerSpec::IncrementBy`, and `HandlerSpec::DecrementBy` are only validated when using `ScalarFieldSpecBuilder::on`. Direct struct construction (via struct literal or `serde_json::from_value`) bypasses the check, and invalid paths surface as `ProjectionError::InvalidPath` at fold time. The `ObjectFieldSpec` doc also notes that the `cleared_by`/sub-field conflict detection is builder-only. This is a documentation fix — the runtime behaviour is covered by WR-03 (which now returns `InvalidPath` instead of silently producing `None`).

---

_Fixed: 2026-04-24_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
