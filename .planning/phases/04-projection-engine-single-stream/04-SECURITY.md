---
phase: "04"
slug: projection-engine-single-stream
status: verified
threats_open: 0
asvs_level: 1
created: "2026-04-24"
---

# Phase 04 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| JSON deserialization | `serde_json::from_str` on user-supplied ProjectionDefinition / EventSchemaDef JSON | Untrusted schema definitions, handler specs |
| Proc macro input | Token stream passed to `#[derive(Event)]` and `projection!` macros at compile time | Developer-authored Rust source code (compile time only) |
| JSON path evaluation | `evaluate_path` traverses event payload Value at runtime | Event payload data (not user-controlled at library level) |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-04.01-01 | Tampering | EventSchemaDef JSON deserialization | mitigate | `#[serde(deny_unknown_fields)]` on FieldDef and EventSchemaDef; tested by `eventschemadef_deny_unknown_fields` | closed |
| T-04.01-02 | Information Disclosure | FieldType enum discriminants | accept | Type names are public schema — no sensitive data | closed |
| T-04.01-03 | Denial of Service | EventSchemaDef with huge fields vector | accept | Application-level concern; caller enforces payload size limits before calling serde_json::from_str | closed |
| T-04.02-01 | Tampering | ProjectionDefinition JSON deserialization | mitigate | `#[serde(deny_unknown_fields)]` on ProjectionDefinition, ScalarFieldSpec, ObjectFieldSpec, and all HandlerSpec inner structs; tested by `projection_definition_deny_unknown_fields` and `scalar_deny_unknown` | closed |
| T-04.02-02 | Tampering | HandlerSpec::Value accepts any JSON Value | accept | By design — literal values. Schema validated at registration time, not parse time | closed |
| T-04.02-03 | Denial of Service | IndexMap with unbounded field count | accept | Application-level responsibility; documented limitation | closed |
| T-04.03-01 | Tampering | Schema stored at first append | mitigate | `record_if_new` uses `HashMap::entry().or_insert_with()` — append-only, existing schemas never overwritten; tested by `record_if_new_is_append_only` | closed |
| T-04.03-02 | Elevation of Privilege | validate_schemas bypassed | accept | Opt-in by design (unopinionated library); NoOpEventSchemaStore is the default | closed |
| T-04.03-03 | Denial of Service | Unbounded schemas HashMap | accept | Bounded by number of distinct event types (compile-time constant in practice) | closed |
| T-04.04-01 | Tampering | JSON path evaluation | mitigate | `evaluate_path` returns None for absent/type-mismatched paths; `FieldNotFound` returned for required fields rather than silent wrong output; tested by path tests | closed |
| T-04.04-02 | Denial of Service | Deeply nested ProjectionDefinition or payload | accept | No recursion depth limit in Phase 4; practical depth bounded by projection schema (authored by library user) | closed |
| T-04.04-03 | Information Disclosure | ProjectionError::FieldNotFound leaks path info | accept | Internal debug info, not secrets; callers decide whether to surface errors to users | closed |
| T-04.04-04 | Tampering | Increment/Decrement on non-numeric field | mitigate | `TypeMismatch` error returned on non-numeric value; no silent corruption; tested by increment test | closed |
| T-04.05-01 | Denial of Service | proc macro infinite loop on malformed input | mitigate | `syn::parse2` returns Result — all errors surface as `syn::Error → compile_error!`, never panic or infinite loop | closed |
| T-04.05-02 | Elevation of Privilege | Generated code references event_sourcing paths | accept | Generated code only calls public API (`FieldType::*`, `FieldDef`, `EventSchemaDef`) — no capability escalation | closed |
| T-04.05-03 | Information Disclosure | Unsupported type name in compile error | accept | Intentional for debuggability; type names in compile errors are not sensitive | closed |
| T-04.06-01 | Denial of Service | projection! macro on pathological input | mitigate | ParseStream bounded by input token count; `Error::new_spanned` on unrecognized token exits early | closed |
| T-04.06-02 | Tampering | Generated ReadModel::definition() returns wrong ProjectionDefinition | mitigate | `projection_macro_definition_matches_builder` asserts macro-generated definition == manually constructed builder result; tested in projection_tests.rs | closed |
| T-04.06-03 | Information Disclosure | Compile error exposes DSL internals | accept | Errors contain user's own DSL tokens for debuggability; no library internals exposed | closed |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-04-01 | T-04.01-02 | FieldType enum discriminants are public schema — no sensitive data | phase author | 2026-04-24 |
| AR-04-02 | T-04.01-03 | Payload size enforcement is an application-level responsibility; library imposes no limit | phase author | 2026-04-24 |
| AR-04-03 | T-04.02-02 | HandlerSpec::Value accepts any JSON Value by design; schema validated at registration | phase author | 2026-04-24 |
| AR-04-04 | T-04.02-03 | IndexMap field count unbounded; caller responsibility | phase author | 2026-04-24 |
| AR-04-05 | T-04.03-02 | validate_schemas is opt-in; unopinionated library design | phase author | 2026-04-24 |
| AR-04-06 | T-04.03-03 | schemas HashMap bounded by distinct event types (compile-time constant) | phase author | 2026-04-24 |
| AR-04-07 | T-04.04-02 | Recursion depth bounded by user-authored projection schema in practice | phase author | 2026-04-24 |
| AR-04-08 | T-04.04-03 | FieldNotFound paths are internal debug info; callers control error surfaces | phase author | 2026-04-24 |
| AR-04-09 | T-04.05-02 | Generated code calls only public event_sourcing API | phase author | 2026-04-24 |
| AR-04-10 | T-04.05-03 | Type names in compile errors are intentional for debuggability | phase author | 2026-04-24 |
| AR-04-11 | T-04.06-03 | Compile errors contain user's own DSL tokens; no library internals | phase author | 2026-04-24 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-04-24 | 19 | 19 | 0 | gsd-secure-phase (all tests passing, dispositions from plan threat registers) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter
