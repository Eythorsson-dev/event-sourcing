# Phase 5: Projection Engine — Multi-Stream and Catch-Up - Context

**Gathered:** 2026-04-09
**Updated:** 2026-04-26
**Status:** Partial — join design settled in 2026-04-26 session. Catch-up reads and multi-stream fetch API still need discussion before planning.

<domain>
## Phase Boundary

Extend the projection engine with multi-stream joins at root level and nested object level, and inline catch-up reads. The single-stream engine from Phase 4 is the foundation.

Phase 5 scope:
- `join` blocks at root level and inside object field blocks
- Tag-based join resolution via `on` keyword
- `removed_by` rename from `cleared_by` (breaking DSL/JSON change — apply before Phase 5.1 to avoid migrating tests twice)
- Inline catch-up reads (to be discussed)

Phase 5 **not** in scope (Phase 5.1):
- List fields (`field[]:` syntax, list-level joins, `removed_by` for list items)

**Depends on:** Phase 4.1 (projection! macro syntax revision, current DSL baseline)

</domain>

<decisions>
## Implementation Decisions

### JSON Schema: Structural Changes from Phase 4

- **D-01:** Drop `"events"` wrapper — REVERTED. `"events"` stays as an explicit wrapper on field specs. With `required` and `default` as sibling keys, keeping `"events"` makes it visually clear where metadata ends and event handlers begin. It also preserves `#[serde(deny_unknown_fields)]` schema safety on the struct.

- **D-02:** **Breaking change: rename `cleared_by` → `removed_by`** on `ObjectFieldSpec`. Applied in Phase 5 to avoid migrating tests twice when Phase 5.1 introduces list item removal with the same keyword. Tracked in `.planning/todos/pending/rename-cleared_by-to-removed_by.md`.

### JSON Schema: Join Declaration

- **D-03:** `join` is a **named map of alias → JoinSpec** at every level — root, object, list. The field name is always `"join"` (not `"joins"`). Multiple joins per scope use distinct alias keys in the same map.

  Root-level example:
  ```json
  {
    "name": "CustomerView",
    "query": { "StartsWith": "customer:" },
    "join": {
      "co": {
        "query": { "StartsWith": "company:" },
        "on": ["c.CompanyAssigned"]
      }
    },
    "fields": {
      "name": {
        "events": { "CustomerRegistered": { "from": "$.name" } }
      },
      "company_name": {
        "events": { "co.CompanyRegistered": { "from": "$.name" } }
      }
    }
  }
  ```

  Object-level example:
  ```json
  "contact_person": {
    "type": "object",
    "removed_by": ["AccountantRemoved"],
    "join": {
      "cp": {
        "query": { "StartsWith": "employee:" },
        "on": ["c.ContactPersonAssigned"]
      }
    },
    "fields": {
      "full_name": {
        "events": {
          "ContactPersonAssigned":    { "from": "$.full_name" },
          "cp.ContactDetailsUpdated": { "from": "$.full_name" }
        }
      }
    }
  }
  ```

  Multiple joins at the same level:
  ```json
  "join": {
    "u_acc": { "query": { "StartsWith": "user:" }, "on": ["c.AccountingResponsibleAssigned"] },
    "u_pay": { "query": { "StartsWith": "user:" }, "on": ["c.PayrollResponsibleAssigned"] }
  }
  ```

- **D-04:** Root-level joins are in Phase 5 scope. Object-level and root-level joins use the same `JoinSpec` type and the same engine resolution mechanics.

### Join Resolution: `on` Keyword

- **D-05:** Join resolution is **tag-based via the `on` keyword**. `on` is always required on a `join` declaration — there is no implicit resolution without naming the source. Exception: static joins (`TagFilter::Equals`) have no `on` because the stream instance is fixed and always the same.

- **D-06:** `on` semantics:

  | DSL form | JSON form | Meaning |
  |---|---|---|
  | `on c` | `"on": "c"` | Any event from alias `c` that carries a matching tag establishes the join key |
  | `on c.EventType` | `"on": ["c.EventType"]` | Only that specific event type establishes the join key |
  | `on c.E1 \| c.E2` | `"on": ["c.E1", "c.E2"]` | Either event can establish the join key |

  When an in-scope event arrives and carries a tag matching the join's `query` pattern, the engine uses that tag value as the joined-stream instance key. Last seen wins if the tag value changes.

- **D-07:** The `on` source alias must be in scope at the level where the join is declared. For root-level joins, `on` references the primary query alias (`c`). For object-level joins, `on` may reference the primary alias or any root-level join alias.

- **D-08:** When multiple joins share the same `query` pattern (e.g., two `user:` joins), they must each use distinct `on` event types for disambiguation. The engine errors at registration time if two joins at the same level share a pattern and neither constrains `on` to disjoint event types.

