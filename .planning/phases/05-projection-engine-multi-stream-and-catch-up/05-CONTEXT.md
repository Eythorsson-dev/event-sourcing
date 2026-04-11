# Phase 5: Projection Engine — Multi-Stream and Catch-Up - Context

**Gathered:** 2026-04-09, updated 2026-04-10
**Status:** Partial — join design captured and revised from Phase 4/5 discussion sessions. Requires `/gsd-discuss-phase 05` before planning to cover catch-up reads and remaining details.

<open_questions>
## Open Questions

### ~~OQ-01: `$tags` vs `tag_starting_with()` for implicit join resolution and list keys~~ — RESOLVED

**Decision: Drop `$tags` entirely (beyond Option B).**

`$tags` as a concept is removed from the projection engine. The reasons go beyond the original framing:
1. **No enforced tag format** — the library does not require `key:value` tag structure, so any feature that parses tag prefixes is built on an unenforced convention.
2. **Batch events break the scalar assumption** — an event tagged `["item:i1", "item:i2", "item:i3"]` cannot be represented by a single `$tags.item` value. This is not an edge case; batch operations are common in event sourcing.
3. **Multi-role same-prefix ambiguity** — a projection referencing two `user:` tags in different roles (accountant vs. responsible) cannot be distinguished by prefix alone.

