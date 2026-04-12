# Phase 4: Projection Engine — Single-Stream - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-06
**Phase:** 04-projection-engine-single-stream
**Areas discussed:** Proc-macro scope, ProjectionDefinition operation vocabulary, Fold/project API, Projection trait design, Schema structure, Joins, Observer wiring, Library vs application DB

---

## Proc-Macro Scope (v1 or deferred?)

| Option | Description | Selected |
|--------|-------------|----------|
| Defer macro to v2 | Builder API only in v1; macro later | |
| Include macro in Phase 4 | PROJ-07 + PROJ-08 in scope | ✓ |

**User's choice:** DSL macro is an essential feature; implement it in Phase 4.
**Notes:** STATE.md had an outdated note saying "proc macro deferred to v2" — superseded by ROADMAP.md which lists PROJ-07, PROJ-08 for Phase 4.

---

## Projection vs ReadModel Trait Naming

| Option | Description | Selected |
|--------|-------------|----------|
| `Projection` trait | Mathematical term for the fold operation | |
| `ReadModel` trait | Domain term for the output type | ✓ |

**User's choice:** `ReadModel` — `ProjectionDefinition` already owns the "projection" term.

---

## Output Associated Type

| Option | Description | Selected |
|--------|-------------|----------|
| `type Output: DeserializeOwned` on trait | Explicit associated type | |
| No associated type — `Output = Self` | Inferred, `trait ReadModel: DeserializeOwned` | ✓ |

**User's choice:** No associated type. Output is always `Self`, so no explicit declaration needed.

---

## Fold / Project API Name

| Option | Description | Selected |
|--------|-------------|----------|
| `fold` | Internal mathematical term | |
| `project` | Domain term — projects events into a read model | ✓ |

**User's choice:** `project`. `fold` is the implementation detail; callers think in "projecting."

---

## Events Parameter Scope

| Option | Description | Selected |
|--------|-------------|----------|
| All events in the log | Engine fetches everything | |
| Specific stream events passed by caller | Caller reads stream, passes iterator | ✓ |

**User's choice:** Caller passes events for a specific stream. Engine stays pure and testable.
**Notes:** Returning a list of read models is the observer's job (fan-out over all stream IDs), not `project`'s.

---

## ProjectionDefinition Schema Structure

### `on` vs `events` key name

| Option | Selected |
|--------|----------|
| `on` (original) | |
| `value` (user suggestion — conflicts with literal-value key) | |
| `events` (event type names as keys of an object) | ✓ |

**Notes:** User's initial suggestion of `value` conflicted with the static literal operation. `events` as an object with event names as keys was settled on — more concise, no redundant `"event": "..."` wrapper.

### `root_stream` vs `from`

| Option | Selected |
|--------|----------|
| `root_stream` | |
| `from` (aligns with SQL FROM clause) | ✓ |

### `events` as array vs object

| Option | Selected |
|--------|----------|
| Array of `{ event, from/value/... }` objects | |
| Object with event name as key | ✓ |

**Notes:** Object approach is more concise and consistent. JSON object key order not guaranteed, but field-update order within a single event type is intentionally order-independent.

### Operation vocabulary

Settled: `from` (JSON Path), `value` (literal), `increment` (N), `decrement` (N), `increment_by` (path), `decrement_by` (path).
Computations (arbitrary functions) deferred to a new phase — user has specific ideas.

### Required fields and defaults

| Scenario | Behavior |
|----------|----------|
| `required: true`, no default | Field is `T` in Rust struct; deserialization fails until event sets it |
| `required: true`, with `default` | Field starts with default value; overwritten by events |
| No `required` (default) | Field is `Option<T>` in Rust struct |

### List keys

Settled: `keys` is an object where field names map to their JSON Path. Supports composite keys (multiple entries). Engine uses these paths for both upsert key resolution and remove.

---

## Joins Design (Phase 5 scope — design locked, not implemented)

### `joins` structure

| Option | Selected |
|--------|----------|
| Array with `name` field inside | |
| Object with alias as key | ✓ |

### Field join reference

| Option | Selected |
|--------|----------|
| `"join": "alias"` string (single join shorthand) | |
| `"join": { "alias": { "EventName": {...} } }` object | ✓ |

**Notes:** Object approach handles multi-stream fields cleanly (a field updated by both root events and join stream events). `events` key always means root stream; `join` key always means joined streams.

### `from_array` (events containing list payloads)

**Decision:** Out of v1. Known limitation — document in README. Design direction noted for future.

---

## Observer Wiring

### ProjectionObserver design

| Option | Selected |
|--------|----------|
| Typed to specific `ReadModel` at compile time | |
| Data-driven: reads definitions from library DB, not typed | ✓ |

**Notes:** Observer loads all registered `ProjectionDefinition`s from the DB and materializes state for each using `serde_json::Value`. Typed `ReadModel` trait is used for registration and deserialization, not for the observer internals.

### Observer invocation modes

**Decision:** Two registration methods on `EventLog` — `register_inline` (synchronous, blocks append) and `register_eventual` (background). Single `Observer` trait shared by both.

---

## Library DB vs Application DB

**Decision:** Clear separation — library owns `events`, `projection_definitions`, `projections` (checkpoints + raw state), `observers`. Application owns read model storage (could be Redis, graph DB, relational DB). For v1, raw `serde_json::Value` temporarily lives in library DB. A dedicated phase will be added after Phase 7 to design this properly.

---

## New Phases Identified During Discussion

- **Temporal joins** — point-in-time join semantics (after Phase 5)
- **Transformation functions** — user-defined named functions in projection fold (after Phase 5, user has specific ideas)
- **Read model persistence / library DB schema** — after Phase 7 (Observer Infrastructure)

## Deferred Ideas

- `from_array` / events with array payloads — v2 or future phase
- `inventory` crate for compile-time ReadModel registration — Phase 7 implementation detail

---

## Schema Conflict Definition — Revisited 2026-04-12

| Option | Description | Selected |
|--------|-------------|----------|
| Permissive (original D-07) | Adding fields OK; only remove/retype = conflict | |
| Strict equality | Any diff (including additions) = conflict; evolution is an explicit future feature | ✓ |

**User's choice:** Keep v1 simple — no schema drift allowed, not even additive. Non-breaking evolution and schema migration deferred to a dedicated todo (`non-breaking-schema-evolution.md`).
**Rationale:** Strict equality is unambiguous and forces deliberate schema changes rather than accidental drift. Evolution semantics (aliases, additive fields, versioned migrations) deserve their own design pass, not a v1 compromise.
**Impact:** D-07 rewritten. New todo created. D-08 (DB+code union) unchanged — still needed for bootstrap and deleted-type survival.