- **D-09:** Static joins (`TagFilter::Equals`) have no `on` field — the stream is always the same single instance. The engine identifies them by the absence of `on` combined with the `Equals` filter shape.

- **D-10:** `on $.field` (payload-field resolution for non-DCB events where the entity ID is in the payload, not the tags) is **deferred**. Tracked as a todo for a future phase. Phase 5 implements tag-based resolution only.

### Event Handler Keys: Alias Prefixing

- **D-11:** Primary-stream event types in `events` maps are always **bare names** in JSON (e.g., `"CustomerRegistered"`), consistent with Phase 4 D-34. This holds at all scopes — root, inside object blocks, inside list item blocks.

- **D-12:** Joined-stream event types in `events` maps are always prefixed with their alias in JSON: `"cp.ContactDetailsUpdated"`, `"o.OrderPlaced"`. The alias prefix is the sole discriminator between primary and joined stream events.

- **D-13:** In the DSL, primary stream events may be written either bare (`ContactPersonAssigned`) or with the primary alias (`c.ContactPersonAssigned`) inside nested scopes — both are valid. The macro normalizes to bare when emitting JSON. Joined stream events always require the alias prefix in both DSL and JSON.

### DSL: `join ... on` Syntax

- **D-14:** `join` syntax additions to the Phase 4.1 DSL baseline:

  ```
  // Root-level join (any event from c establishes the key):
  projection CustomerView {
      query tag.starts_with("customer:") as c
      join tag.starts_with("company:") as co on c

      name:         c.CustomerRegistered.name
      company_name: co.CompanyRegistered.name

      contact_person? {
          join tag.starts_with("employee:") as cp on c.ContactPersonAssigned

          full_name: c.ContactPersonAssigned.full_name
                   | cp.ContactDetailsUpdated.full_name
      } removed_by c.ContactPersonRemoved
  }
  ```

  Rules:
  - `join tag.<filter>("X:") as <alias> on <source>` — declares one join, adds one entry to the local `join` map
  - Multiple `join` lines in the same block produce multiple entries in the map
  - `on <alias>` — any event from that alias
  - `on <alias>.<EventType>` — specific event
  - `on <alias>.<E1> | <alias>.<E2>` — multiple events (pipe-separated)
  - `join` can appear at root level (outside any field block) or inside an object block
  - List-level `join` (`orders[]: { join ... }`) is Phase 5.1 syntax — not parsed in Phase 5

### Join Key Changes and Replay

- **D-15:** Join keys can change at runtime. If an `on` event arrives with a tag value that differs from the previously resolved join key for that alias, the join key updates and the engine performs a **full replay from scratch** — reprocessing all events for the primary stream and the new joined stream from the beginning. Old joined stream events are excluded.

- **D-16:** Replay is bounded by the `GlobalSequenceId` ceiling at the time the projection was triggered. No new events are fetched during replay.

- **D-17:** Left join semantics: when a join key is not yet established (no `on` event seen), the join contributes no events. Once established, a full replay is triggered to fold in the joined stream's events from the beginning.

- **D-18:** Full replay is acceptable for v1. Entity-scoped projections have tens to hundreds of events in practice.

### Circular Join Dependency Detection

- **D-19:** Circular join dependencies (key A → B → A oscillation) are detected by tracking the join key configuration after each replay pass. If the same `{alias → instance_tag}` set recurs, a cycle is confirmed.
- **D-20:** On cycle detection: `ProjectionError::CircularJoinDependency { trace: Vec<JoinKeySnapshot> }` — trace shows the full oscillation sequence.
- **D-21:** No arbitrary pass cap. Cycle detection is structural — true cycles are detected and errored; converging replays run to completion.
- **D-22:** In Phase 7 (Observer Infrastructure), a circular dependency surfaces as `ObserverResult::Failed` — not retried, requires developer intervention.

### Multi-Stream Event Ordering

- **D-23:** Events from all streams (primary + all resolved join instances) are merged and processed in `GlobalSequenceId` order.
- **D-24:** The same `ProjectionEngine` handles both read-model queries and constraint checks (PROJ-05).

### Catch-Up Reads

**To be discussed:** Inline catch-up semantics, `project_from` with `ProjectionCheckpoint`, gating on a specific `GlobalSequenceId`, and how the engine coordinates event fetching across multiple streams. Discuss before planning.

### Claude's Discretion

