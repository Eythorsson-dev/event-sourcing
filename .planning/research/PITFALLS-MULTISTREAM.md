# Pitfalls: Multi-Stream Projection Joins

**Domain:** Rust event sourcing library — multi-stream projection engine
**Researched:** 2026-04-04
**Confidence:** HIGH (most findings confirmed by multiple sources or official framework documentation)

---

## Critical Pitfalls

### Pitfall 1: `fromStreams` Does Not Guarantee Global Order

**What goes wrong:** When merging events from two separate stream filters, the merge order is not automatically the global append order. EventStoreDB's `fromStreams()` API is documented as "best-attempt ordering" — events from different source streams can arrive interleaved inconsistently, even on a single node. Replaying the same event set twice may produce different fold results if the merge comparator is not strictly tied to a monotonic global key.

**Why it happens:** Each stream filter produces an independently ordered sub-sequence. Merging two ordered sequences requires a tie-breaking key that reflects true append order across all streams. Wall-clock timestamps are not sufficient because two events can share the same millisecond and their relative order is not defined by time. Per-stream version numbers are not comparable across streams.

**Consequences:** The projection fold is non-deterministic across replays. A projection that builds a state machine (e.g., order + payment) can end up in different terminal states depending on which stream's event is merged first at a position where both have events with equal or ambiguous keys.

**Prevention:** Merge by a single global, monotonically increasing `SequenceId` assigned at append time. This is the correct tie-breaker. The `ProjectionEngine` must read events from both filters, assign each a `SequenceId` in the `StoredEvent`, and merge by that field — not by timestamp or per-stream version.

**Detection:** Write a test that appends to stream A and stream B concurrently and verifies projection output is identical across two separate replays from scratch. Any non-identity is the bug.

**Relevant evidence:** EventStoreDB issue #814 (confirmed `fromStreams` ordering is not guaranteed; workaround is `fromAll` which uses global position). Marten documentation warns that multi-stream projections under high load produce "apparent event skipping and invalid results" without async daemon locking.

---

### Pitfall 2: Partial Join — One Stream Has Events, the Other Does Not Yet

**What goes wrong:** A join projection is queried for entity X. Stream A has a `Created` event for X. Stream B (e.g., the payment stream for X) has no events yet. The projection engine folds stream A's events and produces a partial output. If the projection's output shape assumes both sides are present, it either returns an incomplete struct or the fold panics on a missing field.

**Why it happens:** Writes to stream A and stream B are independent appends — there is no cross-stream atomic write in standard event sourcing. Between the time stream A is written and stream B is written, any query hits a partial join.

**Consequences:** Read models show entities in impossible states (order exists, no associated payment record). Constraint checks against the join output can falsely pass or fail. If the projection output is used as a constraint (e.g., "payment not yet received"), the check may be evaluated against an incomplete view that does not yet include a payment that was just written.

**Prevention:**
- Design projection fold functions to treat missing join partners as valid empty state, not an error. Use `Option<T>` for fields sourced from the secondary stream.
- Never assert that both join sides must be non-empty at fold time. The fold must be total over any subset of events from either stream.
- For constraint validation specifically: a constraint that needs both streams must tolerate the window where only one stream has data.

**Detection:** Test: write to stream A, immediately query the join projection, verify result is well-formed (not a panic, not a partial-struct corruption). Then write to stream B, re-query, verify result incorporates both.

---

### Pitfall 3: Concurrent Writes Across Joined Streams — Projection Sees Partial State

**What goes wrong:** Two concurrent writes happen: writer 1 appends to stream A (order updated), writer 2 appends to stream B (payment confirmed). A third reader queries the join projection between these two writes. The projection sees stream A's new event but not stream B's new event (or vice versa). The resulting read model is internally inconsistent even though both writes succeeded.

