# Phase 4: Projection Engine — Single-Stream - Context

**Gathered:** 2026-04-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Build the `ProjectionDefinition` type (with builder API and JSON serialization), the `ProjectionEngine` that folds events into read model state, the `ReadModel` trait, and the `projection!` DSL macro with its `event-sourcing-macros` proc-macro crate.

Phase 4 scope: **single-stream projections only**. No joins — those are Phase 5. The JSON schema is designed with joins in mind (Phase 5 extends it) but Phase 4 implements and tests only the `from` + `fields` subset.

</domain>

<decisions>
## Implementation Decisions

### ReadModel Trait

- **D-01:** The trait is named `ReadModel`, not `Projection`. `ProjectionDefinition` already owns the "projection" term. The trait is implemented by user-defined structs that _are_ read models.
- **D-02:** No associated `Output` type. The output is always `Self`. The trait bound is:
  ```rust
  trait ReadModel: serde::de::DeserializeOwned {
      fn definition() -> ProjectionDefinition;
  }
  ```
- **D-03:** The `projection!` macro generates a `#[derive(Deserialize)]` struct and `impl ReadModel for StructName`. This is the primary user-facing interface (PROJ-07). The builder API (PROJ-06) is the underlying mechanism the macro calls, also usable directly by advanced users.
- **D-04:** The proc-macro crate is `event-sourcing-macros` with `proc-macro = true` in its `Cargo.toml`. Re-export the macro from `event-sourcing` core behind a `macros` feature flag (PROJ-08). Compile errors from the macro must be human-readable.

### ProjectionEngine API

- **D-05:** Method named `project`, not `fold`. `fold` is the internal mechanism; `project` is the domain operation.
- **D-06:** Two-layer API:
  ```rust
  // Base: pure function, testable, no trait required
  fn apply_raw(def: &ProjectionDefinition, events: impl Iterator<Item=StoredEvent>)
      -> Result<serde_json::Value, ProjectionError>

  // Typed convenience: calls apply_raw then deserializes
  fn project<M: ReadModel>(events: impl Iterator<Item=StoredEvent>)
      -> Result<M, ProjectionError>
  ```
- **D-07:** `project` operates on events for a **specific stream instance** passed by the caller. The engine does not fetch events itself — callers read the stream from `EventLog` and pass the iterator. This keeps the engine pure and testable.
- **D-08:** Incremental updates via `ProjectionCheckpoint`:
  ```rust
  struct ProjectionCheckpoint {
      raw_state: serde_json::Value,
      last_sequence: GlobalSequenceId,
  }

  fn project_from<M: ReadModel>(
      checkpoint: Option<ProjectionCheckpoint>,
      new_events: impl Iterator<Item=StoredEvent>,
  ) -> Result<(M, ProjectionCheckpoint), ProjectionError>
  ```
  `checkpoint: None` is a full replay from scratch.

### ProjectionDefinition JSON Schema

The authoritative schema. Phase 4 implements the `from` + `fields` subset. Phase 5 adds `joins` at root and list levels, and `join` on fields.

**Phase 4 schema (implementable now):**
```json
{
  "name": "ProjectionName",
  "from": "stream-type",
  "fields": {
    "scalar_field": {
      "type": "String",
      "required": true,
      "default": "draft",
      "events": {
        "EventType":  { "from": "$.json_path" },
        "OtherEvent": { "value": "literal_string" }
      }
    },
    "counter_field": {
      "type": "i32",
      "events": {
        "ItemAdded":   { "increment": 1 },
        "ItemRemoved": { "decrement": 1 },
        "ItemUpdated": { "increment_by": "$.delta" }
      }
    },
    "list_field": {
      "type": "list",
      "keys": { "key_field_name": "$.json_path_to_key" },
      "remove_on": ["EventThatRemovesItem"],
      "fields": {
        "key_field_name": {
          "type": "String",
          "required": true,
          "events": { "ItemAdded": { "from": "$.key_field" } }
        },
        "other_item_field": {
          "type": "i32",
          "events": {
            "ItemAdded":   { "from": "$.quantity" },
            "ItemUpdated": { "from": "$.quantity" }
          }
        }
      }
    }
  }
}
```

