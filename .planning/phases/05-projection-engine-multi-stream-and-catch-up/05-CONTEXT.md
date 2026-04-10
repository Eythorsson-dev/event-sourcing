# Phase 5: Projection Engine — Multi-Stream and Catch-Up - Context

**Gathered:** 2026-04-09
**Status:** Partial — join design captured from Phase 4 discussion session. Requires `/gsd-discuss-phase 05` before planning to cover catch-up reads and remaining details.

<domain>
## Phase Boundary

Extend the projection engine with multi-stream joins, list-level joins, `$tags`-based join resolution, and inline catch-up reads. The single-stream engine from Phase 4 is the foundation — this phase adds the `joins` block at root and list levels, and implements the runtime mechanics of join resolution, replay, and cycle detection.

**Depends on:** Phase 4 (single-stream engine, `$tags`, `TagFilter`, `ProjectionDefinition.query`)

</domain>

<decisions>
## Implementation Decisions

### Join Declaration

- **D-01:** Joins are declared at two levels:
  - **Root-level** — one joined stream instance for the whole projection
  - **List-level** — one joined stream instance per list item (declared inside the list block)
- **D-02:** Join syntax in DSL: `join tag.starts_with("product:") as p` — the `TagFilter` and alias. Multiple joins are allowed: `join tag.starts_with("vendor:") as v` alongside `join tag.starts_with("product:") as p`.
- **D-03:** JSON schema for root-level joins:
  ```json
  "joins": {
    "p": { "query": { "StartsWith": "product:" } },
    "v": { "query": { "StartsWith": "vendor:" } }
  }
  ```
  For list-level joins, the same structure appears inside the list field's object:
  ```json
  "list_field": {
    "type": "list",
    "key": "$tags.item",
    "joins": {
      "p": { "query": { "StartsWith": "product:" } }
    },
    "fields": { ... }
  }
  ```

### Join Resolution — Implicit `on` via `$tags`

- **D-04:** For `TagFilter::StartsWith(prefix)` joins, `on` is **implicit**: the engine looks up `$tags[prefix]` on the current object (root projection state for root joins, item state for list joins) to find the instance tag value. No explicit `on` declaration needed.
- **D-05:** Explicit `on $.field` is available as a fallback for non-DCB events where the join key comes from an event payload field rather than a tag. Use: `join tag.starts_with("product:") as p on $.product_id`.
- **D-06:** For `TagFilter::Equals(tag)` joins, no `on` is needed — the joined stream is always the same instance (static join).
- **D-07:** For `TagFilter::And`, `Or`, `EndsWith` joins, explicit `on` is required — the engine cannot infer the prefix unambiguously.
- **D-08:** **Application responsibility for tagging:** Events must carry tags for all collections they participate in for implicit `on` to work. If `ItemAdded` is about both an item and a product, it should carry both `item:i1` and `product:p1` tags. The library does not pull join keys from event payloads automatically — that is the application's concern. The engine emits a validation warning at `ProjectionDefinition` registration time if a declared join prefix has no matching event type tags.

### $tags Population

- **D-09:** `$tags` on each read model object (root and list items) is populated from the tags of events that affected that object. Stores the **current** value per tag prefix — latest event wins per prefix, not a historical union.
- **D-10:** When an event arrives carrying tag `product:p2` for an item previously tagged `product:p1`, `$tags["product"]` updates to `"p2"`. Old values are replaced, not accumulated.
- **D-11:** `$tags` on list items is populated from the tags of events that upsert or update that item. `$tags` on the root projection is populated from the tags of events that affect the root (any field with `events` — not `join` — handlers).

### Join Key Changes and Replay

