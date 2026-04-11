---
title: Discuss removed_by semantics for list item removal
area: projection-macro
created: 2026-04-06
status: reopened
resolved: ~
---

# removed_by key mapping — REOPENED

Previously resolved during Phase 4 discuss-phase session 2026-04-09, but reopened after `$tags` removal invalidated the original design.

## Why Reopened

The original `removed_by` design relied on tag-based key matching: the removal event had to carry the same tag prefix as the list's `key` declaration. With `$tags` dropped entirely (tags are for consistency boundaries only, projections use payload field paths), the removal mechanism needs rethinking.

## Open Questions

1. **Key matching**: Does the removal event need to carry the same key fields as the list's `key` declaration? What if the removal event uses a different field name for the same identity?
2. **Batch removals**: An event like `BatchRemovedOrderItems` may remove multiple list items at once. How does the engine know which items to remove?
3. **DSL syntax**: Where does removal declaration live? Inside the list block? In the `[]` brackets? As a separate clause?
4. **JSON schema**: What replaces `"remove_on": ["EventType"]`?

## Previous Decisions (may still apply)

- `removed_by` lives inside the list block (correct placement — it is a list-level control operation)
- Syntax was: `removed_by: EventType | OtherEvent` — event type names only

## Context

- Phase 4 CONTEXT.md OQ-DSL-01
- List keys now use bracket syntax: `items[item_id] { ... }`
- `$tags` removal rationale: no enforced tag format, batch events break scalar assumption, multi-role same-prefix ambiguity