**Phase 5 extension (design locked, not implemented in Phase 4):**
```json
{
  "joins": {
    "alias": { "stream": "stream-type", "on": "$.state_field_path" }
  },
  "fields": {
    "join_only_field": {
      "type": "String",
      "join": {
        "alias": {
          "JoinedEvent": { "from": "$.name" }
        }
      }
    },
    "multi_stream_field": {
      "type": "String",
      "events": { "RootEvent": { "value": "from_root" } },
      "join": {
        "alias": { "JoinedEvent": { "value": "from_join" } }
      }
    }
  }
}
```

### Schema Design Rules

- **D-09:** `from` — root stream type name. All events without a `join` reference come from this stream.
- **D-10:** `events` is always an **object** with event type names as keys (not an array). Values are handler objects.
- **D-11:** Handler operation vocabulary (exhaustive for Phase 4):
  - `{ "from": "$.path" }` — copy field from event payload via JSON Path
  - `{ "value": "literal" }` — set to a static literal
  - `{ "increment": N }` — add N to a numeric field
  - `{ "decrement": N }` — subtract N from a numeric field
  - `{ "increment_by": "$.path" }` — add the value at path to a numeric field
  - `{ "decrement_by": "$.path" }` — subtract the value at path from a numeric field
- **D-12:** `required: true` → the Rust struct field is `T` (not `Option<T>`); deserialization fails if null. `required: false` (default) → `Option<T>`. `default` sets the initial state value before any events — a `required` field with a `default` starts populated.
- **D-13:** List `keys` is an **object** where field names map to their JSON Path in the event payload. The engine uses these paths for ALL list operations (upsert key resolution and remove). Supports composite keys (multiple entries in the object).
- **D-14:** List upsert is implicit — any event that appears in any list item field's `events` triggers an upsert for that item. `remove_on` is an explicit array of event type names. Both use the `keys` paths for key extraction.
- **D-15:** JSON Path uses `$.` prefix for event payload references. Supports nested paths (`$.address.city`).

### ProjectionObserver (Phase 7 Design Note)

- **D-16:** `ProjectionObserver` is **not** typed to a specific `ReadModel`. It is a data-driven infrastructure component that reads `ProjectionDefinition`s from the library database and materializes state for all registered projections.
- **D-17:** On startup, users register their `ReadModel` types (via `library.register::<OrderSummary>()` or the `inventory` crate pattern). The library syncs their `ProjectionDefinition` JSON to the `projection_definitions` table. If the definition hash changes, the projection is flagged for replay.
- **D-18:** The library database owns: `events`, `projection_definitions`, `projections` (checkpoint + raw `serde_json::Value` state), `observers`. The application database owns the actual read model storage (could be Redis, PostgreSQL, graph DB, or anything). For v1, raw state is temporarily materialized in the library DB as `serde_json::Value`.
- **D-19:** Two observer invocation modes on `EventLog`: `register_inline(observer)` (synchronous, blocks `append`) and `register_eventual(observer)` (background, eventual consistency). The `Observer` trait is the same for both — the difference is registration.

### Claude's Discretion

- Internal module layout within `event-sourcing` core for `ProjectionDefinition`, `ProjectionEngine`, and `ReadModel`
- Internal representation of `ProjectionDefinition` in Rust (struct vs enum hierarchy)
- Whether `ProjectionEngine` is a struct or a module of free functions
- JSON Path evaluation library choice (or hand-rolled for simple `$.field` paths)
- Error variants in `ProjectionError`
- Test structure and coverage scope

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Prior Phase Contracts
- `.planning/phases/01-workspace-setup-and-core-types/01-CONTEXT.md` — Core types: `StoredEvent`, `StreamId`, sequence ID types, `serde_json::Value` payload (D-07), `event_type: String` on every event (D-08). These are the inputs to the projection engine.
- `.planning/phases/03-event-log-and-optimistic-concurrency/03-CONTEXT.md` — `EventLog<S>` API: callers read streams via `EventLog::read_stream`, results passed to `ProjectionEngine::project`.

### Requirements
- `.planning/REQUIREMENTS.md` — PROJ-01, PROJ-03, PROJ-04, PROJ-06, PROJ-07, PROJ-08 are the requirements for this phase.
- `.planning/PROJECT.md` — Core value: "The projection engine is the heart." Constraints: Rust library, type safety, unopinionated.

