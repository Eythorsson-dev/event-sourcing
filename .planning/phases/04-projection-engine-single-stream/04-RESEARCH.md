# Phase 4: Projection Engine — Single-Stream — Research

**Phase:** 4 — Projection Engine, Single-Stream (Scalars and Nested Objects)
**Confidence:** HIGH
**Date:** 2026-04-12

---

## RESEARCH COMPLETE

---

## Key Findings

1. **All API contracts are fully locked in CONTEXT.md.** The planner has unusually complete input: every type signature, every JSON schema key, the DSL syntax, the fold algorithm semantics (D-17 through D-22), and the proc-macro crate setup (D-14). The only discretion areas are internal module layout, whether `ProjectionEngine` is a struct vs free functions, JSON path library choice, and error variant design.

2. **The largest implementation risk is the `projection!` proc-macro.** It parses a custom grammar (not Rust syntax), must emit three outputs simultaneously (struct definition, `#[derive(Deserialize)]`, `impl ReadModel`), and must produce span-pointing compile errors (PROJ-08). Use `syn::parse::Parse` with `syn::Error::new_spanned` — not `panic!`. The builder API must be implemented first because the macro generates builder calls, not raw struct literals.

3. **`EventLog<S>` must become `EventLog<S, E: EventSchemaStore>`.** Adding the second type parameter breaks all existing call sites. The fix is to add a default: `EventLog<S, E = NoOpEventSchemaStore>`. Phase 4 must include an `InMemoryEventSchemaStore` (in `event-sourcing-logstore-inmemory`) and a `NoOpEventSchemaStore` (for tests that don't need schema tracking).

4. **`IndexMap` (not `HashMap`) is required for `ProjectionDefinition.fields`.** `HashMap` has non-deterministic iteration order — JSON round-trip tests will be flaky. Add `indexmap = "2"` to workspace dependencies.

5. **`#[serde(deny_unknown_fields)]` satisfies success criterion 5** (clear error on unknown JSON fields), but it conflicts with `#[serde(flatten)]` and has a subtle interaction with `#[serde(tag = "type")]` internally-tagged enums. Place `deny_unknown_fields` on variant structs, not on the enum itself.

6. **JSON path traversal uses `serde_json_path` (RFC 9535).** Initially assessed as hand-rollable for Phase 4's simple `$.a`/`$.a.b` paths, but `serde_json_path` was adopted instead: (a) Phase 4.1 list fields will need array traversal — adopting it now avoids replacing the evaluator mid-stream; (b) path strings in `ProjectionDefinition` are validated RFC 9535 expressions, not a bespoke subset; (c) the builder validates paths at definition-construction time using `JsonPath::parse()`, so the engine can evaluate without error-handling overhead. Add `serde_json_path = "0.7"` (confirm latest 0.x version) to workspace dependencies.

---

## Recommended Wave Order

1. **Wave 1 — Data types + serde:** `FieldType`, `FieldDef`, `EventSchemaDef`, `Event` trait, `ProjectionDefinition` + builder API, all serde round-trip tests, `deny_unknown_fields` coverage.
2. **Wave 2 — EventSchemaStore + EventLog integration:** `EventSchemaStore` trait, `InMemoryEventSchemaStore`, `NoOpEventSchemaStore`, `EventLog<S, E>` with default param, `validate_schemas()`, `AppendError::SchemaConflict` variant.
3. **Wave 3 — ProjectionEngine:** `apply_raw`, `project<M>`, `project_from`, `ProjectionCheckpoint`, `ProjectionError` variants, full fold tests with scalar + nested object + `cleared_by` scenarios.
4. **Wave 4 — `#[derive(Event)]` proc-macro:** New `event-sourcing-macros` crate, derive macro for `Event` trait, Rust type → `FieldType` mapping, compile-error tests.
5. **Wave 5 — `projection!` DSL macro:** Custom grammar parser, code generation with `quote!`, re-export behind `macros` feature flag, end-to-end integration test (macro → fold → typed result).

---

## Architecture Decisions for Planner

**Internal module layout (Claude's discretion — recommendation):**
```
event-sourcing/src/
├── schema.rs               # FieldType, FieldDef, EventSchemaDef, Event trait, EventSchemaStore trait
├── projection/
│   ├── mod.rs
│   ├── definition.rs       # ProjectionDefinition, FieldSpec, HandlerSpec, ObjectFieldSpec, builder
│   ├── engine.rs           # ProjectionEngine (unit struct), apply_raw, project, project_from
│   └── error.rs            # ProjectionError, EventSchemaError, SchemaConflictError
└── (all existing files unchanged)

event-sourcing-macros/src/
├── lib.rs                  # proc-macro = true
├── derive_event.rs         # #[derive(Event)]
└── projection_macro.rs     # projection! { }
```

**`ProjectionEngine` as unit struct** (not free functions): enables future state addition without breaking callers.

**`EventSchemaStore` uses native `async fn in trait`** (static dispatch, same pattern as `LogStore`) — no `async-trait` needed for Phase 4.

---

## Standard Stack (Phase 4 additions)

| Library | Version | Purpose |
|---------|---------|---------|
| syn | 2.x | Parse `projection!` custom grammar (per CLAUDE.md stack) |
| quote | 1.0 | Generate Rust struct + impl from parsed AST (per CLAUDE.md stack) |
| proc-macro2 | 1.0 | Token stream abstraction (per CLAUDE.md stack) |
| indexmap | 2.x | Insertion-order map for deterministic JSON serialization |

All other libraries (serde 1.0.220, serde_json 1.0.149, thiserror 2.0) already in workspace.

---

## Open Questions (RESOLVED)

1. **`HandlerSpec::Value { value: null }` vs absent key in serde:** `{ "value": null }` should deserialize as `Value::Null`. Confirm by test — the enum variant approach handles this correctly.
2. **`NoOpEventSchemaStore` home:** Recommend defining it in the core crate behind `#[cfg(test)]` for unit tests; `InMemoryEventSchemaStore` lives in `event-sourcing-logstore-inmemory`.
3. **`indexmap` workspace dep:** Should be added to `[workspace.dependencies]` since later phases may use it too.

---

## Assumptions Log

| # | Claim | Risk |
|---|-------|------|
| A1 | `async-trait` not needed for `EventSchemaStore` (static dispatch only) | Low — if `dyn EventSchemaStore` needed, add `async-trait`; not blocking Phase 4 |
| A2 | Default type parameter `EventLog<S, E = NoOpEventSchemaStore>` for backward compat | Confirm strategy; alternative is explicit update of all call sites |
| A3 | `IndexMap` required for deterministic JSON | Confirm if any test compares JSON strings directly |
| A4 | `#[derive(Event)]` maps Rust types via path-string matching (no type alias resolution) | Acceptable for v1; document limitation |
| A5 | Hand-rolled JSON path sufficient for `$.a.b.c` (no arrays) | Confirmed in CONTEXT.md as known limitation for Phase 4 |