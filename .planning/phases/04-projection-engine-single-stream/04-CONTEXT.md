# Phase 4: Projection Engine — Single-Stream - Context

**Gathered:** 2026-04-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Build the `EventSchemaDef` type and `Event` trait (with derive macro), the `EventSchemaStore` trait, the `ProjectionDefinition` type (with builder API and JSON serialization), the `ProjectionEngine` that folds events into read model state, the `ReadModel` trait, and the `projection!` DSL macro with its `event-sourcing-macros` proc-macro crate.

Phase 4 scope: **single-stream projections only**. No joins — those are Phase 5. The JSON schema is designed with joins in mind (Phase 5 extends it) but Phase 4 implements and tests only the `from` + `fields` subset.

**Depends on:** Phase 03.1 (DCB model revision) — the correct `StoredEvent` structure and sequence ID model must be settled before the projection engine is built on top of it.

</domain>

<decisions>
## Implementation Decisions

### FieldType

- **D-01:** `FieldType` is defined in the core crate. It is a **macro-generated enum** (not a trait) covering all primitives the projection engine understands. Starting set: `String`, `Integer`, `Decimal`, `Date`, `DateTime`. `List` is a structural qualifier, not a scalar type.
- **D-02:** The macro-generated enum approach is chosen because: (a) the `projection!` macro must emit concrete Rust field types at compile time, which requires exhaustive matching; (b) enum serializes cleanly to JSON; (c) the library can always be refactored to a trait later if extensibility is needed. Custom domain types are an application concern — they map to/from primitives in the application layer.

### Event Trait and EventSchema

- **D-03:** Events must implement the `Event` trait:
  ```rust
  trait Event {
      fn event_type() -> &'static str;
      fn schema() -> EventSchemaDef;
  }
  ```
  The derive macro `#[derive(Event)]` generates this impl from the struct's field names and Rust types, mapping them to `FieldType` variants. This is the bridge between compiled Rust types and the persisted `EventSchemaDef`.

- **D-04:** `EventSchemaDef` is the serializable schema record for a single event type:
  ```rust
  struct EventSchemaDef {
      event_type: String,
      fields: Vec<FieldDef>,
  }
  struct FieldDef {
      name: String,
      field_type: FieldType,
  }
  ```

- **D-05:** EventSchema is recorded **at first append** of a given event type. The `EventLog` calls `EventSchemaStore::record_if_new` during append orchestration. If a schema for that `event_type` already exists in the store, the record is skipped (append-only — existing schemas are never overwritten).

- **D-06:** **Schema immutability**: once a schema is persisted, the event struct in code must not diverge from it. The library enforces this at two layers:
  1. **Startup (explicit):** `event_log.validate_schemas().await -> Result<(), SchemaConflictError>` — compares all `#[derive(Event)]` schemas in compiled code against every persisted `EventSchemaDef`. Returns a typed error; the application decides whether to panic or refuse to start.
  2. **Append (implicit safety net):** If a schema for the event type already exists in the store and it differs from `event.schema()`, `EventLog::append` returns `Err(AppendError::SchemaConflict { event_type, expected, actual })`.

- **D-07:** **Conflict definition (simple semantic):** A conflict is triggered when a field is removed or its `FieldType` changes. Adding new fields is allowed — old events simply won't have those fields in their JSON payload (they produce `null` for projection paths that reference them). Renaming a field is always a conflict (appears as remove + add). Field reordering is not a conflict.

- **D-08:** **Validator uses both DB and code schemas (union):** On startup and at ProjectionDefinition registration, the validator resolves schemas from the union of (a) persisted `EventSchemaDef`s from the store and (b) `Event::schema()` from all registered compiled types. This solves the bootstrap problem: in a new environment with no events yet, schemas come entirely from code. Over time, the DB becomes the complete historical record. If a Rust event type is later deleted, the persisted schema remains usable by the projection engine for historical events.

