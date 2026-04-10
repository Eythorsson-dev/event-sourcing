---
title: Derived fields in projection engine
area: projection-engine
created: 2026-04-10
status: pending
---

# Derived fields in projection engine

Support computing new fields from existing projection state rather than only mapping raw event payload fields directly.

## Concept

A projection field declared as a derived expression would be recomputed each time the fold produces a new state, using the current values of sibling fields as inputs.

```
projection OrderView {
    query tag.starts_with("order:") as o

    items {
        key: $tags.item
        unit_price: o.ItemAdded.unit_price
        quantity:   o.ItemAdded.quantity |+ o.ItemUpdated.delta |? 0
        total:      $derived(unit_price * quantity)   // computed, not from event
    }

    subtotal: $derived(sum(items.total))
    tax:      $derived(subtotal * 0.1)
    grand_total: $derived(subtotal + tax)
}
```

## Use Cases

- **Line item totals**: `total = unit_price * quantity`
- **Order aggregates**: `subtotal = sum(items.total)`, `grand_total = subtotal + tax`
- **Display names**: `full_name = first_name + " " + last_name`
- **Status labels**: computed from multiple boolean flags
- **Percentages / ratios**: `completion_rate = completed / total`

## Design Questions

- Expression language: string template? Subset of Rust closures? Custom DSL node in `ProjectionDefinition`?
- How are derived fields represented in the JSON schema for `ProjectionDefinition`?
- Execution order: topological sort on field dependencies to evaluate in the right order
- Should derived fields be serialized into the stored read model, or recomputed at query time?
- Interaction with list aggregates (`sum`, `count`, `avg` over a list field) — these are a special case of derived fields

## Why Deferred

Event field mapping covers Phase 4 and Phase 5 use cases. Derived fields add expression evaluation complexity (parsing, type inference, error handling) that belongs in a later phase once the fold engine is stable. Phase 10 (GROUP BY and Window Functions) is adjacent — derived fields may be planned alongside or after it.

## References

Raised during Phase 04 discuss-phase session 2026-04-10.
