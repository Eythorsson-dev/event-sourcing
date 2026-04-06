---
title: Projection-aware schema conflict detection
area: event-schema
created: 2026-04-06
status: pending
---

# Projection-aware schema conflict detection

Currently using simple semantic checking: a schema change is a conflict if a field is removed or its FieldType changes, regardless of which projections reference that field.

## Opportunity

Tighten to projection-aware mode: a schema change is only a conflict if a registered `ProjectionDefinition` references the changed field. This is more precise — it tells the developer which projection broke and which field caused it, rather than flagging any structural change.

## Requirements

- Validator reads all persisted `ProjectionDefinition`s during schema conflict check
- Cross-references changed/removed fields against projection `events` and `join` field mappings
- Reports conflict with: event type, field name, affected projection(s)

## Tradeoffs

- More precise: no false positives for fields nothing cares about
- Requires reading projection definitions during schema validation (cross-store dependency)
- Adds complexity — defer until both `EventSchemaStore` and projection registration are stable

## References

Research confirmed this is the approach closest to what Marten and EventStore provide. Confluent Schema Registry analogue: FULL compatibility mode (both reader and writer compatible).
