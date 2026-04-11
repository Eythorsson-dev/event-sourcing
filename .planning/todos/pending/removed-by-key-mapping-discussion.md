---
title: Discuss removed_by key mapping before planning Phase 4
area: projection-macro
created: 2026-04-06
status: resolved
resolved: 2026-04-09
---

# removed_by key mapping — RESOLVED

Resolved during Phase 4 discuss-phase session 2026-04-09.

## Decisions

- `removed_by` lives inside the list block (correct placement — it is a list-level control operation)
- Syntax: `removed_by: EventType | OtherEvent` — event type names only, no field paths
- Engine extracts the key from the removal event's payload using the list's `key` field path declaration, then removes the matching list item
- Explicit field override: `removed_by: o.EventType on $.item_id`

## Composite keys

Resolved: `key: $.order_id, $.item_id` — multiple payload field paths.

## $tags removal

`$tags`-based key resolution was dropped entirely. Tags are for consistency boundaries (`TagFilter`, `AppendCondition`); projections use payload field paths for all identity and data resolution. See Phase 4 CONTEXT.md D-27 and DSL settled rules.
