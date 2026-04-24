---
phase: "04"
plan: "06"
subsystem: event-sourcing-macros, event-sourcing-core
tags: [proc-macro, dsl, projection, read-model, e2e-test, macros-feature]
dependency_graph:
  requires: [04-02, 04-04, 04-05]
  provides: [projection! DSL macro, CustomerView e2e test, macros feature re-export]
  affects: [phase-05-joins, phase-06-constraints, phase-07-observer, phase-10-examples]
tech_stack:
  added: []
  patterns:
    - "Custom grammar parser via syn::parse::ParseBuffer (not Rust syntax)"
    - "syn::custom_keyword! for DSL keywords: projection, query, tag, starts_with, ends_with, equals, cleared_by"
    - "ParseBuffer helper functions (not &ParseStream — ParseStream is already a reference)"
    - "Code generator emits companion structs for object fields, main struct with Deserialize, ReadModel impl"
    - "proc_macro re-export via pub use event_sourcing_macros::projection in event-sourcing core under macros feature"
key_files:
  created:
    - event-sourcing-macros/src/projection_macro.rs
    - event-sourcing/tests/projection_tests.rs
    - event-sourcing/tests/projection_e2e.rs
  modified:
    - event-sourcing-macros/src/lib.rs
    - event-sourcing/src/lib.rs
    - event-sourcing/src/projection/engine.rs
    - event-sourcing-macros/tests/derive_event_tests.rs
decisions:
  - "Tests placed in event-sourcing/tests/ not event-sourcing-macros/tests/ — proc-macro re-exports via use are not usable in the defining crate's own integration tests without circular dep"
  - "alias field removed from ProjectionInput struct — only needed during parsing, not code generation"
  - "ParseBuffer used in helper function signatures instead of &ParseStream — ParseStream is a type alias for &ParseBuffer, so &ParseStream would be &&ParseBuffer"
  - "last_sequence field annotated #[allow(dead_code)] — opaque to callers per D-20, written by engine but not read outside tests"
  - "derive_event_tests.rs test structs annotated #[allow(dead_code)] — fields only accessed via schema() derive output, not direct construction"
metrics:
  duration: "~9 minutes"
  completed: "2026-04-24"
  tasks_completed: 3
  files_changed: 7
---

# Phase 4 Plan 6: projection! DSL Macro and End-to-End Integration Test Summary

**One-liner:** `projection!` DSL macro (Proposal H syntax) using a custom `syn` grammar parser that emits `#[derive(Deserialize)]` structs and `impl ReadModel`, completing the full Phase 4 loop: macro → ProjectionEngine::project → typed result.

## What Was Built

### Task 1 — projection! Macro Implementation (commit 18ea776, style beec4dd)

Created `event-sourcing-macros/src/projection_macro.rs` with a custom grammar parser and code generator:

**Parser** (`syn::parse::ParseBuffer` — not Rust syntax):
- `syn::custom_keyword!` macros for: `projection`, `query`, `tag`, `starts_with`, `ends_with`, `equals`, `cleared_by`
- `ProjectionInput` struct: `name`, `query: QuerySpec`, `fields: Vec<FieldDecl>`
- `QuerySpec` enum: `StartsWith(String)`, `EndsWith(String)`, `Equals(String)`
- `FieldDecl` enum: `Scalar { name, optional, handlers }` or `Object { name, optional, sub_fields, cleared_by_event }`
- `HandlerDecl`: `event_type: String` + `OperationDecl` (FromPath, ValueNull, ValueString, IncrementBy, DecrementBy)
- Alias validation: handlers referencing a different alias than the query alias produce `Error::new_spanned` compile errors
- `parse_dot_path`: multi-segment path `name.city` → `"$.name.city"` with fork-based lookahead
- `|+` / `|-` operator parsing for increment/decrement handlers

**Code generator** (`generate_code`):
- For each `FieldDecl::Object`: emits a companion struct `{Name}{FieldPascal}` with `#[derive(Debug, serde::Deserialize)]` and `Option<String>` sub-fields
- For the main struct: `#[derive(Debug, serde::Deserialize)] pub struct {Name}` with correct field types (`String`, `Option<String>`, `f64`, `Option<f64>` for increment fields)
- `impl event_sourcing::ReadModel for {Name}`: calls `ProjectionDefinition::builder(...)` then `.scalar()` / `.object()` chains using the builder API from Plan 02
- Tag filter codegen: `TagFilter::StartsWith`, `EndsWith`, `Equals` all handled

