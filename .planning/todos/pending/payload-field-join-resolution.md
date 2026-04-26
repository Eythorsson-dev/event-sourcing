---
title: Payload-field join resolution (`on $.field`)
area: projection-engine
suggested_phase: post-5
created: 2026-04-26
---

# Payload-Field Join Resolution

## Problem

Phase 5 implements tag-based join resolution only (`on alias` / `on alias.EventType`). This requires events to carry tags for all entities they reference — the DCB model. For non-DCB events where the referenced entity ID lives in the event payload rather than the tags, there is no supported join resolution strategy.

## Proposed Change

Add an `on $.field` resolution strategy for joins:

```
// DSL
join tag.starts_with("product:") as p on $.product_id

// JSON
"p": {
  "query": { "StartsWith": "product:" },
  "on": "$.product_id"
}
```

Semantics: the join key is read from the named field in the current projection state. If the field is null (not yet projected), the join contributes no events (left join semantics). When the field is populated (by a prior event in the fold), the engine uses its value as the joined-stream instance key.

## Context

Decided during Phase 5 discuss-phase (2026-04-26). Phase 5 defers this to keep scope tight. The `on` keyword is reused — `on "$.path"` is a string starting with `$` (payload field path), distinguished from `on "alias"` or `on ["alias.Event"]` (tag-based resolution).

## Scope

Requires:
- New `JoinOn::PayloadField(String)` variant in the engine
- Serde disambiguation: `"on": "$.product_id"` → PayloadField, `"on": "c"` → AnyFrom, `"on": ["c.E"]` → Events
- Engine fold: read the field path from current state at join resolution time
- Registration-time validation: warn if the referenced field isn't declared in the projection