**What replaced it:**
- **List keys:** Always payload field paths — `key: $.item_id` (DSL) / `"key": "$.item_id"` (JSON). See Phase 4 D-27.
- **Join resolution:** Explicit `for EventType` (engine extracts join key from specified event's tags) or `on $.field` (reads from projected state). See D-04–D-08 below.
- **Tag extraction expressions:** `tag_starting_with()` / `tag_ending_with()` remain available as per-event extraction utilities (D-09–D-13 below) for cases where tag values need to be projected as fields — but they are not used for list keys or implicit join resolution.

</open_questions>

<domain>
## Phase Boundary

Extend the projection engine with multi-stream joins, list-level joins, and inline catch-up reads. The single-stream engine from Phase 4 is the foundation — this phase adds the `joins` block at root and list levels, and implements the runtime mechanics of join resolution, replay, and cycle detection.

The `$tags` design question (OQ-01) has been resolved — `$tags` is dropped entirely. List keys use payload field paths, join resolution uses explicit `for EventType` or `on $.field`.

**Depends on:** Phase 4 (single-stream engine, `TagFilter`, `ProjectionDefinition.query`)

</domain>

<decisions>
## Implementation Decisions

### Join Declaration

- **D-01:** Joins are declared at two levels:
  - **Root-level** — one joined stream instance for the whole projection
  - **List-level** — one joined stream instance per list item (declared inside the list block)
- **D-02:** Join syntax in DSL:
  ```
  // Basic join with event-scoped resolution
  join tag.starts_with("product:") as p for ItemAdded

  // Multiple events can update the same join key
  join tag.starts_with("product:") as p for ItemAdded | ItemProductChanged

  // Payload-field resolution (non-DCB events)
  join tag.starts_with("product:") as p on $.product_id

  // Static join — TagFilter::Equals, always the same instance
  join tag.equals("global-config") as cfg
  ```
  Multiple joins are allowed at the same level with distinct aliases.
- **D-03:** JSON schema for root-level joins:
  ```json
  "joins": {
    "p": {
      "query": { "StartsWith": "product:" },
      "for": ["ItemAdded", "ItemProductChanged"]
    },
    "cfg": {
      "query": { "Equals": "global-config" }
    }
  }
  ```
  For list-level joins, the same structure appears inside the list field's object:
  ```json
  "items": {
    "type": "list",
    "key": { "tag_starting_with": "item:" },
    "joins": {
      "p": {
        "query": { "StartsWith": "product:" },
        "for": ["ItemAdded"]
      }
    },
    "fields": { ... }
  }
  ```

### Join Resolution Strategies (replaces D-04–D-08 implicit `$tags` design)

- **D-04:** Three explicit join resolution strategies — every join must use one:

  | Strategy | Syntax | How key is resolved |
  |---|---|---|
  | Event-scoped | `for EventType` | Engine extracts the join's matching tag from the specified event type(s) when they arrive |
  | Payload field | `on $.field` | Engine reads the join key from a projected field in the current read model state |
  | Static | *(no `for`/`on`)* | Valid only for `TagFilter::Equals(tag)` joins — stream is always the same instance |

- **D-05:** `for EventType` resolution detail: when one of the specified event types arrives, the engine extracts the tag matching the join's `TagFilter` from that event's tags and uses it as the join key for this alias. Multiple event types: `for ItemAdded | ItemProductChanged` — any of them can update the join key. The last seen event wins (supports key changes at runtime — see D-12).

- **D-06:** `on $.field` resolution detail: the join key is read from the named field in the current projection state at the time the join needs to resolve. If the field is null (not yet projected), the join contributes no events (left join semantics — see D-15). Useful for non-DCB events where the referenced entity ID lives in the payload rather than the tags.

- **D-07:** `for EventType` and `on $.field` are the only two dynamic resolution strategies. There is no implicit resolution from accumulated tag state — `$tags` as a user-visible magic map in the read model is dropped entirely. Read model state contains only explicitly declared field projections.

- **D-08:** **Application responsibility for tagging:** Events must carry tags for all entities they participate in. `for ItemAdded` on a `product:` join only works if `ItemAdded` events carry a `product:X` tag. The engine emits a validation warning at `ProjectionDefinition` registration time if a declared `for` event type has no matching tag for the join's `TagFilter` (best-effort static analysis).

### Tag Extraction Expressions

- **D-09:** `tag_starting_with("prefix:")` — extracts the suffix value from the first tag on the current event whose string starts with `prefix:`. Example: `tag_starting_with("item:")` on an event tagged `item:i1` returns `"i1"`.

- **D-10:** `tag_ending_with(":suffix")` — extracts the prefix value from the first tag on the current event whose string ends with `:suffix`. Example: `tag_ending_with(":admin")` on an event tagged `user:admin` returns `"user"`.

- **D-11:** Both expressions evaluate against **the current event being processed** — not accumulated state. They return null if no matching tag is found on that event.

- **D-12:** When `tag_starting_with()` or `tag_ending_with()` is used as a list `key` and **multiple tags match** on one event (e.g., `item:i1` and `item:i2` both present):
  - In `key` position: the engine routes the event to **all matching list items** — supports bulk update events naturally.
  - In field position: registration-time warning is emitted; first match is used at runtime. Developer should narrow the filter or project a payload field instead.

- **D-13:** If a `key` expression evaluates to null (the arriving event carries no matching tag), the event cannot be routed to any list item and is silently skipped for that list. A validation warning is emitted at `ProjectionDefinition` registration if this can be statically detected.

### Join Key Changes and Replay

- **D-14:** Join keys can change at runtime — a subsequent `for` event arriving with a different tag value updates the join key for that alias. This is a supported use case (e.g., `ItemProductChanged` changes which product the item references).

- **D-15:** When a join key changes (root or list-item level), the engine performs a **full replay from scratch**: reprocesses all events for the root stream and the new joined stream from the beginning. The old joined stream's events are excluded.

- **D-16:** Replay is bounded by the `GlobalSequenceId` ceiling at the time the projection was triggered. No new events are fetched during replay — only already-stored events up to the ceiling are reprocessed.

- **D-17:** **Left join semantics**: when a join key is not yet established (no `for` event seen, or `on $.field` is null), the join contributes no events. Once the key is established, a full replay is triggered to fold in the joined stream's events from the beginning.

- **D-18:** In practice, replay of entity-scoped projections is fast — an order might have tens to hundreds of events. Full replay is acceptable for v1.

### Circular Join Dependency Detection

- **D-19:** Circular join dependencies (join key A → B → A) are detected by tracking the sequence of join key configurations across replay passes. If the same `{alias → instance_tag}` configuration recurs, a cycle is confirmed.
- **D-20:** Detection algorithm: after each replay pass, record the resolved join key set. If the set matches any previously seen set in this replay chain → cycle detected.
- **D-21:** On cycle detection: `ProjectionError::CircularJoinDependency { trace: Vec<JoinKeySnapshot> }` — the trace shows the full oscillation sequence so the developer can identify the cause.
- **D-22:** In Phase 7 (Observer Infrastructure), a circular join dependency surfaces as `ObserverResult::Failed` — requires developer intervention. It is not retried.
- **D-23:** No arbitrary pass cap. Cycle detection is structural — if a true cycle exists it is detected and errored; if it does not exist, the replay converges (guaranteed by determinism of the fold given a fixed event ceiling).

### List-Level Join Resolution

- **D-24:** The engine maintains an internal join resolution map (not user-visible, not part of read model state): `{alias, join_key} → [item_keys]`. When a `for` event arrives for a list item (identified by the list `key` expression), the engine records the resolved join key for that item in the map.

- **D-25:** When a joined stream event arrives (e.g., `ProductUpdated` tagged `product:p1`), the engine looks up the internal map for all list items whose resolved key for the `p` alias is `"p1"` and routes the event to those items.

- **D-26:** One joined stream event can update multiple list items simultaneously — all items whose `for` event established the same join key value.

- **D-27:** If no list items have a matching internal join key entry for the arriving joined event, the event is silently ignored for that list (left join behavior).

### Flat Scope — Multiple Named Joins for Same Tag Prefix

- **D-28:** When a projection needs multiple distinct relationships of the same tag prefix (e.g., two different users in different roles), declare multiple named joins disambiguated by their `for` event types:
  ```
  join tag.starts_with("user:") as u_acc for AccountingResponsibleAssigned
  join tag.starts_with("user:") as u_pay for PayrollResponsibleAssigned
  ```
  Each join alias resolves independently — `AccountingResponsibleAssigned` updates only `u_acc`, `PayrollResponsibleAssigned` updates only `u_pay`.

- **D-29:** The flat scope (multiple named join fields at the same level) is preferred over a list when the relationships are semantically distinct with different field shapes. The list scope is preferred when relationships are uniform and variable in number.

### Multi-Stream Event Ordering

- **D-30:** Events from multiple streams (root + all joins) are merged and processed in `GlobalSequenceId` order. This is the same ordering guarantee the `LogStore` provides.
- **D-31:** The same `ProjectionEngine` handles both read model queries and constraint checks (PROJ-05) — no separate engine for constraints.

### Catch-Up Reads

- **To be discussed:** Inline catch-up semantics, `project_from` with `ProjectionCheckpoint`, gating on a specific `GlobalSequenceId`. Covered in `ProjectionCheckpoint` (D-20 in Phase 4 context) but runtime mechanics need full discussion.

### Claude's Discretion

- Internal representation of the join resolution map (`{alias, join_key} → [item_keys]`) during fold — whether it lives alongside the read model state or in a separate parallel structure
- Whether root-level and list-level joins share implementation or have separate code paths
- Error variant structure for `ProjectionError::CircularJoinDependency`
- Validation warning mechanism at registration time (static analysis of `for` event type tag coverage)
- Whether `tag_starting_with` / `tag_ending_with` are represented as an enum variant in the `ProjectionDefinition` field value type or as a dedicated key expression type

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Prior Phase Contracts
- `.planning/phases/03.2-keyed-tag-and-tag-filter/03.2-CONTEXT.md` — `TagFilter` type used in join `query` declarations
- `.planning/phases/04-projection-engine-single-stream/04-CONTEXT.md` — Single-stream engine, `ProjectionDefinition` schema, `ProjectionCheckpoint` (D-20), `ReadModel` trait, `ProjectionEngine` API
  - **Note:** D-34–D-36 (`$tags`) were removed from Phase 4 context. `$tags` is dropped entirely — see OQ-01 resolution above and Phase 4 D-27 revision.

### Requirements
- `.planning/REQUIREMENTS.md` — PROJ-02 (multi-stream joins), PROJ-05 (same engine for read models and constraints), LOG-08 (inline catch-up)

</canonical_refs>

<deferred>
## Deferred Ideas

- Temporal joins: point-in-time join semantics (limit joined stream events to those with `GlobalSequenceId ≤` the root event that triggered the join). Use case: "product name at the time the item was added." Post-Phase 5.
- Transformation functions: user-defined named functions called during the fold. Post-Phase 5.
- Polymorphic list items: list items with different field shapes based on a type discriminator. Tracked in `.planning/todos/pending/projection-derived-fields.md`. Post-Phase 5.

</deferred>

---

*Phase: 05-projection-engine-multi-stream-and-catch-up*
*Context gathered: 2026-04-09, updated 2026-04-10*
