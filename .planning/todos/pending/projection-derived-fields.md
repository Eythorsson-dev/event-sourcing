---
title: Polymorphic types in projection read model
area: projection-engine
created: 2026-04-10
status: pending
---

# Polymorphic types in projection read model

Support discriminated union / polymorphic output shapes in a projection, where a list or field can hold items of different types depending on a discriminator — rather than forcing a single flat shape across all items.

## Concept

A projection list today produces items that all share the same field schema. Polymorphism would allow items in the same list to take different shapes based on a type discriminator, modelled as a Rust enum at the output type level.

```
projection ActivityFeed {
    query tag.starts_with("order:") as o

    events {
        key: $.event_id
        type: $discriminator          // drives which variant is selected

        OrderPlaced {
            customer_id: o.OrderPlaced.customer_id
            placed_at:   o.OrderPlaced.timestamp
        }

        ItemAdded {
            item_id:   o.ItemAdded.item_id
            name:      o.ItemAdded.name
            quantity:  o.ItemAdded.quantity
        }

        ItemRemoved {
            item_id:    o.ItemRemoved.item_id
            removed_at: o.ItemRemoved.timestamp
        }
    }
}
```

The generated Rust type would be an enum:
```rust
enum ActivityFeedEvent {
    OrderPlaced { customer_id: String, placed_at: DateTime },
    ItemAdded { item_id: String, name: String, quantity: i32 },
    ItemRemoved { item_id: String, removed_at: DateTime },
}
```

## Use Cases

- **Activity / audit feeds**: heterogeneous event log projected as a typed feed
- **Notification queues**: different notification shapes in one list
- **Timeline projections**: ordered sequence of varied event types with per-type payloads
- **Polymorphic domain objects**: e.g. a `Shipment` that can be `Parcel | Pallet | Container` with different fields

## Design Questions

- How is the discriminator declared in the `ProjectionDefinition` JSON schema?
- How does the DSL macro surface the variant selection?
- Serde representation: `#[serde(tag = "type")]` internally, or caller-controlled?
- Interaction with list `key` — each variant may contribute a different key field
- How does `removed_by` work when items may be of different types?

## Why Deferred

Single-shape list items cover Phase 4 and Phase 5 use cases. Polymorphic items require discriminator resolution logic in the fold engine and a more complex `ProjectionDefinition` schema. Natural to tackle after the core engine is stable.

## References

Raised during Phase 04 discuss-phase session 2026-04-10.
