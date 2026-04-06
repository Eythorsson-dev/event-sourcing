---
title: Discuss removed_by key mapping before planning Phase 4
area: projection-macro
created: 2026-04-06
status: pending
---

# removed_by key mapping — open decisions

The `removed_by` syntax on list fields needs a design decision before Phase 4 can be planned. The syntax direction is settled (`removed_by: o.ItemRemoved.item_id | o.ItemArchived.item_id`), but several edge cases are unresolved.

## Open questions

### 1. Composite keys
If a list has composite keys `items[order_id, item_id]`, how does `removed_by` declare the mapping?

```
-- Option A: all key fields on one line
removed_by: o.ItemRemoved.order_id + o.ItemRemoved.item_id

-- Option B: separate line per key field
removed_by: o.ItemRemoved { order_id, item_id }

-- Option C: positional (order must match key declaration order)
removed_by: o.ItemRemoved.order_id | o.ItemRemoved.item_id
```

### 2. Events from joined streams
Can an event from a joined stream trigger removal?

```
-- Example: product discontinued removes all items linked to it
items[id] {
    removed_by: o.ItemRemoved.item_id
               | p.ProductDiscontinued.product_id  -- joined stream event
}
```

The joined stream's key path may not match the item key field — requires a resolution mapping.

### 3. Conditional removal
Should removal ever be conditional (e.g., "remove only if quantity drops to 0")? Or is removal always unconditional?

Current assumption: unconditional. Worth confirming.

### 4. Remove vs soft-delete
Should `removed_by` hard-delete (remove from list in state) or soft-delete (set a field to indicate removed, keep in list)? Current assumption: hard-delete. Soft-delete is application concern.

## Context

Raised during discuss-phase session 2026-04-06 (Phase 4 projection macro discussion).
Current pipe syntax: `removed_by: o.ItemRemoved.item_id | o.ItemArchived.item_id`
This must be resolved before Phase 4 planning to avoid changing the JSON schema mid-implementation.
