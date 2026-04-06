---
title: Derive-style projection definition (attribute macro on struct)
area: projection-macro
created: 2026-04-06
status: pending
---

# Derive-style projection definition

Alternative to the `projection!{}` query language: a `#[derive(Projection)]` attribute macro where the struct lives outside the macro.

## Concept

```rust
#[derive(Projection, Debug, Serialize)]
#[projection(stream = "order")]
struct OrderView {
    #[field(default = "draft")]
    status: String,

    #[field(default = 0)]
    total: i64,

    items: Vec<OrderItem>,
}

#[projection_handlers(OrderView)]
impl OrderViewHandlers {
    fn on_order_placed(state: &mut OrderView, event: OrderPlaced) {
        state.status = event.status.clone();
        state.total  = event.total_cents;
    }

    fn on_item_added(state: &mut OrderView, event: ItemAdded) {
        state.total += event.price_cents;
        state.items.push(OrderItem { id: event.item_id.clone(), name: event.name.clone(), quantity: event.quantity });
    }
}
```

## Appeal

- Struct is a normal Rust struct — composes freely with `#[derive(Debug, Serialize)]`
- Handlers are plain Rust — no DSL restrictions, full language available
- Familiar to anyone who has used Bevy, Axum, or similar frameworks

## Trade-off

Raw Rust closures cannot generate a serializable `ProjectionDefinition` automatically. The engine is data-driven from persisted schemas (D-21) — this approach either: (a) requires a separate JSON schema definition alongside the Rust handlers, or (b) abandons runtime serializability for this usage mode.

## Decision

Not suitable as the primary API. Consider as a secondary "advanced" interface once the core query language is stable. Document the serializability trade-off clearly.

## References

Raised during discuss-phase session 2026-04-06 (phase 4). Explored as Proposal B, rejected in favour of a declarative query language.
