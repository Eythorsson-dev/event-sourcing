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
- Engine matches removal events to items via `$tags` key: the removal event must carry the same tag prefix as the list's `key` declaration
- Fallback for non-DCB events: `removed_by: o.EventType on $.item_id` — explicit field path

## Composite keys

Resolved: `key: $tags.order, $tags.item` for composite tag-based keys.
Field fallback: `key: { fields: ["$.order_id", "$.item_id"] }`.

See Phase 4 CONTEXT.md D-27 and DSL settled rules.