**Why it happens:** There is no cross-stream atomic boundary. Optimistic concurrency only prevents conflicting writes within a single stream. A read that spans two streams has no snapshot isolation guarantee unless the storage layer provides it (e.g., a repeatable read transaction). Most event log implementations do not offer cross-stream read snapshots.

**Consequences:** A constraint check using a multi-stream join may read stream A at position 10 and stream B at position 7, producing a state that never existed as a committed whole. If the constraint allows an action based on this phantom state, an invariant is violated.

**Prevention:**
- For constraint validation use cases, constrain the join to streams that are causally related — events in both streams must reference the same correlation ID or be written within the same logical operation. This does not eliminate the window but reduces its practical impact.
- The inline catch-up path (read projection at min `seq_id`) partially mitigates this: if the caller passes the `SequenceId` returned from the most recent append, the engine will catch up to at least that position before returning. This ensures the reader sees its own write. It does not guarantee it sees concurrent writes from other writers.
- Document clearly: the multi-stream join is not serializable. It provides causal consistency for a single writer's writes, not cross-writer snapshot isolation.

**Detection:** Concurrency test: two writers append to their respective streams with 1ms separation; a reader queries the join projection at various points and checks for states that could not have been produced by any sequential history.

---

### Pitfall 4: Replay Non-Determinism When Join Key Appears in Both Streams

**What goes wrong:** Both stream A and stream B contain events that carry the join key (e.g., `order_id`). If the projection fold function has any state-dependent branching on which stream's event arrives first, the fold is order-sensitive. On the first replay, stream A's events come first (because the global sequence happened to be A=1, B=2). On a second replay from a backup where the sequence was partially reordered (or from a different storage implementation), B arrives before A. The terminal projection state differs.

**Why it happens:** Projection fold functions are written assuming implicit ordering. When the fold processes a "PaymentReceived" before "OrderCreated," intermediate state may be inconsistent (e.g., a payment is recorded against a non-existent order). Whether the final state is correct depends on whether the fold is commutative for these two events — and most folds are not.

**Consequences:** A projection rebuilt from the same underlying events produces a different read model depending on the storage backend or replay batch ordering. Constraints that passed during production fail during a cold replay.

**Prevention:**
- Fold functions must be written to be correct regardless of which stream's events arrive first within a global-sequence-sorted merge. Specifically: always allocate the parent struct before processing child events. Use `entry().or_insert_with()` style patterns to lazily initialize the projection target.
- For this library's design: the `ProjectionEngine` must sort the merged event stream by global `SequenceId` before folding, not by per-stream position. This is the single source of truth for order.
- Write replay correctness tests using both insertion orderings (A before B, B before A) and assert identical output.

---

## Moderate Pitfalls

### Pitfall 5: Last-Write-Wins on Concurrent Async Projection Updates

**What goes wrong:** Two events are processed concurrently by an async projection observer. Both load the current projection document at state S. One saves S + event_1. The other saves S + event_2, overwriting event_1's update. The final projection document is missing event_1's contribution.

**Why it happens:** Async (observer-based) projection materialization is not atomic with respect to the projection document. Without optimistic concurrency on the document write, two concurrent updates produce a last-write-wins outcome.

**Consequences:** The materialized read model silently drops events. Counters under-count. Nested lists are missing items. No error is raised.

**Prevention:**
- The `ProjectionObserver` must use optimistic concurrency on writes to the materialized document (check-and-set version). On conflict, reload the document and re-apply.
- Alternatively: ensure projection document updates are always single-writer (serialize updates per projection key).
- Marten resolved this by defaulting multi-stream projections to async-daemon-exclusive processing. The same principle applies here: the `ObserverRegistry` should process events for a given projection key sequentially, not in parallel.

**Evidence:** Marten issue #2606 is an exact reproduction of this bug: two concurrent commands both loaded the same projection document at state [0,1,2], one saved [0,1,2,3], the other saved [0,1,2,4], permanently losing item 3.

---

### Pitfall 6: Nested List Items Orphaned When Parent Does Not Exist Yet