- **D-12:** Join keys can change at runtime — joined stream events are allowed to update `$tags`, which changes the resolved join key. This is a supported use case.
- **D-13:** When a join key changes (root or list-item level), the engine performs a **full replay from scratch** (Option A): reprocesses all events for the root stream and the new joined stream from the beginning. The old joined stream's events are excluded.
- **D-14:** Replay is bounded by the `GlobalSequenceId` ceiling at the time the projection was triggered. No new events are fetched during replay — only already-stored events up to the ceiling are reprocessed.
- **D-15:** **Left join semantics**: when `$tags[prefix]` is null (join key not yet set), the join contributes no events. Once an event sets the tag, a replay is triggered to fold in the joined stream's events from the beginning.
- **D-16:** In practice, replay of entity-scoped projections is fast — an order might have tens to hundreds of events. Full replay is acceptable for v1.

### Circular Join Dependency Detection

- **D-17:** Circular join dependencies (join key A → B → A) are detected by tracking the sequence of join key configurations across replay passes. If the same `{alias → instance_tag}` configuration recurs, a cycle is confirmed.
- **D-18:** Detection algorithm: after each replay pass, record the resolved join key set. If the set matches any previously seen set in this replay chain → cycle detected.
- **D-19:** On cycle detection: `ProjectionError::CircularJoinDependency { trace: Vec<JoinKeySnapshot> }` — the trace shows the full oscillation sequence so the developer can identify the cause.
- **D-20:** In Phase 7 (Observer Infrastructure), a circular join dependency surfaces as `ObserverResult::Failed` — requires developer intervention. It is not retried.
- **D-21:** No arbitrary pass cap. Cycle detection is structural — if a true cycle exists it is detected and errored; if it does not exist, the replay converges (guaranteed by determinism of the fold given a fixed event ceiling).

### List-Level Join Resolution

- **D-22:** When a joined stream event arrives (e.g., `ProductUpdated` tagged `product:p2`), the engine scans list items whose `$tags` contains a matching tag for the join's prefix. Items with `$tags["product"] == "p2"` are updated; others are not.
- **D-23:** This allows one joined stream event to update multiple list items simultaneously (all items linked to the same product).
- **D-24:** If no list items have a matching `$tags` entry for the arriving joined event, the event is silently ignored for that list (left join behavior).

### Multi-Stream Event Ordering

- **D-25:** Events from multiple streams (root + all joins) are merged and processed in `GlobalSequenceId` order. This is the same ordering guarantee the `LogStore` provides.
- **D-26:** The same `ProjectionEngine` handles both read model queries and constraint checks (PROJ-05) — no separate engine for constraints.

### Catch-Up Reads

- **To be discussed:** Inline catch-up semantics, `project_from` with `ProjectionCheckpoint`, gating on a specific `GlobalSequenceId`. Covered in `ProjectionCheckpoint` (D-20 in Phase 4 context) but runtime mechanics need full discussion.

### Claude's Discretion

- Internal representation of join state during fold (how the engine tracks which joined stream each alias resolves to mid-replay)
- Whether root-level and list-level joins share implementation or have separate code paths
- How `$tags` is stored in `serde_json::Value` state (reserved key `$tags` in the JSON object)
- Error variant structure for `ProjectionError::CircularJoinDependency`
- Validation warning mechanism for missing join tag coverage at registration time

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Prior Phase Contracts
- `.planning/phases/03.2-keyed-tag-and-tag-filter/03.2-CONTEXT.md` — `TagFilter` type used in join `query` declarations
- `.planning/phases/04-projection-engine-single-stream/04-CONTEXT.md` — Single-stream engine, `ProjectionDefinition` schema, `$tags` (D-34–D-36), `ProjectionCheckpoint` (D-20), `ReadModel` trait, `ProjectionEngine` API

### Requirements
- `.planning/REQUIREMENTS.md` — PROJ-02 (multi-stream joins), PROJ-05 (same engine for read models and constraints), LOG-08 (inline catch-up)

</canonical_refs>

<deferred>
## Deferred Ideas

- Temporal joins: point-in-time join semantics (limit joined stream events to those with `GlobalSequenceId ≤` the root event that triggered the join). Use case: "product name at the time the item was added." Post-Phase 5.
- Transformation functions: user-defined named functions called during the fold. Post-Phase 5.

</deferred>

---

*Phase: 05-projection-engine-multi-stream-and-catch-up*
*Context gathered: 2026-04-09 (partial — from Phase 4 discussion)*