### EventSchemaStore Trait

- **D-09:** `EventSchemaStore` is a **separate trait from `LogStore`**. `EventLog` holds both:
  ```rust
  struct EventLog<S: LogStore, E: EventSchemaStore> { ... }
  ```
  `EventLog` is the orchestrator — it knows both stores but neither store knows about the other.

- **D-10:** Minimum `EventSchemaStore` surface:
  ```rust
  #[async_trait]
  trait EventSchemaStore {
      async fn record_if_new(&self, schema: &EventSchemaDef) -> Result<(), EventSchemaError>;
      async fn fetch_all(&self) -> Result<Vec<EventSchemaDef>, EventSchemaError>;
      async fn fetch_one(&self, event_type: &str) -> Result<Option<EventSchemaDef>, EventSchemaError>;
  }
  ```
  The table is **append-only** — no update or delete methods are exposed.

### ReadModel Trait

- **D-11:** The trait is named `ReadModel`, not `Projection`. `ProjectionDefinition` already owns the "projection" term. The trait is implemented by user-defined structs that _are_ read models.
- **D-12:** No associated `Output` type. The output is always `Self`. The trait bound is:
  ```rust
  trait ReadModel: serde::de::DeserializeOwned {
      fn definition() -> ProjectionDefinition;
  }
  ```
- **D-13:** The `projection!` macro generates a `#[derive(Deserialize)]` struct and `impl ReadModel for StructName`. This is the primary user-facing interface (PROJ-07). The builder API (PROJ-06) is the underlying mechanism the macro calls, also usable directly by advanced users.
- **D-14:** The proc-macro crate is `event-sourcing-macros` with `proc-macro = true` in its `Cargo.toml`. Re-export the macro from `event-sourcing` core behind a `macros` feature flag (PROJ-08). Compile errors from the macro must be human-readable.

### ProjectionDefinition Validation

- **D-15:** A `ProjectionDefinition` is **always validated before being persisted** to the database. Validation checks:
  1. Every event type referenced in the projection's `events` block exists in schemas (DB union code).
  2. Every event type referenced in `join` blocks exists in schemas (DB union code). *(Phase 5 — noted here for completeness.)*
  3. Every mapped field (`$.amount`) exists in that event type's `EventSchemaDef`.
  4. All event field mappings for a single projection output field resolve to the same `FieldType`.
- **D-16:** `ProjectionDefinition` does **not** specify `FieldType` on its fields. Field types are owned by `EventSchemaDef` and resolved by the validator at registration time. The projection only declares what fields it maps from events — not what type those fields are.

### ProjectionEngine API

- **D-17:** Method named `project`, not `fold`. `fold` is the internal mechanism; `project` is the domain operation.
- **D-18:** Two-layer API:
  ```rust
  // Base: pure function, testable, no trait required
  fn apply_raw(def: &ProjectionDefinition, events: impl Iterator<Item=StoredEvent>)
      -> Result<serde_json::Value, ProjectionError>

  // Typed convenience: calls apply_raw then deserializes
  fn project<M: ReadModel>(events: impl Iterator<Item=StoredEvent>)
      -> Result<M, ProjectionError>
  ```
- **D-19:** `project` operates on events for a **specific stream instance** passed by the caller. The engine does not fetch events itself — callers read the stream from `EventLog` and pass the iterator. This keeps the engine pure and testable.
- **D-20:** Incremental updates via `ProjectionCheckpoint`:
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
- **D-21:** The `ProjectionEngine` is **data-driven from persisted `EventSchemaDef`s**, not from compiled Rust types. It does not require the event's Rust struct to exist in code — only the `ProjectionDefinition` (what to do) and the `EventSchemaDef` (what fields exist and their types). This means historical events remain processable after an event type is deleted from the codebase.
- **D-22:** The engine does not silently drop events whose type does not appear in the projection's `events` block — that is intentional filtering by the `ProjectionDefinition`, not a missing-type error. The engine only errors if it encounters an event type that has no entry in any known `EventSchemaDef` (neither DB nor code).

### ProjectionDefinition JSON Schema

The authoritative schema. Phase 4 implements the `query` + `fields` subset. Phase 5 adds `joins` at root and list levels, and `join` on fields.

**Phase 4 schema (implementable now):**
```json
{
  "name": "ProjectionName",
  "query": { "StartsWith": "order:" },
  "fields": {
    "scalar_field": {
      "required": true,
      "default": "draft",
      "events": {
        "EventType":  { "from": "$.json_path" },
        "OtherEvent": { "value": "literal_string" }
      }
    },
    "counter_field": {
      "events": {
        "ItemAdded":   { "increment": 1 },
        "ItemRemoved": { "decrement": 1 },
        "ItemUpdated": { "increment_by": "$.delta" }
      }
    },
    "list_field": {
      "type": "list",
      "key": "$tags.item",
      "remove_on": ["EventThatRemovesItem"],
      "fields": {
        "some_item_field": {
          "required": true,
          "events": { "ItemAdded": { "from": "$.field" } }
        },
        "other_item_field": {
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

Note: `type` is **not** specified on scalar fields in `ProjectionDefinition` — the validator resolves it from the referenced `EventSchemaDef` (D-16).

List `key` can also use a payload field path as a fallback for non-DCB events: `"key": { "field": "$.item_id" }`.

**Phase 5 extension (design locked, not implemented in Phase 4):**
```json
{
  "query": { "StartsWith": "order:" },
  "joins": {
    "p": { "query": { "StartsWith": "product:" } }
  },
  "fields": {
    "list_field": {
      "type": "list",
      "key": "$tags.item",
      "joins": {
        "p": { "query": { "StartsWith": "product:" } }
      },
      "fields": {
        "name": {
          "events": { "ItemAdded": { "from": "$.name" } },
          "join": { "p": { "ProductUpdated": { "from": "$.name" } } }
        }
      }
    }
  }
}
```

### Schema Design Rules

- **D-23:** `query` — replaces `from`. A `TagFilter` (same type used in `AppendCondition.query`) describing which events this projection operates on. For single-stream projections: `{ "StartsWith": "order:" }`. The engine does not fetch events itself — the caller uses this field to construct their `LogStore::query` call, adding instance-specific tags at runtime.
- **D-24:** `events` is always an **object** with event type names as keys (not an array). Values are handler objects.
- **D-25:** Handler operation vocabulary (exhaustive for Phase 4):
  - `{ "from": "$.path" }` — copy field from event payload via JSON Path
  - `{ "value": "literal" }` — set to a static literal
  - `{ "increment": N }` — add N to a numeric field
  - `{ "decrement": N }` — subtract N from a numeric field
  - `{ "increment_by": "$.path" }` — add the value at path to a numeric field
  - `{ "decrement_by": "$.path" }` — subtract the value at path from a numeric field
- **D-26:** `required: true` → the Rust struct field is `T` (not `Option<T>`); deserialization fails if null. `required: false` (default) → `Option<T>`. `default` sets the initial state value before any events — a `required` field with a `default` starts populated.
- **D-27:** List `key` declares how the engine identifies items for upsert and removal. Two forms:
  - Tag-based (preferred in DCB model): `"key": "$tags.item"` — key is the value of the `item:` prefix in the event's tags. Composite: `"key": ["$tags.order", "$tags.item"]`.
  - Field-based (fallback for non-DCB events): `"key": { "field": "$.item_id" }` — key extracted from event payload path. Composite: `"key": { "fields": ["$.order_id", "$.item_id"] }`.
- **D-28:** List upsert is implicit — any event appearing in any list item field's `events` triggers an upsert. `remove_on` is an explicit array of event type names. The engine uses the `key` declaration for both upsert identity and removal matching.
- **D-29:** JSON Path uses `$.` prefix for event payload references. Supports nested paths (`$.address.city`).

### $tags on Read Model Objects

- **D-34:** Every projection root object and every list item automatically gets a `$tags` map populated from the tags of events that affected it. This is implicit — the user does not need to declare a `$tags` field.
- **D-35:** `$tags` stores the **current** value per tag prefix group, not a historical union. When a new event arrives carrying tag `product:p2` for an item that previously had `product:p1`, the `$tags.product` entry is updated to `"p2"`. The `$tags` map always reflects the latest identity of the object.
- **D-36:** `$tags` is available to the projection engine for key resolution and join resolution (Phase 5). It is also available in the serialized `serde_json::Value` state for inspection. The field name `$tags` is reserved and cannot be used as a user-declared field name.

### ProjectionObserver (Phase 7 Design Note)

- **D-30:** `ProjectionObserver` is **not** typed to a specific `ReadModel`. It is a data-driven infrastructure component that reads `ProjectionDefinition`s from the library database and materializes state for all registered projections.
- **D-31:** On startup, users register their `ReadModel` types (via `library.register::<OrderSummary>()` or the `inventory` crate pattern). The library syncs their `ProjectionDefinition` JSON to the `projection_definitions` table. If the definition hash changes, the projection is flagged for replay.
- **D-32:** The library database owns: `events`, `event_schemas`, `projection_definitions`, `projections` (checkpoint + raw `serde_json::Value` state), `observers`. The application database owns the actual read model storage (could be Redis, PostgreSQL, graph DB, or anything). For v1, raw state is temporarily materialized in the library DB as `serde_json::Value`.
- **D-33:** Two observer invocation modes on `EventLog`: `register_inline(observer)` (synchronous, blocks `append`) and `register_eventual(observer)` (background, eventual consistency). The `Observer` trait is the same for both — the difference is registration.

### projection! Macro — Query Language DSL (Proposal H — in progress)

The `projection!` macro is a **query language**, not a Rust-mimicking DSL. It does not try to look like Rust. Design is informed by SQL, GraphQL, and pipe-based languages.

**Settled syntax (Proposal H — updated for tag model):**

Phase 4 (single-stream):
```
projection OrderView {
    query tag.starts_with("order:") as o

    status:  o.OrderPlaced.status
           | o.OrderCancelled = "cancelled"
           |? "draft"

    total:  o.OrderPlaced.total_cents
          |+ o.ItemAdded.price_cents
          |- o.ItemRemoved.price_cents
          |? 0

    items {
        key: $tags.item
        removed_by: o.ItemRemoved | o.ItemArchived

        name:     o.ItemAdded.name
        quantity: o.ItemAdded.quantity
                |+ o.ItemUpdated.delta
                |? 0
        note?:    o.ItemNoteAdded.text
    }
}
```

Phase 5 extension (design locked, not implemented in Phase 4):
```
projection OrderView {
    query tag.starts_with("order:") as o
    join tag.starts_with("product:") as p    // on implicit: $tags.product

    items {
        key: $tags.item
        join tag.starts_with("product:") as p  // list-level join; on implicit: $tags.product
        removed_by: o.ItemRemoved | o.ItemArchived

        name:     o.ItemAdded.name | p.ProductUpdated.name
        quantity: o.ItemAdded.quantity |+ o.ItemUpdated.delta |? 0
    }

    product_name?: p.ProductUpdated.name |? "unknown"
}
```

**Settled rules:**
- `field:` — required (`T`). `field?:` — optional (`Option<T>`). Required by default.
- `|` — pipe: on this event, assign. `|+` increment. `|-` decrement. `|?` default (always last).
- `o.EventType.field` — event field reference via stream alias + event type + field name.
- `o.EventType = "literal"` — literal value assignment from a specific event.
- Required and default are independent: `|? value` sets initial state regardless of `?:` suffix.
- `{}` appears only on list fields, not on scalar fields.
- `query tag.starts_with("X:") as alias` declares the primary stream. Alias used to reference events.
- `key: $tags.X` — list item key from tag prefix. `key: $tags.X, $tags.Y` for composite keys.
- `key: $.field` — list item key from event payload field (fallback for non-DCB events).
- `removed_by: EventType | OtherEvent` — event type names only; engine matches via item's `$tags` key. Fallback with explicit field: `removed_by: o.EventType on $.item_id`.
- `join tag.starts_with("X:") as alias` — Phase 5. `on` implicit from prefix via `$tags`. Explicit `on $.field` available for non-DCB events.

**Resolved items (previously open):**
- `removed_by` syntax: event type names only (tag-based key). Placement inside list block is correct — it is a list-level control operation. Field-path syntax available as fallback.
- Composite list keys: `key: $tags.order, $tags.item` (tag-based) or `key: { fields: ["$.order_id", "$.item_id"] }` (field-based).
- Joins inside list fields: list-level `join` block (Phase 5). Design locked above.

**Deferred to Phase 10:**
- GROUP BY and window functions in the DSL (Phase 10 covers both the JSON schema extension and the query language surface).

### Claude's Discretion

- Internal module layout within `event-sourcing` core for `EventSchemaDef`, `ProjectionDefinition`, `ProjectionEngine`, and `ReadModel`
- Internal Rust representation of `ProjectionDefinition` (struct vs enum hierarchy)
- Whether `ProjectionEngine` is a struct or a module of free functions
- JSON Path evaluation library choice (or hand-rolled for simple `$.field` paths)
- Error variants in `ProjectionError` and `EventSchemaError`
- Test structure and coverage scope
- Whether `EventLog<S, E>` uses two type parameters or a combined store abstraction

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Prior Phase Contracts
- `.planning/phases/03.1-dcb-model-revision-storedevent-streamid-and-sequence-id-accuracy/03.1-CONTEXT.md` — Authoritative source for `StoredEvent`, `Tag`, `Query`, `Criterion`, `AppendCondition`, `LogStore` trait.
- `.planning/phases/03.2-keyed-tag-and-tag-filter/03.2-CONTEXT.md` — **Must be implemented before Phase 4.** Defines `TagFilter` enum (`Equals`, `StartsWith`, `EndsWith`, `And`, `Or`) and `Criterion.tag_filter: Option<TagFilter>`. `ProjectionDefinition.query` uses this type.

### Requirements
- `.planning/REQUIREMENTS.md` — PROJ-01, PROJ-03, PROJ-04, PROJ-06, PROJ-07, PROJ-08 are the requirements for this phase.
- `.planning/PROJECT.md` — Core value: "The projection engine is the heart." Constraints: Rust library, type safety, unopinionated.

### Technology Stack
- `CLAUDE.md` — Proc-macro crate pattern: separate crate with `proc-macro = true`, re-exported from core behind feature flag. Stack versions for `serde`, `serde_json`, `syn 2.x`, `quote`, `proc-macro2`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `event-sourcing/src/event.rs` — `StoredEvent` is the input type to the projection engine. Fields: `global_sequence`, `stream_id`, `stream_sequence`, `event_type: String`, `payload: serde_json::Value`, `timestamp`. **Subject to revision in Phase 03.1.**
- `event-sourcing/src/event_log.rs` — `EventLog::read_stream` is how callers get event iterators to pass to `project`. Phase 4 adds `EventSchemaStore` as a second type parameter.
- `event-sourcing/src/lib.rs` — Public re-export pattern to follow. Add `Event`, `EventSchemaDef`, `FieldType`, `ReadModel`, `ProjectionDefinition`, `ProjectionEngine`, `EventSchemaStore` here.

### Established Patterns
- Static dispatch throughout: `S: LogStore` generics, native `async fn in trait`. `ProjectionEngine` and `EventSchemaStore` should follow the same pattern — no `dyn`, generic where needed.
- `thiserror` for all error types: `ProjectionError`, `EventSchemaError`, `SchemaConflictError` should all use `#[derive(thiserror::Error)]`.
- No re-exports from core except at `lib.rs`.

