---
phase: "04"
plan: "05"
subsystem: event-sourcing-macros
tags: [proc-macro, derive, event-trait, macros-feature]
dependency_graph:
  requires: [04-01]
  provides: [derive(Event) proc macro, macros feature flag]
  affects: [phase-07-observer, phase-10-examples]
tech_stack:
  added: [syn 2.x, quote 1.0, proc-macro2 1.0]
  patterns:
    - "proc-macro crate with proc-macro = true; no circular dep on event-sourcing"
    - "Generated code references event_sourcing:: paths as token streams resolved at call site"
    - "Type string matching via quote!(#ty).to_string() with whitespace stripped"
    - "Option<T> unwrapped by prefix/suffix string detection, sets optional=true on FieldDef"
    - "Error::new_spanned for span-pointing compile errors on unsupported types"
    - "macros feature on core crate re-exports DeriveEvent alias to avoid Event name collision"
key_files:
  created:
    - event-sourcing-macros/Cargo.toml
    - event-sourcing-macros/src/lib.rs
    - event-sourcing-macros/src/derive_event.rs
    - event-sourcing-macros/tests/derive_event_tests.rs
  modified:
    - event-sourcing/Cargo.toml
    - event-sourcing/src/lib.rs
    - Cargo.toml
decisions:
  - "proc-macro crate has no dependency on event-sourcing to avoid circular dep — generated tokens reference event_sourcing:: paths resolved at call site"
  - "Type mapping uses string representation of token stream (not AST pattern matching) for pragmatism — known limitation: type aliases not resolved"
  - "bool maps to FieldType::String (stringified) per v1 simplification from plan"
  - "Re-export as DeriveEvent (not Event) to avoid name collision with the Event trait already in scope"
  - "Integration tests added as dev-dependency on event-sourcing so generated impl can be compiled and verified"
metrics:
  duration: "~20 minutes"
  completed: "2026-04-24"
  tasks_completed: 3
  files_changed: 7
---

# Phase 4 Plan 5: Event Derive Macro Summary

**One-liner:** `event-sourcing-macros` proc-macro crate with `#[derive(Event)]` generating `Event` trait impls from struct field types, plus `macros` feature flag re-export on the core crate.

## What Was Built

### Task 1 — Crate Scaffold (commit 3db59aa)

Created `event-sourcing-macros` as a standalone proc-macro crate:

- `event-sourcing-macros/Cargo.toml` with `proc-macro = true` and `syn`/`quote`/`proc-macro2` workspace deps
- `event-sourcing-macros/src/lib.rs` with `#[proc_macro_derive(Event)]` entry point
- Added `"event-sourcing-macros"` to workspace members in root `Cargo.toml`
- Added `syn = { version = "2", features = ["full"] }`, `quote = "1.0"`, `proc-macro2 = "1.0"` to `[workspace.dependencies]`

No circular dependency: the macros crate does NOT depend on `event-sourcing`. Generated code references `event_sourcing::` token paths resolved at the call site.

### Task 2 — #[derive(Event)] Implementation (commit 2130f1a, fmt e1b7d98)

Created `event-sourcing-macros/src/derive_event.rs`:

**`impl_derive_event`** — parses `DeriveInput` via `syn::parse2`, extracts named struct fields, maps each field type to a `FieldType` token, emits an `impl event_sourcing::Event for StructName` block.

**`map_rust_type_to_field_type`** — converts Rust type token strings to `FieldType` tokens:

| Rust type | FieldType variant |
|-----------|------------------|
| `String`, `std::string::String`, `&str`, `str` | `FieldType::String` |
| `i8`/`i16`/`i32`/`i64`/`i128`/`u8`/`u16`/`u32`/`u64`/`u128`/`usize`/`isize` | `FieldType::Integer` |
| `f32`/`f64` | `FieldType::Decimal` |
| `bool` | `FieldType::String` (stringified, v1) |
| `chrono::NaiveDate` | `FieldType::Date` |
| `chrono::DateTime`, `chrono::DateTime<chrono::Utc>`, `DateTime<Utc>` | `FieldType::DateTime` |
| `Option<T>` | unwrap inner type + `optional: true` |
| anything else | `Error::new_spanned` compile error |

**Integration tests** in `event-sourcing-macros/tests/derive_event_tests.rs` (with `event-sourcing` as dev-dep):
- `derive_event_basic_struct` — String/f64/i32 fields → correct event_type, 3 FieldDef entries
- `derive_event_option_field` — required String + Option<String> → optional flags correct
- `derive_event_all_supported_types` — 9 fields covering all supported type groups

