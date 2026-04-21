---
phase: "04"
plan: "01"
subsystem: event-sourcing-core
tags: [schema, serde, event-trait, field-type]
dependency_graph:
  requires: []
  provides: [FieldType, FieldDef, EventSchemaDef, Event trait]
  affects: [04-02, 04-03, 04-05]
tech_stack:
  added: []
  patterns: [deny_unknown_fields, HashSet field compatibility, serde transparent EventType]
key_files:
  created:
    - event-sourcing/src/schema.rs
  modified:
    - event-sourcing/src/lib.rs
decisions:
  - "FieldType uses serde string discriminants (e.g. \"String\", \"Integer\") matching D-01/D-02"
  - "is_compatible_with builds HashSet<(&str, &FieldType, bool)> triples for order-independent comparison (D-07)"
  - "deny_unknown_fields on both FieldDef and EventSchemaDef per T-04.01-01 threat mitigation"
metrics:
  duration: "~10 minutes"
  completed: "2026-04-21"
  tasks_completed: 2
  files_changed: 2
---

# Phase 4 Plan 1: Schema Primitives Summary

**One-liner:** FieldType/FieldDef/EventSchemaDef/Event trait defined in schema.rs with strict HashSet-based compatibility and deny_unknown_fields serde protection.

## What Was Built

`event-sourcing/src/schema.rs` introduces the core schema primitives that all downstream Phase 4 plans depend on:

- `FieldType` enum — 5 variants (String, Integer, Decimal, Date, DateTime), serializes as JSON string discriminants
- `FieldDef` struct — name, field_type, optional flag; `#[serde(deny_unknown_fields)]` rejects tampered payloads
- `EventSchemaDef` struct — event_type (EventType newtype) + Vec<FieldDef>; `#[serde(deny_unknown_fields)]`
- `EventSchemaDef::is_compatible_with` — strict equality via HashSet of (name, field_type, optional) triples; field ordering ignored, optional flag changes are conflicts (D-07)
- `Event` trait — `fn event_type() -> &'static str` and `fn schema() -> EventSchemaDef`

`event-sourcing/src/lib.rs` updated with:
- `pub mod schema;` (alphabetical between query and store)
- `pub use schema::{Event, EventSchemaDef, FieldDef, FieldType};`

## Tests

10 schema tests pass:
- `fieldtype_serde_roundtrip` — all 5 variants serialize/deserialize as JSON strings
- `eventschemadef_serde_roundtrip` — 3-field schema round-trips without data loss
- `eventschemadef_deny_unknown_fields` — unknown JSON keys return Err
- `is_compatible_with_true_when_equal` — identical schemas are compatible
- `is_compatible_with_true_when_fields_reordered` — reordering Vec is not a conflict
- `is_compatible_with_false_when_field_added` — extra field is a conflict
- `is_compatible_with_false_when_field_type_changed` — type change is a conflict
- `is_compatible_with_false_when_event_type_differs` — different event_type is incompatible
- `is_compatible_with_false_when_optional_changed` — changing optional flag is a conflict
- `event_trait_manual_impl_compiles` — manual Event impl verifies trait surface

Total: 53 tests pass (43 pre-existing + 10 new); 0 failures.

## Acceptance Criteria Verification

- `event-sourcing/src/schema.rs` exists — PASS
- `pub enum FieldType` — PASS
- All 5 FieldType variants present — PASS (String, Integer, Decimal, Date, DateTime)
- `pub struct FieldDef` — PASS
- `pub struct EventSchemaDef` — PASS
- `pub trait Event` — PASS
- `is_compatible_with` method — PASS
- `deny_unknown_fields` on at least 2 structs — PASS (FieldDef + EventSchemaDef)
- `cargo test -p event-sourcing --lib schema::` exits 0 with 10 passing tests — PASS
- `pub mod schema;` in lib.rs — PASS
- `pub use schema::{Event, EventSchemaDef, FieldDef, FieldType};` in lib.rs — PASS
- `cargo build -p event-sourcing` exits 0 — PASS
- `cargo test -p event-sourcing --lib` exits 0 — PASS

## Deviations from Plan

None — plan executed exactly as written.

## Known Stubs

None.

## Threat Surface Scan

No new network endpoints, auth paths, file access patterns, or schema changes at trust boundaries. The only security-relevant surface introduced is JSON deserialization of `EventSchemaDef` and `FieldDef`, both protected by `#[serde(deny_unknown_fields)]` per T-04.01-01.

## Self-Check: PASSED

- `event-sourcing/src/schema.rs` — FOUND
- `event-sourcing/src/lib.rs` (modified) — FOUND
- Commit `aef820e` (test: failing tests) — FOUND
- Commit `4539011` (feat: schema implementation) — FOUND
- Commit `d2fc7e6` (feat: re-export at crate root) — FOUND
