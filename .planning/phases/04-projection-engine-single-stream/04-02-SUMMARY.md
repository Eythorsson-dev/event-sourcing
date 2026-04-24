---
phase: "04"
plan: "02"
subsystem: event-sourcing-core
tags: [projection, serde, indexmap, builder-api, json-schema]
dependency_graph:
  requires: [04-01]
  provides: [ProjectionDefinition, HandlerSpec, ScalarFieldSpec, ObjectFieldSpec, builder API]
  affects: [04-03, 04-04, 04-05, 04-06]
tech_stack:
  added:
    - indexmap v2.14.0 (serde feature) — insertion-order-preserving map for fields and event handlers
    - serde_json_path v0.7.2 — RFC 9535 JSONPath validation at builder time
  patterns:
    - untagged serde enum for HandlerSpec (inner structs carry deny_unknown_fields)
    - untagged serde enum for FieldSpec (Object tried before Scalar for disambiguation)
    - deny_unknown_fields on ProjectionDefinition, ScalarFieldSpec, ObjectFieldSpec, HandlerSpec inner structs
    - builder pattern with consuming method chaining (mut self -> Self)
    - panic-on-invalid-path at builder call site (programming error, not runtime input)
key_files:
  created:
    - event-sourcing/src/projection/mod.rs
    - event-sourcing/src/projection/definition.rs
  modified:
    - event-sourcing/src/lib.rs
    - Cargo.toml
    - event-sourcing/Cargo.toml
decisions:
  - "HandlerSpec uses #[serde(untagged)] with inner newtype structs (not struct variants) so each variant serializes as a single-key JSON object without a wrapper"
  - "FieldSpec tries Object before Scalar in untagged deserialization — ObjectFieldSpec has required 'type' field that disambiguates"
  - "deny_unknown_fields NOT placed on untagged FieldSpec enum (unsupported in serde, panics); placed on each variant struct instead"
  - "HandlerSpec::from_path/value/increment/etc convenience constructors keep test and callsite ergonomics clean"
  - "ScalarFieldSpecBuilder::on() validates path strings via serde_json_path::JsonPath::parse and panics immediately — paths are authored by developers, not end users"
metrics:
  duration: "~5 minutes"
  completed: "2026-04-24"
  tasks_completed: 3
  files_changed: 5
---

# Phase 4 Plan 2: ProjectionDefinition Types and Builder API Summary

**One-liner:** ProjectionDefinition/HandlerSpec/ScalarFieldSpec/ObjectFieldSpec defined in projection/definition.rs with untagged serde, IndexMap field ordering, deny_unknown_fields protection, and a fully chainable builder API with JSONPath validation.

## What Was Built

`event-sourcing/src/projection/definition.rs` introduces the JSON-serializable projection schema types:

### Types

- `HandlerSpec` enum — 6 operation variants (From, Value, Increment, Decrement, IncrementBy, DecrementBy). Uses `#[serde(untagged)]` with per-variant inner structs (`HandlerFrom`, `HandlerValue`, etc.) each carrying `#[serde(deny_unknown_fields)]`. This produces single-key JSON objects: `{"from":"$.x"}`, `{"value":null}`, `{"increment":5.0}`.
- `ScalarFieldSpec` struct — `required` (default false), optional `default` value, `IndexMap<EventType, HandlerSpec>` events. `#[serde(deny_unknown_fields)]`.
- `ObjectFieldSpec` struct — `field_type` (always "object", serialized as `"type"`), optional `cleared_by` vec, `IndexMap<String, ScalarFieldSpec>` nested fields. `#[serde(deny_unknown_fields)]`.
- `FieldSpec` enum — `#[serde(untagged)]` union of Object and Scalar. Object is tried first since it has a required `"type"` key scalars don't have.
- `ProjectionDefinition` struct — `name`, `query: TagFilter`, `fields: IndexMap<String, FieldSpec>`. `#[serde(deny_unknown_fields)]`.

### Builder API

- `ProjectionDefinitionBuilder::new(name, query)` / `.scalar()` / `.object()` / `.build()`
- `ScalarFieldSpecBuilder::new()` / `.required()` / `.default_value()` / `.on(event_type, handler)` / `.build()`
- `ObjectFieldSpecBuilder::new()` / `.cleared_by()` / `.field()` / `.build()`
- `ProjectionDefinition::builder(name, query)` — convenience entry point

### Path validation

`ScalarFieldSpecBuilder::on()` calls `serde_json_path::JsonPath::parse()` on any path string in `HandlerSpec::From`, `HandlerSpec::IncrementBy`, or `HandlerSpec::DecrementBy`. Invalid paths panic immediately at construction time with a message including the invalid path — programming errors surface in tests rather than silently producing broken projections.

`event-sourcing/src/projection/mod.rs` re-exports all public types from `definition`.

`event-sourcing/src/lib.rs` updated with:
- `pub mod projection;` (alphabetical position between query and schema)
- `pub use projection::{FieldSpec, HandlerSpec, ObjectFieldSpec, ObjectFieldSpecBuilder, ProjectionDefinition, ProjectionDefinitionBuilder, ScalarFieldSpec, ScalarFieldSpecBuilder};`

Workspace `Cargo.toml` and `event-sourcing/Cargo.toml` updated with `indexmap v2` (serde feature) and `serde_json_path v0.7`.

## Tests

9 projection::definition tests pass:

- `handler_from_roundtrip` — `HandlerSpec::from_path("$.amount")` → `{"from":"$.amount"}` → back, assert equality
- `handler_value_null_roundtrip` — `HandlerSpec::value(Value::Null)` → `{"value":null}` → back
- `handler_increment_roundtrip` — `HandlerSpec::increment(5.0)` → `{"increment":5.0}` → back
- `projection_definition_full_roundtrip` — CustomerView example (name + accountant_name + address object with cleared_by) serializes/deserializes without data loss
- `projection_definition_deny_unknown_fields` — `"extra":true` in JSON returns `Err`
- `scalar_deny_unknown` — `"bogus":1` in ScalarFieldSpec JSON returns `Err`
- `insertion_order_preserved` — fields added as `["z","a","m"]` come back in that order after round-trip
- `builder_produces_equivalent_to_manual` — builder output `==` hand-constructed struct literal
- `builder_panics_on_invalid_path` — `"not-a-valid-path"` in HandlerSpec::From panics with message including the path string

Total: 62 tests pass (53 pre-existing + 9 new); 0 failures; clippy clean.

## Acceptance Criteria Verification

- `pub struct ProjectionDefinition` in definition.rs — PASS
- `pub enum HandlerSpec` in definition.rs — PASS
- `pub struct ObjectFieldSpec` in definition.rs — PASS
- `pub struct ScalarFieldSpec` in definition.rs — PASS
- `IndexMap` occurrences (at least 3) — PASS (13 occurrences)
- `deny_unknown_fields` occurrences (at least 2) — PASS (14 occurrences)
- `ProjectionDefinitionBuilder` in definition.rs — PASS (5 occurrences)
- `cargo test -p event-sourcing --lib projection::definition::` exits 0 with 9 tests — PASS
- `pub mod projection;` in lib.rs — PASS
- `pub use projection::{...}` including `ProjectionDefinition` in lib.rs — PASS
- `cargo build -p event-sourcing` exits 0 — PASS
- `cargo test -p event-sourcing --lib` exits 0 (62 tests) — PASS

## Commits

| Hash | Type | Description |
|------|------|-------------|
| 8854cc0 | chore | add indexmap and serde_json_path workspace dependencies |
| d123443 | feat | implement ProjectionDefinition types and builder API |
| 153c86e | feat | re-export ProjectionDefinition types at crate root |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] HandlerSpec untagged serde instead of rename_all**
- **Found during:** Task 2 (TDD GREEN phase — tests failed)
- **Issue:** The plan specified `#[serde(rename_all = "snake_case", deny_unknown_fields)]` on `HandlerSpec` with struct variants (e.g., `From { from: String }`). This produced nested JSON objects: `{"from":{"from":"$.amount"}}` instead of `{"from":"$.amount"}` because serde wraps struct variants in their variant name key, then wraps the fields inside.
- **Fix:** Replaced struct variants with tuple variants wrapping individual newtype structs (`HandlerFrom`, `HandlerValue`, etc.), each with `#[serde(deny_unknown_fields)]`. Used `#[serde(untagged)]` on the enum so each variant serializes as its inner struct — a single-key JSON object. Added `HandlerSpec::from_path()`, `value()`, `increment()`, etc. convenience constructors to keep callsite ergonomics equivalent to the original struct-literal style.
- **Files modified:** `event-sourcing/src/projection/definition.rs`
- **Commit:** d123443

**2. [Rule 1 - Bug] FieldSpec variant order (Object before Scalar)**
- **Found during:** Task 2 implementation
- **Issue:** The plan listed `FieldSpec` as `Scalar(ScalarFieldSpec), Object(ObjectFieldSpec)`. With `#[serde(untagged)]`, serde tries variants in order — if Scalar is tried first, it would match ObjectFieldSpec JSON because ScalarFieldSpec accepts any JSON object (its fields are all optional/defaulted). Object must be tried first since its required `"type"` field causes Scalar deserialization to fail fast.
- **Fix:** Reordered to `Object(ObjectFieldSpec), Scalar(ScalarFieldSpec)`.
- **Files modified:** `event-sourcing/src/projection/definition.rs`
- **Commit:** d123443

## Known Stubs

None — all types are fully implemented and tested.

## Threat Surface Scan

The only security-relevant surface introduced is JSON deserialization of `ProjectionDefinition`, `ScalarFieldSpec`, `ObjectFieldSpec`, and `HandlerSpec`. All are protected per the threat model:

| Flag | File | Description |
|------|------|-------------|
| mitigated: T-04.02-01 | definition.rs | deny_unknown_fields on ProjectionDefinition, ScalarFieldSpec, ObjectFieldSpec, and all HandlerSpec inner structs — tested by `projection_definition_deny_unknown_fields` and `scalar_deny_unknown` |

T-04.02-02 (HandlerSpec::Value accepts any Value — by design) and T-04.02-03 (unbounded field count — accepted) remain as documented.

## Self-Check: PASSED

- `event-sourcing/src/projection/mod.rs` — FOUND
- `event-sourcing/src/projection/definition.rs` — FOUND
- `event-sourcing/src/lib.rs` (modified) — FOUND
- `Cargo.toml` (modified) — FOUND
- `event-sourcing/Cargo.toml` (modified) — FOUND
- Commit `8854cc0` (chore: workspace deps) — FOUND
- Commit `d123443` (feat: definition types) — FOUND
- Commit `153c86e` (feat: re-exports) — FOUND