### Task 3 — Macros Feature Flag (commit 7ffcdec)

- `event-sourcing/Cargo.toml`: added `event-sourcing-macros = { path = "../event-sourcing-macros", optional = true }` and `[features] macros = ["event-sourcing-macros"]`
- `event-sourcing/src/lib.rs`: added `#[cfg(feature = "macros")] pub use event_sourcing_macros::Event as DeriveEvent;`

Base build (`cargo build -p event-sourcing`) is unaffected. Feature is opt-in.

## Tests

3 integration tests pass in `event-sourcing-macros`:
- `derive_event_basic_struct`
- `derive_event_option_field`
- `derive_event_all_supported_types`

Full workspace: 131 tests passing (86 event-sourcing lib + 19 event-sourcing integration + 23 inmemory + 3 macros), 0 failures.

## Acceptance Criteria Verification

| Criterion | Status |
|-----------|--------|
| `grep "proc-macro = true" event-sourcing-macros/Cargo.toml` | PASS |
| `grep '"event-sourcing-macros"' Cargo.toml` | PASS |
| `grep "syn" event-sourcing-macros/Cargo.toml` | PASS |
| `cargo build -p event-sourcing-macros` exits 0 | PASS |
| `grep "fn impl_derive_event" derive_event.rs` | PASS |
| `grep "map_rust_type_to_field_type" derive_event.rs` | PASS |
| `grep "Error::new_spanned" derive_event.rs` | PASS |
| `cargo test -p event-sourcing-macros` exits 0 | PASS (3 tests) |
| `grep 'macros.*event-sourcing-macros' event-sourcing/Cargo.toml` | PASS |
| `grep '\[features\]' event-sourcing/Cargo.toml` | PASS |
| `cargo build -p event-sourcing --features macros` exits 0 | PASS |
| `cargo build -p event-sourcing` exits 0 | PASS |
| `cargo test -p event-sourcing --lib` exits 0 | PASS (86 tests) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Added `use event_sourcing::Event as _` import in integration tests**
- **Found during:** Task 2 GREEN phase
- **Issue:** Calling `OrderPlaced::event_type()` in tests failed with "items from traits can only be used if the trait is in scope" — the `Event` trait must be imported to use trait methods
- **Fix:** Added `use event_sourcing::Event as _` to bring trait into scope without naming conflict
- **Files modified:** `event-sourcing-macros/tests/derive_event_tests.rs`
- **Commit:** 2130f1a

No other deviations — plan executed as written.

## Known Stubs

None. The derive macro fully generates `event_type()` and `schema()` impls. All type mappings are implemented per the plan's interface table.

## Threat Surface Scan

No new network endpoints, auth paths, or file access patterns introduced. The proc-macro runs at compile time only — no runtime surface. Threat mitigations per plan:

| Threat | Mitigation | Status |
|--------|-----------|--------|
| T-04.05-01 — DoS via infinite loop on malformed input | `syn::parse2` returns `Result` — all errors surface as `syn::Error` → `compile_error!`, never panic/loop | MITIGATED |
| T-04.05-02 — Elevation of privilege via generated code | Generated code only calls `event_sourcing::FieldType::*`, `FieldDef`, `EventSchemaDef` — all public API | ACCEPTED |
| T-04.05-03 — Info disclosure via type name in compile error | Intentional for debuggability; type names in compile errors are not sensitive | ACCEPTED |

## Self-Check: PASSED

| Item | Status |
|------|--------|
| event-sourcing-macros/Cargo.toml | FOUND |
| event-sourcing-macros/src/lib.rs | FOUND |
| event-sourcing-macros/src/derive_event.rs | FOUND |
| event-sourcing-macros/tests/derive_event_tests.rs | FOUND |
| event-sourcing/Cargo.toml (modified) | FOUND |
| event-sourcing/src/lib.rs (modified) | FOUND |
| Cargo.toml (modified) | FOUND |
| Commit 3db59aa (Task 1: scaffold) | FOUND |
| Commit 2130f1a (Task 2: implementation) | FOUND |
| Commit 7ffcdec (Task 3: feature flag) | FOUND |
| 3 macros integration tests | PASSED |
| 86 event-sourcing lib tests | PASSED |
| 131 total workspace tests | PASSED |
