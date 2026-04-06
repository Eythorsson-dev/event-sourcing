# Phase 2: In-Memory Log Store - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-05
**Phase:** 02-in-memory-log-store
**Areas discussed:** dyn LogStore compatibility, Clone/sharing semantics, Non-existent stream reads

---

## dyn LogStore Compatibility

| Option | Description | Selected |
|--------|-------------|----------|
| Add async-trait to LogStore | Retrofit trait with #[async_trait]. dyn LogStore works immediately. Small Box<dyn Future> cost. | |
| Provide a type-erasure wrapper (BoxedLogStore) | Keep trait as-is, add wrapper type that boxes futures. More complex API surface. | |
| Relax success criteria — no dyn LogStore | Drop criterion #5. Callers use S: LogStore generics only. | ✓ |

**User's choice:** Relax success criteria — no dyn LogStore requirement
**Notes:** Phase 1 chose native `async fn in trait` which is incompatible with object safety. Keeping that decision and dropping the dyn requirement keeps the design simpler.

---

## Clone and Sharing Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Arc-backed interior, Clone shares state | InMemoryLogStore holds Arc<RwLock<State>> internally. clone() shares the same store. | ✓ |
| Value type, no Clone — callers wrap in Arc | InMemoryLogStore owns data directly. Callers use Arc::new(store). | |
| Clone creates independent snapshot | clone() deep-copies current state. Sharing requires explicit Arc. | |

**User's choice:** Arc-backed interior, clone() shares state
**Notes:** Tests can pass clones to multiple components and observe shared state without manually wrapping in Arc.

---

## Non-Existent Stream Reads

| Option | Description | Selected |
|--------|-------------|----------|
| Empty stream — no error | Returns Ok(empty). "Not found" == "empty" in append-only log. | ✓ |
| Error — stream not found | Returns Err(StoreError::StreamNotFound). Callers handle extra error variant. | |
| Option<Stream> | Returns Result<Option<Stream>>. Requires Phase 1 trait signature change. | |

**User's choice:** Empty stream (after discussion)
**Notes:** User initially leaned toward error, then considered Option<Stream> as more semantically honest. Concluded that in an append-only log "not found" and "empty" are truly identical observable states — there is no scenario where distinguishing them carries meaning. Empty stream is correct.

---

## Claude's Discretion

- Internal concurrency primitive choice
- Internal data structure (HashMap<StreamId, Vec<StoredEvent>> expected)
- Global sequence counter strategy
- Concrete EventStream associated type
- Test coverage scope

## Deferred Ideas

None.
