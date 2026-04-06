# Phase 3: Event Log and Optimistic Concurrency - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the discussion.

**Date:** 2026-04-06
**Phase:** 03-event-log-and-optimistic-concurrency
**Mode:** discuss
**Areas discussed:** EventLog error type, Read API at EventLog level, EventLog::append API surface

---

## Gray Areas Presented

| Area | Options Considered |
|------|--------------------|
| Error type | New `EventLogError` now vs. extend `AppendError` in Phase 6 |
| Read API | Mirror LogStore exactly vs. store accessor vs. higher-level helpers |
| Append API surface | Same `AppendCondition` signature vs. named helpers |

---

## Decisions Made

### Error type
- **Decision:** New `EventLogError` now. `AppendError` stays storage-only.
- **Rationale:** Constraints are an `EventLog`-level concern. Putting `ConstraintViolation` on `AppendError` would bleed a higher-level concern into the storage trait's error type. Clean layer separation.

### Read API
- **Decision:** `EventLog::read_stream` / `read_all` as thin wrappers, same signatures as `LogStore`.
- **Rationale:** Callers must always go through `EventLog` for reads. Phase 7 adds inline catch-up logic to these paths — if callers bypass `EventLog` to read from the store directly, they miss that. Establishing the pattern now.

### Append API surface
- **Decision:** Same `AppendCondition` signature as `LogStore::append`. No named helpers.
- **Rationale:** `AppendCondition::ExpectedVersion` is load-bearing — callers should see it explicitly. Named helpers would obscure the concurrency semantics.

---

## Note on interaction style

User prefers conversational chat over AskUserQuestion menus for discuss-phase sessions.

---

*Discussion completed: 2026-04-06*