Added `#[proc_macro] pub fn projection(input: TokenStream) -> TokenStream` to `event-sourcing-macros/src/lib.rs`.

**Integration tests** in `event-sourcing/tests/projection_tests.rs` (5 tests, run with `--features macros`):
- `projection_macro_generates_struct`: basic struct, `Foo::definition().name == "Foo"`
- `projection_macro_optional_field`: `required_name` has `required=true`, `optional_name?` has `required=false`
- `projection_macro_object_field`: object block generates companion struct, `FieldSpec::Object` with correct sub-fields
- `projection_macro_cleared_by`: `cleared_by q.AddressCleared` → `ObjectFieldSpec.cleared_by = ["AddressCleared"]`
- `projection_macro_definition_matches_builder`: full CustomerViewMacro macro output `==` manually built `ProjectionDefinition` (T-04.06-02 mitigation)

**Note on test placement:** The plan called for tests in `event-sourcing-macros/tests/`. These were placed in `event-sourcing/tests/` instead because `projection!` is a proc-macro re-exported through `event-sourcing --features macros`, and proc-macro crates cannot use their own macros via `use` in integration tests without creating a circular dependency.

### Task 2 — Re-export projection! from core macros feature (commit 18ea776)

Added to `event-sourcing/src/lib.rs`:
```rust
#[cfg(feature = "macros")]
pub use event_sourcing_macros::projection;
```

Alongside the existing `DeriveEvent` re-export. The `macros` feature in `event-sourcing/Cargo.toml` already has `event-sourcing-macros` as an optional dependency. No new dependencies needed.

### Task 3 — End-to-end integration test (commit 190971a)

Created `event-sourcing/tests/projection_e2e.rs` with the full CustomerView scenario:

```rust
projection! {
    projection CustomerView {
        query tag.starts_with("customer:") as c
        name:  c.CustomerRegistered.name | c.CustomerRenamed.name
        accountant_name?:  c.AccountantAssigned.name | c.AccountantRemoved = null
        address? {
            city:        c.CustomerRegistered.city | c.AddressChanged.city
            postal_code: c.CustomerRegistered.postal_code | c.AddressChanged.postal_code
        } cleared_by c.AddressCleared
    }
}
```

3 e2e tests:
- `e2e_projection_scalar_fields`: CustomerRegistered → CustomerRenamed → `name = "Alicia"` (last wins), `address.city = "London"`, `accountant_name = None`
- `e2e_projection_clear_object`: CustomerRegistered → AddressCleared → `address = None`
- `e2e_projection_accountant_assigned_then_removed`: AccountantAssigned → AccountantRemoved → `accountant_name = None` (Value(null) clears to None)

## Tests

| Suite | Count | Command |
|-------|-------|---------|
| event-sourcing lib (unit) | 86 | `cargo test -p event-sourcing --lib` |
| event-sourcing integration (event_log) | 19 | `cargo test -p event-sourcing --test event_log_integration` |
| projection macro tests | 5 | `cargo test -p event-sourcing --features macros --test projection_tests` |
| projection e2e tests | 3 | `cargo test -p event-sourcing --features macros --test projection_e2e` |
| inmemory logstore | 23 | `cargo test -p event-sourcing-logstore-inmemory` |
| event-sourcing-macros | 3 | `cargo test -p event-sourcing-macros` |
| **Total** | **139** | `cargo test --workspace --features macros` |

All 139 tests pass. `cargo clippy --workspace --all-targets --features macros -- -D warnings` exits 0.

## Acceptance Criteria Verification