**What goes wrong:** A projection builds a nested structure: `Order { id, items: Vec<LineItem> }` where order events come from stream A and line-item events come from stream B. A `LineItemAdded` event arrives (from stream B) before the `OrderCreated` event (from stream A) because the global sequence placed them in that order.

**Why it happens:** In a system with high concurrency or batch imports, stream B events can be written before stream A events even though logically B depends on A. The projection engine processes them in global sequence order, so a `LineItemAdded` for order_id=42 arrives before `OrderCreated` for order_id=42.

**Consequences:** The fold function finds no parent struct to attach the line item to. Options: silently drop the item (permanent data loss in the read model), panic (crashes the projection), or buffer the item for later (complex and state-heavy).

**Prevention:**
- Fold functions must create the parent struct lazily when any event referencing that key arrives, regardless of event type. The parent may be partially populated (no `OrderCreated` fields yet). Set a flag or use `Option` for fields that come from `OrderCreated`.
- When `OrderCreated` is finally processed, fill in the remaining fields. The nested items are already present.
- This "lazy initialization" pattern must be a first-class design requirement in the `ProjectionEngine`'s fold contract.

**Detection:** Test: write `LineItemAdded` for order 42 before `OrderCreated` for order 42. Verify the final projection contains all data correctly.

---

### Pitfall 7: Constraint Checks on Multi-Stream Joins Are TOCTOU-Vulnerable

**What goes wrong:** The constraint check pattern is: (1) read events via `ProjectionEngine`, (2) run `check(output)`, (3) atomically append if check passes. Steps 1 and 3 are not atomic across multiple streams. Between step 1 and step 3, a concurrent writer may append to one of the joined streams, changing the state the constraint was evaluated against.

**Why it happens:** Optimistic concurrency at the `LogStore` level only covers the stream being appended to. A constraint that reads from stream B while appending to stream A has no lock on stream B during the window.

**Consequences:** A constraint like "total items across all orders for customer C must not exceed 100" can be violated if two concurrent appends both read the current total (95), both conclude "5 more is fine," and both succeed, ending at 105.

**Prevention:**
- This is fundamentally the same TOCTOU problem as with aggregate-based optimistic concurrency, but harder to solve across streams.
- The most robust mitigation in a DCB design: include the `SequenceId` of all joined streams in the `AppendCondition`. If any of those streams advanced since the projection was read, the append must be retried.
- Callers using multi-stream constraints must be prepared to retry on conflict.
- Document explicitly: multi-stream constraints provide consistency only if all joined streams are covered by the `AppendCondition` version check.

---

### Pitfall 8: Grouping Logic That Requires Loading the Projection Document Itself

**What goes wrong:** A multi-stream projection's "which group does this event belong to?" logic depends on reading the current projection output (e.g., "look up which customer owns this order to route a payment event"). This creates a circular dependency: grouping requires the projection state, but the projection state is being built.

**Why it happens:** Grouping and folding are parallel operations in optimized projection engines. If grouping depends on fold output, the two phases must become sequential, breaking the parallelism assumption.

**Consequences:** The grouping lookup returns stale or absent data. Events are routed to the wrong projection instance. The read model silently includes events that belong elsewhere.

**Prevention:**
- Grouping/routing must be based only on data in the event payload itself, never on current projection state.
- If routing requires a lookup (e.g., "find the customer for this payment"), denormalize the lookup key into the event at write time (include `customer_id` in the `PaymentReceived` event).
- Marten explicitly documents this as a known limitation: "ViewProjection will not function correctly because of operation ordering (grouping happens in parallel to building projection views)."

---

## Minor Pitfalls

### Pitfall 9: Unbounded Join Scope Causes Full-Log Scans

**What goes wrong:** A stream filter like `StreamFilter("orders.*")` matches every order in the system. For a join projection that needs to produce a single document, this reads the entire order sub-log on every evaluation. At scale this is a performance cliff, not a bug.

