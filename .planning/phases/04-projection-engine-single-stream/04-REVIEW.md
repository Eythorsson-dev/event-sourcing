---
phase: 04-projection-engine-single-stream
reviewed: 2026-04-24T00:00:00Z
depth: standard
files_reviewed: 16
files_reviewed_list:
  - event-sourcing/src/projection/definition.rs
  - event-sourcing/src/projection/engine.rs
  - event-sourcing/src/projection/error.rs
  - event-sourcing/src/projection/mod.rs
  - event-sourcing/src/schema_store.rs
  - event-sourcing/src/event_log.rs
  - event-sourcing/src/error.rs
  - event-sourcing/src/lib.rs
  - event-sourcing/src/schema.rs
  - event-sourcing-logstore-inmemory/src/lib.rs
  - event-sourcing-macros/src/lib.rs
  - event-sourcing-macros/src/derive_event.rs
  - event-sourcing-macros/src/projection_macro.rs
  - event-sourcing/tests/projection_e2e.rs
  - event-sourcing/tests/projection_tests.rs
  - event-sourcing/tests/event_log_integration.rs
findings:
  critical: 0
  warning: 6
  info: 5
  total: 11
status: issues_found
---

# Phase 04: Code Review Report

**Reviewed:** 2026-04-24
**Depth:** standard
**Files Reviewed:** 16
**Status:** issues_found

## Summary

The projection engine fold logic is well-structured and the test coverage for the happy paths is thorough. The `apply_raw` / `apply_event` / `apply_handler` call chain is clean and the incremental checkpoint approach is sound. No critical security or data-loss issues were found.

Six warnings cover logic correctness gaps: a stale checkpoint when the new-event iterator is empty, a `cleared_by` rebuild race, a panic reachable from deserialized payloads in `evaluate_path`, an incorrect schema-error fallback that swallows the real cause, a `bool`→`String` mapping in the derive macro that silently misrepresents the field type, and an unvalidated JSON path in `ObjectFieldSpecBuilder`. Five info items cover dead code, missing error variants, and test coverage gaps.

---

## Warnings

### WR-01: `project_from` returns stale checkpoint when new_events is empty

**File:** `event-sourcing/src/projection/engine.rs:92`
**Issue:** `last_sequence` is initialized to `GlobalSequenceId::ZERO` before the event loop. When `new_events` is empty the loop body never executes, so the returned `ProjectionCheckpoint::last_sequence` is `ZERO` regardless of what the previous checkpoint held. A caller that stores the new checkpoint to resume incremental folds will believe the last processed position was 0, causing a full replay on the next call.

**Fix:** Preserve the checkpoint's `last_sequence` when no new events are processed. Extract the initial sequence from the incoming checkpoint before consuming it:

```rust
pub fn project_from<M: ReadModel>(
    checkpoint: Option<ProjectionCheckpoint>,
    new_events: impl Iterator<Item = StoredEvent>,
) -> Result<(M, ProjectionCheckpoint), ProjectionError> {
    let def = M::definition();
    let (mut state, mut last_sequence) = match checkpoint {
        Some(cp) => (cp.raw_state, cp.last_sequence),   // preserve position
        None => {
            let mut s = Value::Object(Map::new());
            Self::initialize_state(&def, &mut s);
            (s, GlobalSequenceId::ZERO)
        }
    };
    for event in new_events {
        last_sequence = event.global_sequence;
        Self::apply_event(&def, &event, &mut state)?;
    }
    let model: M = serde_json::from_value(state.clone())?;
    Ok((model, ProjectionCheckpoint { raw_state: state, last_sequence }))
}
```

---

### WR-02: `cleared_by` and sub-field handler on the same event type rebuilds object to partial state

**File:** `event-sourcing/src/projection/engine.rs:156-194`
**Issue:** In `apply_event`, when an event type is listed in both `cleared_by` *and* as a handler for one or more sub-fields, the code hits the `cleared_by` branch (`continue`) and the sub-field handlers are never applied — which is the documented intent. However if the reverse order is used in the definition (sub-field handler registered *before* the `cleared_by` is listed), the current code is still correct because it checks `cleared_by` first. The real hazard is subtler: if a caller constructs an `ObjectFieldSpec` where the *same* event type appears in *both* `cleared_by` and a sub-field handler, the `cleared_by` branch silently wins and the sub-field update is dropped with no error or warning. This is a silent logic conflict that is easy to introduce accidentally, especially via the builder.

**Fix:** Add a validation step in `ObjectFieldSpecBuilder::build()` (or as a standalone `validate` method on `ProjectionDefinition`) that detects and errors on any event type that appears in both `cleared_by` and a sub-field handler:

```rust
pub fn build(self) -> ObjectFieldSpec {
    // Detect conflicting event registrations
    for evt in &self.cleared_by {
        for (_, scalar) in &self.fields {
            if scalar.events.contains_key(evt) {
                panic!(
                    "event '{}' appears in both cleared_by and a sub-field handler; \
                     this is ambiguous — remove it from one",
                    evt
                );
            }
        }
    }
    ObjectFieldSpec {
        field_type: "object".to_owned(),
        cleared_by: self.cleared_by,
        fields: self.fields,
    }
}
```

---

### WR-03: `evaluate_path` silently swallows invalid JSON path strings at runtime

**File:** `event-sourcing/src/projection/engine.rs:14-17`
**Issue:** `evaluate_path` calls `JsonPath::parse(path).ok()?` — if the path string is malformed it returns `None` instead of an error. The builder (`ScalarFieldSpecBuilder::on`) validates paths at construction time and panics on invalid input. However `ProjectionDefinition` is also `Deserialize`: a definition loaded from JSON (e.g., a stored definition retrieved from a database) bypasses all builder validation. A deserialized `HandlerSpec::From { from: "not-a-path" }` would silently produce `None` from `evaluate_path`, which on a required field triggers `FieldNotFound`, but on an optional field silently sets the field to `null` with no indication that the path was invalid rather than absent.

**Fix:** Return a distinct error instead of silently eating the parse failure:

```rust
pub(crate) fn evaluate_path(payload: &Value, path: &str) -> Result<Option<Value>, ProjectionError> {
    let compiled = JsonPath::parse(path).map_err(|_| ProjectionError::InvalidPath {
        path: path.to_owned(),
    })?;
    Ok(compiled.query(payload).first().cloned())
}
```

Add a corresponding `InvalidPath { path: String }` variant to `ProjectionError`. This also requires updating all call sites to propagate the error.

---

### WR-04: `validate_schemas` swallows storage failure and returns a misleading `SchemaConflictError::Conflict`

**File:** `event-sourcing/src/event_log.rs:127-139`
**Issue:** When `schema_store.fetch_all()` returns an error, the code maps it to a fabricated `SchemaConflictError::Conflict` with `event_type: "unknown"` and empty `EventSchemaDef`s. The caller receives what looks like a real schema conflict but with meaningless fields. The storage failure is silently discarded.

```rust
self.schema_store
    .fetch_all()
    .await
    .map_err(|_e| SchemaConflictError::Conflict {
        event_type: EventType::from("unknown"),  // fabricated
        expected: EventSchemaDef { event_type: EventType::from(""), fields: vec![] },
        actual: EventSchemaDef { event_type: EventType::from(""), fields: vec![] },
    })?;
```

**Fix:** Add a `StorageFailure` variant to `SchemaConflictError`, or change the return type signature to include both error kinds:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SchemaConflictError {
    #[error("schema conflict for event type '{event_type}': persisted schema differs from compiled schema")]
    Conflict { event_type: EventType, expected: EventSchemaDef, actual: EventSchemaDef },

    #[error("schema store failure during validation: {0}")]
    StorageFailure(String),
}
```

Then in `validate_schemas`:
```rust
.map_err(|e| SchemaConflictError::StorageFailure(e.to_string()))?;
```

---

### WR-05: `#[derive(Event)]` maps `bool` to `FieldType::String` silently

**File:** `event-sourcing-macros/src/derive_event.rs:79`
**Issue:** The `map_rust_type_to_field_type` function maps `bool` to `FieldType::String`. A `bool` field in an event struct will be recorded as a `String` field in the `EventSchemaDef`, which is factually wrong. This means a `bool` field and a genuine `String` field become indistinguishable in the schema store, breaking schema compatibility checks. A downstream schema conflict that should be detected (changing a field from `bool` to `String`) will not be caught.

```rust
"bool" => quote! { event_sourcing::FieldType::String },  // wrong
```

**Fix:** Either add a `FieldType::Boolean` variant to match the full Rust type set, or reject `bool` at macro expand time with a clear error:

```rust
"bool" => {
    return Err(Error::new_spanned(
        span_target,
        "#[derive(Event)] does not support `bool` fields. Use `String` (\"true\"/\"false\") or a newtype instead.",
    ))
}
```

The second option is safer until `FieldType::Boolean` is an intended part of the schema.

---

### WR-06: `ObjectFieldSpecBuilder` does not validate JSON paths on sub-field handlers