### Technology Stack
- `CLAUDE.md` — Proc-macro crate pattern: separate crate with `proc-macro = true`, re-exported from core behind feature flag. Stack versions for `serde`, `serde_json`, `syn 2.x`, `quote`, `proc-macro2`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `event-sourcing/src/event.rs` — `StoredEvent` is the input type to the projection engine. Fields: `global_sequence`, `stream_id`, `stream_sequence`, `event_type: String`, `payload: serde_json::Value`, `timestamp`.
- `event-sourcing/src/event_log.rs` — `EventLog::read_stream` is how callers get event iterators to pass to `project`. The engine is a consumer of this API.
- `event-sourcing/src/lib.rs` — Public re-export pattern to follow. Add `ReadModel`, `ProjectionDefinition`, `ProjectionEngine` here.

### Established Patterns
- Static dispatch throughout: `S: LogStore` generics, native `async fn in trait`. `ProjectionEngine` should follow the same pattern — no `dyn`, generic where needed.
- `thiserror` for all error types: `ProjectionError` should use `#[derive(thiserror::Error)]`.
- No re-exports from core except at `lib.rs`.

### Integration Points
- Phase 5 extends `ProjectionEngine` with multi-stream join support and `project_from` for catch-up reads.
- Phase 6 (Constraint Validation) uses `ProjectionEngine` to validate invariants before appends.
- Phase 7 (Observer Infrastructure) introduces `ProjectionObserver` which calls `project_from` with a stored checkpoint.

</code_context>

<specifics>
## Specific Ideas

- The `projection!` macro is the primary user-facing interface. The builder API is the internal mechanism that the macro generates calls to — advanced users can also call it directly.
- `ProjectionCheckpoint` is opaque to callers — they store and restore it, but do not interpret its internals.
- A `required: true` field with a `default` value starts populated in initial state. A `required: true` field without a `default` is null in raw state until an event sets it — deserialization of `M` will fail until then.
- JSON Path `$.` convention aligns with standard JSON Path. For Phase 4, simple paths (`$.field`, `$.nested.field`) are sufficient. Arrays in payloads (e.g., `$.items[0].id`) are a known limitation — see deferred ideas.

</specifics>

<deferred>
## Deferred Ideas

### Joins (Phase 5)
Full join design is locked in this context (see D-16 extension schema) but not implemented. Phase 5 adds `joins` object at root and list levels, and `join` as an object on fields (alias → events map). The `on` field resolves against the containing projection state to find the joined stream ID.

### Temporal Joins (new phase after Phase 5)
Point-in-time join semantics: limit a joined stream's events to those with `GlobalSequenceId ≤` the sequence of the triggering root stream event. Use case: "product name at the time the item was added, not today's name." Design note: requires the engine to pass the root event's sequence as a read boundary when subscribing to the joined stream.

### Transformation Functions (new phase after Phase 5)
User-defined named functions registered at startup and called during the projection fold. Enables computed fields (`"transform": "calculate_tax"`) that can't be expressed in the JSON schema. User has specific ideas for this feature. Add as a dedicated phase.

### Read Model Persistence / Library DB Schema (new phase after Phase 7)
Covers: `ProjectionStore` trait, library DB table schema (`projection_definitions`, `projections`, `observers`), application DB separation, how applications read materialized `serde_json::Value` state and convert to typed structs. The `ProjectionObserver` temporarily stores raw state in the library DB for v1 — this phase designs the proper separation.

### Events Containing Array Payloads (`from_array`)
Known limitation: projections process one event at a time. An event whose payload contains a list of items (e.g., `BulkOrderCreated` with `$.items: [...]`) cannot fan out to multiple list upserts via the current schema. Workaround: emit one event per item. Design direction noted (`from_array` / `source_array` at the list level with per-element scoped `$` path). README should document this limitation explicitly.

### Multi-Key List Items
`keys` already supports composite keys (multiple entries in the object). No additional design needed — the schema handles it.

</deferred>

---

*Phase: 04-projection-engine-single-stream*
*Context gathered: 2026-04-06*