### Integration Points
- Phase 03.1 revises `StoredEvent` — Phase 4 must consume whatever structure Phase 03.1 settles on.
- Phase 5 extends `ProjectionEngine` with multi-stream join support and `project_from` for catch-up reads.
- Phase 6 (Constraint Validation) uses `ProjectionEngine` to validate invariants before appends.
- Phase 7 (Observer Infrastructure) introduces `ProjectionObserver` which calls `project_from` with a stored checkpoint. Also owns startup `validate_schemas()` orchestration.

</code_context>

<specifics>
## Specific Ideas

- The `projection!` macro is the primary user-facing interface. The builder API is the internal mechanism that the macro generates calls to — advanced users can also call it directly.
- `ProjectionCheckpoint` is opaque to callers — they store and restore it, but do not interpret its internals.
- A `required: true` field with a `default` value starts populated in initial state. A `required: true` field without a `default` is null in raw state until an event sets it — deserialization of `M` will fail until then.
- JSON Path `$.` convention aligns with standard JSON Path. For Phase 4, simple paths (`$.field`, `$.nested.field`) are sufficient. Arrays in payloads (e.g., `$.items[0].id`) are a known limitation — see deferred ideas.
- `EventSchemaDef` records are append-only. The library never updates or deletes them. Old records survive even if the Rust event type is deleted from code, keeping the projection engine functional for historical events.
- `validate_schemas()` is explicit — the library never panics on its own. The application calls it at startup and handles the result.