| Criterion | Status |
|-----------|--------|
| `fn impl_projection_macro` in projection_macro.rs | PASS |
| `pub fn projection` in lib.rs | PASS |
| `Error::new_spanned` in projection_macro.rs (human-readable errors) | PASS |
| `cargo test -p event-sourcing-macros` exits 0 | PASS (3 tests) |
| `event_sourcing_macros::projection` in event-sourcing/src/lib.rs | PASS |
| `cargo build -p event-sourcing --features macros` exits 0 | PASS |
| `event-sourcing/tests/projection_e2e.rs` exists | PASS |
| `e2e_projection_scalar_fields` test present | PASS |
| `e2e_projection_clear_object` test present | PASS |
| 3 e2e tests pass | PASS |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test placement: projection_tests.rs placed in event-sourcing/tests/ not event-sourcing-macros/tests/**
- **Found during:** Task 1 TDD RED phase
- **Issue:** The plan specified integration tests in `event-sourcing-macros/tests/projection_tests.rs` using `use event_sourcing::projection`. This creates a circular dependency: `event-sourcing-macros` would need `event-sourcing` as a dev-dep with the `macros` feature, which depends back on `event-sourcing-macros`. Cargo rejects this circular dep. Additionally, proc-macro crates cannot use their own `#[proc_macro]` functions via `use` in integration tests.
- **Fix:** Placed tests in `event-sourcing/tests/projection_tests.rs` with `--features macros`. The tests use `event_sourcing::projection` which is a valid re-export path. This is the correct location for macro integration tests that verify generated code compiles and runs.
- **Files modified:** `event-sourcing/tests/projection_tests.rs` (created), NOT `event-sourcing-macros/tests/`
- **Commits:** 18ea776, beec4dd

**2. [Rule 1 - Bug] ParseBuffer in helper functions instead of &ParseStream**
- **Found during:** Task 1 implementation (first build attempt)
- **Issue:** Initial code used `&ParseStream` in helper function signatures. `ParseStream` is a type alias `type ParseStream<'a> = &'a ParseBuffer<'a>` — so `&ParseStream` would be `&&ParseBuffer`, causing type mismatch errors.
- **Fix:** Changed all helper function signatures to accept `&ParseBuffer` directly.
- **Files modified:** `event-sourcing-macros/src/projection_macro.rs`
- **Commit:** 18ea776

**3. [Rule 2 - Missing critical functionality] Clippy fixes for pre-existing dead_code warnings**
- **Found during:** Post-task clippy run
- **Issue:** `cargo clippy --workspace --all-targets -- -D warnings` failed on: (a) `derive_event_tests.rs` test structs with unread fields, (b) `ProjectionCheckpoint::last_sequence` declared but not read outside tests, (c) unused `alias` field in `ProjectionInput` struct.
- **Fix:** Added `#[allow(dead_code)]` on test structs in `derive_event_tests.rs`, added `#[allow(dead_code)]` on `last_sequence` field (opaque checkpoint field per D-20), removed unused `alias` field from `ProjectionInput` struct.
- **Files modified:** `event-sourcing-macros/tests/derive_event_tests.rs`, `event-sourcing/src/projection/engine.rs`, `event-sourcing-macros/src/projection_macro.rs`
- **Commit:** beec4dd

## Known Stubs

None. The `projection!` macro fully parses all Phase 4 DSL constructs and generates working code. The e2e test verifies the full pipeline with real `StoredEvent` values.

DSL features not implemented (deferred per CONTEXT.md):
- `|?` default operator (Phase 4 scope only covers `|`, `|+`, `|-`)
- `join` block inside object (Phase 5)
- List fields (Phase 4.1)

## Threat Surface Scan

| Flag | File | Description |
|------|------|-------------|
| mitigated: T-04.06-01 | projection_macro.rs | syn ParseBuffer is bounded by input token count; Error::new_spanned exits early on unrecognized token — no unbounded recursion |
| mitigated: T-04.06-02 | projection_tests.rs | `projection_macro_definition_matches_builder` asserts macro-generated definition `==` manually built definition |
| accepted: T-04.06-03 | projection_macro.rs | Compile errors include user's own DSL tokens; no library internals exposed |

## Commits

| Hash | Type | Description |
|------|------|-------------|
| 18ea776 | feat | implement projection! DSL macro parser and code generator |
| 190971a | test | add e2e integration test projection! -> ProjectionEngine -> typed result |
| beec4dd | style | cargo fmt + clippy fixes |

## Self-Check: PASSED

- event-sourcing-macros/src/projection_macro.rs — FOUND
- event-sourcing-macros/src/lib.rs (modified) — FOUND
- event-sourcing/src/lib.rs (modified) — FOUND
- event-sourcing/tests/projection_tests.rs — FOUND
- event-sourcing/tests/projection_e2e.rs — FOUND
- Commit 18ea776 — FOUND
- Commit 190971a — FOUND
- Commit beec4dd — FOUND
- 5 projection macro tests — PASSED
- 3 e2e tests — PASSED
- 139 total workspace tests — PASSED
- clippy --workspace --all-targets --features macros -D warnings — PASSED