- Serde representation of `JoinOn` (string vs array in JSON, enum variant shape)
- Internal representation of the join resolution map during fold
- Whether root-level and object-level joins share one code path or have separate implementations
- Error variant structure for `ProjectionError::CircularJoinDependency`
- Registration-time validation of `on` event type tag coverage (best-effort static analysis)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Prior Phase Contracts
- `.planning/phases/03.2-keyed-tag-and-tag-filter/03.2-CONTEXT.md` — `TagFilter` type used in join `query` declarations
- `.planning/phases/04-projection-engine-single-stream/04-CONTEXT.md` — `ProjectionDefinition` JSON schema baseline, `HandlerSpec`, `ScalarFieldSpec`, `ObjectFieldSpec` (has `cleared_by` — renamed to `removed_by` in Phase 5), `ProjectionCheckpoint` (D-20), `ReadModel` trait, `ProjectionEngine` API
- `.planning/phases/04.1-projection-macro-syntax-revision/04.1-01-PLAN.md` — Current DSL syntax baseline that Phase 5 extends with `join ... on`
- `.planning/phases/04.2-projection-engine-list-fields-inserted/04.2-CONTEXT.md` — Absorbed phase; join design decisions (D-08–D-17) that fed into this context

### Pending Work
- `.planning/todos/pending/rename-cleared_by-to-removed_by.md` — Breaking DSL/JSON change to apply in Phase 5

### Requirements
- `.planning/REQUIREMENTS.md` — PROJ-02 (multi-stream joins), PROJ-05 (same engine for read models and constraints), LOG-08 (inline catch-up)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `event-sourcing/src/projection/definition.rs` — `ObjectFieldSpec` (has `cleared_by: Vec<EventType>` — rename to `removed_by`), `ScalarFieldSpec`, `FieldSpec`, `HandlerSpec`, `ProjectionDefinition`. Phase 5 adds `JoinSpec` and `JoinOn` types; extends `ObjectFieldSpec` with `join: Option<IndexMap<String, JoinSpec>>`; adds `join` to `ProjectionDefinition` at root level.
- `event-sourcing/src/query.rs` — `TagFilter` with `StartsWith`, `Equals`, `And`, `Or` variants. Reused as `JoinSpec.query` type.
- `event-sourcing/src/projection/engine.rs` — Current single-stream fold. Phase 5 extends with multi-stream merge and join resolution.
- `event-sourcing-macros/src/projection_macro.rs` — `parse_field_decl` is the extension point for `join` keyword parsing.

### Established Patterns
- `IndexMap<EventType, HandlerSpec>` for event handler registration — joined stream handlers use `"alias.EventType"` string keys in the same map
- `#[serde(untagged)]` on `FieldSpec` discriminates `Object` from `Scalar` via the `"type"` key — a future `List` variant (Phase 5.1) needs `"type": "list"`
- `#[serde(deny_unknown_fields)]` on all field spec structs for schema safety — preserved by keeping `"events"` wrapper

### Integration Points
- Phase 6 (Constraint Validation) uses `ProjectionEngine` to validate invariants before appends — same engine, no changes needed in Phase 6
- Phase 5.1 (List Fields) adds `ListFieldSpec` with `join` inside it, using Phase 5's `JoinSpec` type
- Phase 7 (Observer Infrastructure) calls `project_from` with a stored checkpoint — catch-up API designed in Phase 5 is consumed here

</code_context>

<specifics>
## Specific Ideas

- The `join` map key is the alias string (e.g., `"cp"`, `"o"`) — the same alias used as the prefix in `"alias.EventType"` event handler keys. Aliases are local to their declaration scope.
- For the `on` JSON representation: `"on": "c"` (bare string = any event from alias c) vs `"on": ["c.EventType"]` (array = specific events). Serde untagged enum handles the string/array distinction. Single-event case can use either form.
- Static joins (`TagFilter::Equals`) omit `on` entirely. Engine identifies them by `Equals` filter shape + absent `on` field.
- The `removed_by` rename is a breaking change to the JSON schema (field name change in `ObjectFieldSpec`) and DSL keyword. All Phase 4 tests using `cleared_by` must be updated in Phase 5.

</specifics>

<deferred>
## Deferred Ideas

- **`on $.field` — payload-field join resolution:** For non-DCB events where the referenced entity ID lives in the payload, not the tags. Deferred — needs its own todo/phase.
- **`query` → `from` keyword rename:** Open question deferred to Phase 5.2 (ESQL macro refactor). Should `query tag.starts_with("X:") as c` become `from tag.starts_with("X:") as c`? SQL alignment argument; breaking DSL change.
- **Temporal joins:** Point-in-time join semantics (limit joined stream events to `GlobalSequenceId ≤` the root event that triggered the join). Use case: "product name at the time the item was added." Post-Phase 5.
- **Transformation functions:** User-defined named functions called during the fold. Post-Phase 5.

</deferred>

---

*Phase: 05-projection-engine-multi-stream-and-catch-up*
*Context gathered: 2026-04-09, updated 2026-04-26*
