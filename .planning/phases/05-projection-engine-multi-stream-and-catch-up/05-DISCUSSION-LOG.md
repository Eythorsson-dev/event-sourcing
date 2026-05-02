# Phase 5: Projection Engine — Multi-Stream and Catch-Up - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-26
**Phase:** 05-projection-engine-multi-stream-and-catch-up
**Session:** Update session — join design was already settled; this session resolved multi-stream fetch API, engine architecture, and catch-up scope.
**Areas discussed:** Catch-Up Reads (scope), Engine API shape, Multi-Stream Fetch, Collection Projections, Cycle Detection Tests

---

## Catch-Up Reads

| Option | Description | Selected |
|--------|-------------|----------|
| Implement in Phase 5 | Inline catch-up: engine gates on a target GlobalSequenceId, fetches missing events | |
| Defer to separate phase | Extract as its own phase; not needed for MVP | ✓ |

**User's choice:** Defer. "Catch-up can be extracted as a separate phase. This is more of noise to help than a strict requirement for an MVP."

---

## Engine API Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Pure iterator (caller fetches everything) | Engine takes pre-fetched event iterator, no IO | Partial |
| LogStore closure injection | Engine takes `impl Fn(TagFilter) -> Events` closure | |
| LogStore trait injection into project fn | `&dyn LogStore` passed to `project`/`project_all` | ✓ |
| ProjectionEngine unit struct | Wrapper struct with associated functions | |
| Free functions | Module-level functions, no struct wrapper | ✓ |

**User's choice:** `&dyn LogStore` injected into orchestration functions (`project`, `project_all`). Pure `fold` core stays IO-free. `ProjectionEngine` unit struct removed in favour of free functions.

**Notes:** User questioned why `ProjectionEngine` exists as a struct when it has no state ("Could we just use a normal function instead?"). Agreed — it's just a namespace; a module serves that purpose. The struct adds no value.

---

## Multi-Stream Fetch Coordination

| Option | Description | Selected |
|--------|-------------|----------|
| Caller orchestrates | Caller does two-phase: first fold to discover keys, then fetches joined streams | |
| Engine owns multi-pass loop | Engine drives the fetch-fold loop with LogStore; fold core stays pure | ✓ |
| Eager within-pass fetch | Engine fetches joined stream events mid-fold when on-event encountered | |

**User's choice:** Engine owns the multi-pass orchestration loop. Pure `fold(def, events, join_key_map)` returns updated state + join key map. Orchestration calls fold, inspects discovered keys, fetches joined streams, repeats until stable.

**Notes:** User said "I don't think it's the caller's responsibility to perform the joining. I think the engine needs to be able to query the events when it sees that it's missing some of the events." Also: "It should only fetch the events it actually cares about in the stream" — clarified as: fetch all events for the resolved stream instance (selective by instance, not by event type). The fold discards irrelevant event types silently (existing D-22).

---

## Collection Projections ("All Customers")

| Option | Description | Selected |
|--------|-------------|----------|
| Phase 5.1 list fields only | Collection projections require a list-field definition | |
| Caller iterates per stream | Caller lists stream instances, calls project per instance | |
| Engine-side grouping via project_all | Engine groups events by stream instance, calls project per group | ✓ |

**User's choice:** `project_all` — engine handles grouping. Query `StartsWith("customer:")` → engine produces one `CustomerView` per distinct stream instance.

**Notes:** User's mental model: "I thought that we would select all tags that start with the prefix defined, and then it just iterates over each of them and creates one read model per tag." That model is correct and is what `project_all` implements. This is distinct from Phase 5.1 list fields (list fields within a single entity's projection, not one-model-per-stream). No code duplication: `project_all` calls `project` per instance; both share the `fold` core.

---

## Query Engine vs Projection Engine Naming

| Option | Description | Selected |
|--------|-------------|----------|
| "Projection engine" for Phase 5 | Everything is a projection engine | |
| Phase 5 = query engine; Phase 7 = projection engine | Distinction based on persistence | ✓ |

**User's choice:** Phase 5 builds an on-demand query engine (compute and return, no persistence). The projection engine (persistent, incrementally maintained) arrives in Phase 7.

**Notes:** User initiated: "I feel like this becomes a projection engine once persistence of the result is added? What is the difference between a projection engine and a query engine?"

---

## Cycle Detection Tests

| Option | Description | Selected |
|--------|-------------|----------|
| Single test — detection fires | Assert CircularJoinDependency error is returned | |
| Two tests — fires + demonstrates necessity | Also show oscillation across N passes without detection | ✓ |

**User's choice:** Two tests required. (a) Prove detection fires. (b) Prove it's needed — instrument pass counter to show unbounded oscillation without the check.

**Notes:** "I want to see that it (a) works, and (b) is needed."

---

## Cross-Type Joins

**Topic raised by user:** "There is a valid case for the iterator containing multiple of the primary stream. When querying customer:abc, there might be a downstream join that joins in customer:def. We need to handle and allow this."

**Decision:** Valid and supported. The fold attributes events by full tag value (not prefix). `customer:abc` events → primary stream → bare handlers. `customer:def` events (resolved as join alias) → `alias.EventType` handlers. Both can share the same tag prefix; the specific tag value is the discriminator.

---

## Claude's Discretion

- Internal representation of `JoinKeyMap` (e.g., `IndexMap<String, String>`)
- Serde representation of `JoinOn` (string vs array)
- Whether root-level and object-level joins share one code path
- Error variant structure for `ProjectionError::CircularJoinDependency`
- Registration-time static cycle detection on the join dependency graph
- Exact shape of `MultiCheckpoint`

## Deferred Ideas

- Inline catch-up reads (LOG-08) — deferred to a separate phase
- `on $.field` payload-field join resolution — future phase
- `query` → `from` keyword rename — Phase 5.2
- Temporal joins — post-Phase 5
- Transformation functions — post-Phase 5