**File:** `event-sourcing/src/projection/definition.rs:296-309`
**Issue:** `ScalarFieldSpecBuilder::on` validates JSON path strings at call time (line 253) and panics on invalid paths. `ObjectFieldSpecBuilder::field` accepts a fully-built `ScalarFieldSpec` directly, bypassing the validation in `on`. However a user could also call `ScalarFieldSpecBuilder::build()` and pass the resulting struct directly to `ObjectFieldSpecBuilder::field` after constructing the `ScalarFieldSpec` via serde deserialization or struct literal. This is a minor gap: the main builder path is safe, but the raw struct constructor has no guard. The issue is lower severity than WR-03 because it only affects the uncommon direct-construction path.

**Fix:** Document on `ObjectFieldSpec` (and `ScalarFieldSpec`) that path strings are not re-validated if the struct is constructed directly, and that callers must use the builder for validation guarantees. Alternatively, add a `validate()` method to `ProjectionDefinition` that checks all paths using `JsonPath::parse`.

---

## Info

### IN-01: `ProjectionError::UnknownEventType` variant is dead code

**File:** `event-sourcing/src/projection/error.rs:6-7`
**Issue:** The `UnknownEventType(EventType)` variant is defined and has a display impl but is never constructed anywhere in the engine. The engine silently ignores unregistered event types (design decision D-22). This variant misleads readers into thinking the engine can raise it.

**Fix:** Remove `UnknownEventType` from `ProjectionError`, or add a `#[allow(dead_code)]` attribute with a comment explaining it is reserved for future use when schema validation is wired into `apply_event`.

---

### IN-02: `HandlerFrom.from` / `HandlerIncrementBy.increment_by` / `HandlerDecrementBy.decrement_by` are fully public with no validation contract

**File:** `event-sourcing/src/projection/definition.rs:12-49`
**Issue:** All handler inner structs have `pub` fields. A user reading JSON into a `HandlerSpec::From { from: "bad" }` directly (e.g., via `serde_json::from_value`) bypasses path validation entirely. The public fields make this easy to do accidentally.

**Fix:** Consider making the inner struct fields `pub(crate)` and exposing read-only access through methods if external consumers only need to inspect the path string. This narrows the surface where unvalidated paths can be introduced.

---

### IN-03: `projection!` macro generates `String` for all non-numeric scalar fields regardless of the `HandlerSpec::Value` operation

**File:** `event-sourcing-macros/src/projection_macro.rs:476-479`
**Issue:** The macro generates `pub field: String` (or `Option<String>`) for every non-numeric scalar, even when the field's handlers use only `HandlerSpec::value(...)` with a non-string JSON value (e.g., a boolean or number). The generated struct may fail to deserialize or silently coerce the value. This is an inherent limitation of the macro's type inference and should be documented.

**Fix:** Add a doc comment above the generated struct explaining that field types in the generated struct are inferred heuristically (`f64` for increment/decrement fields, `String` otherwise) and that users who need different types (e.g., `bool`, `u64`) should implement `ReadModel` manually instead of using the macro.

---

### IN-04: Commented-out description in `apply_event` creates ambiguity about the rebuild-after-clear path

**File:** `event-sourcing/src/projection/engine.rs:163`
**Issue:** The inline comment `// Still process nested field handlers below even after a clear? / // No — cleared_by semantics: the entire object is null; skip sub-fields.` is a reasoning trail left from development. It reads as a resolved question but will confuse future readers who wonder whether the behaviour was deliberately chosen or left unfinished.

**Fix:** Replace with a single-line positive statement:
```rust
// cleared_by: entire object becomes null; sub-field handlers are intentionally skipped.
continue;
```

---

### IN-05: No test for `project_from` when `new_events` is empty with an existing checkpoint

**File:** `event-sourcing/src/projection/engine.rs` (test block)
**Issue:** The `project_from_checkpoint_incremental` test always passes at least one new event to the incremental call. The bug described in WR-01 (stale `last_sequence`) is not caught by any existing test because no test exercises the empty-new-events path on an existing checkpoint.

**Fix:** Add a test:
```rust
#[test]
fn project_from_empty_new_events_preserves_checkpoint_sequence() {
    // ... build checkpoint at seq 3
    let (_, new_cp) = ProjectionEngine::project_from::<CounterModel>(
        Some(checkpoint_at_3),
        std::iter::empty(),
    ).unwrap();
    // last_sequence must remain 3, not revert to ZERO
    assert_eq!(new_cp.last_sequence, GlobalSequenceId::new(3));
}
```

---

_Reviewed: 2026-04-24_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