**Prevention:** Constrain filters by correlation ID or a bounded tag. If a join projection is meant to aggregate a single entity (e.g., order 42 + its payment), the filter must include the entity ID. Global aggregations must be bounded to a time window or partition.

---

### Pitfall 10: At-Least-Once Observer Delivery Duplicates Events Into Projection

**What goes wrong:** The `ObserverResult::Retry` path causes the observer to re-process events. If the `ProjectionObserver`'s fold function is not idempotent, a retried event is applied twice, corrupting counts and nested structures.

**Why it happens:** The retry loop is correct for handling transient failures. But if the projection storage write succeeded and the observer returned `Retry` anyway (e.g., due to a subsequent network error), the same events will be re-applied on the next invocation.

**Prevention:**
- Fold functions must be idempotent with respect to event IDs: before applying an event, check whether the event's `SequenceId` has already been incorporated into the stored projection watermark.
- The `ProjectionObserver` must track the highest `SequenceId` it has applied and skip events at or below that watermark on retry.

---

## Phase-Specific Warnings for This Library

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| `ProjectionEngine` multi-stream merge | Global sequence ordering not enforced in merge | Merge by `SequenceId` strictly, never by timestamp or per-stream version |
| `ProjectionEngine` fold design | Partial join / lazy init not handled | Require fold functions to accept any event from any joined stream first |
| `ConstraintSet` with multi-stream joins | TOCTOU between constraint read and append | `AppendCondition` must cover all joined stream versions |
| `ProjectionObserver` retry | At-least-once delivery duplicates events | Track `SequenceId` watermark; skip already-applied events |
| Inline catch-up on reads | Catches up caller's own writes, not concurrent writers | Document the guarantee precisely; do not over-promise serializability |
| Nested list projections | Orphaned child events before parent exists | Lazy-init parent on first event from any stream for that key |
| Async observer dispatch | Last-write-wins on concurrent document updates | Serialize updates per projection key in `ObserverRegistry` |

---

## Sources

- EventStoreDB issue #814 — `fromStreams` ordering is best-attempt, not guaranteed: https://github.com/EventStore/EventStore/issues/814 (HIGH — confirmed by EventStoreDB maintainer)
- Marten issue #2606 — async projection loses events in concurrent scenario: https://github.com/JasperFx/marten/issues/2606 (HIGH — confirmed root cause and resolution)
- Marten multi-stream projection docs — async default due to contention: https://martendb.io/events/projections/multi-stream-projections (MEDIUM — 403 on direct fetch, confirmed via search excerpts)
- Marten issue #3052 — Reboot Projection API Model (slicing API complexity): https://github.com/JasperFx/marten/issues/3052 (HIGH — inspected directly)
- Marten grouping limitation — grouping cannot depend on projection state: https://martendb.io/events/projections/multi-stream-projections (MEDIUM)
- Axon Server 2025.1 DCB announcement — no snapshot support, experimental status: https://www.axoniq.io/blog/axon-server-future-proof-event-store (MEDIUM)
- SoftwareMill — Things I wish I knew (part 2, consistency): https://softwaremill.com/things-i-wish-i-knew-when-i-started-with-event-sourcing-part-2-consistency/ (MEDIUM — confirmed via search excerpt: "changes propagated to one didn't reach the other yet")
- Kurrent — Counterexamples regarding consistency: https://www.kurrent.io/blog/counterexamples-regarding-consistency-in-event-sourced-solutions-part-1/ (MEDIUM — 403 on direct fetch, confirmed via search excerpt)
- Domain Centric — Deduplication strategies: https://domaincentric.net/blog/event-sourcing-projection-patterns-deduplication-strategies (MEDIUM)
- Event-Driven.io — Nested object structure projections: https://event-driven.io/en/how_to_create_projections_of_events_for_nested_object_structures/ (MEDIUM — 403 on direct fetch, confirmed via search)
