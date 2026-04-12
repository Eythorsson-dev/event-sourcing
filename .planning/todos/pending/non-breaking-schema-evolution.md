---
title: Non-breaking schema evolution and schema migration
area: event-schema
created: 2026-04-12
status: pending
---

# Non-breaking schema evolution and schema migration

Phase 4 locks `EventSchemaDef` conflict detection to **strict equality** (D-07): any difference between the persisted schema and the compiled `Event::schema()` — including adding a new field — is a conflict that blocks append and fails startup validation.

This is the safe v1 default but it makes every schema change painful. Real event-sourced systems need a controlled path to evolve event payloads without rewriting history.

## Goals for a future phase

1. **Additive changes without conflict** — adding a new field to an event type should be allowed. Old `StoredEvent` payloads that lack the field should either (a) deserialize with the field as `Option::None` / `default`, or (b) be flagged only if a `ProjectionDefinition` actually references the new field.
2. **Field renames via aliases** — a `#[event_field(alias = "old_name")]` (or JSON `aliases: [...]`) annotation so a rename is not a remove + add.
3. **Explicit schema migrations** — a way to declare "payload version 2 reads differently from version 1," with a transform function applied at projection time so historical events remain usable.
4. **Versioned `EventSchemaDef`** — persisted schemas may carry a version or hash; multiple versions coexist for the same `event_type` and the engine picks the correct one per stored event.
5. **Conflict granularity** — downgrade "schema drift" from a hard failure to a typed warning when no registered projection reads the changed field (composes with the existing `projection-aware-schema-conflict.md` todo).

## Design questions to resolve before planning

- Is this one phase or several? Aliases + additive fields is a small change; full versioned migration with transform functions is a large one.
- Where does the transform function live — as user-registered Rust code, as JSON-declared mappings, or both?
- How does the `EventSchemaStore` represent multiple versions of the same `event_type` (one row per version? version column?)?
- Interaction with `ProjectionEngine` — does the engine apply migrations lazily at projection time, or does a separate pass upgrade stored schemas?
- Interaction with the `macros` feature — can the derive macro emit alias metadata and version numbers automatically?

## Tradeoffs

- Strict equality (current) = simple, safe, painful. Every schema change needs a conscious decision.
- Additive-only evolution = much less friction, still safe (old payloads never lie about having fields they don't have).
- Full versioned migration = most flexible, most complexity, most test surface.

## Related

- `.planning/todos/pending/projection-aware-schema-conflict.md` — complementary (projection-awareness reduces false positives; this todo reduces true positives that aren't actually breaking).
- Phase 4 `D-07` — the decision being deferred.
- Phase 4 `D-08` — union-of-DB-and-code validator already handles deleted-type survival; extending to multi-version persisted schemas is the natural next step.

## References

- Confluent Schema Registry compatibility modes (BACKWARD, FORWARD, FULL) — reference design for what "non-breaking" can mean.
- Marten's event upcasting pattern.
- EventStore's "event upcaster" hooks.