</specifics>

<deferred>
## Deferred Ideas

### Projection-aware schema conflict detection
Tracked in `.planning/todos/pending/projection-aware-schema-conflict.md`. Currently using simple semantic checking (field removed or retyped = conflict). Opportunity to tighten: only flag a conflict if a registered `ProjectionDefinition` references the changed field. Defer until `EventSchemaStore` and projection registration are both stable.

### Joins (Phase 5)
Full join design is locked in this context (see Phase 5 extension schema) but not implemented. Phase 5 adds `joins` object at root and list levels, and `join` as an object on fields (alias → events map). The `on` field resolves against the containing projection state to find the joined stream ID.

### Temporal Joins (new phase after Phase 5)
Point-in-time join semantics: limit a joined stream's events to those with `GlobalSequenceId ≤` the sequence of the triggering root stream event. Use case: "product name at the time the item was added, not today's name."

### Transformation Functions (new phase after Phase 5)
User-defined named functions registered at startup and called during the projection fold. Enables computed fields (`"transform": "calculate_tax"`) that can't be expressed in the JSON schema.

### Read Model Persistence / Library DB Schema (new phase after Phase 7)
Covers: `ProjectionStore` trait, library DB table schema, application DB separation, typed read model conversion.

### Events Containing Array Payloads (`from_array`)
Known limitation: projections process one event at a time. An event whose payload contains a list of items cannot fan out to multiple list upserts. Workaround: emit one event per item. README should document this limitation.

### ReadModelStore trait (Phase 7+)
Optional trait `ReadModelStore<M: ReadModel>` for users who want to persist read models in their own storage (Redis, Postgres, etc.). Methods: `get(key)`, `save(&self, read_model)`, `delete(&self, read_model)`, `configure(&self)` (called when projection definition changes — creates/updates/deletes backing schema). The observer calls this after updating its checkpoint. Defer until Phase 7 observer infrastructure is designed.

</deferred>

---

*Phase: 04-projection-engine-single-stream*
*Context updated: 2026-04-06 (macro DSL discussion in progress — removed_by and list joins unresolved)*
