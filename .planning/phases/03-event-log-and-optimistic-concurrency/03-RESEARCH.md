# Phase 3: Event Log and Optimistic Concurrency - Research

**Researched:** 2026-04-06
**Domain:** Rust thin wrapper / orchestration layer over an existing trait
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** `EventLog::append` returns `EventLogError`, a new enum distinct from `AppendError`. Phase 3 variants: `ConcurrencyConflict { stream_id, expected, actual }` and `StorageFailure(..)`. Phase 6 adds `ConstraintViolation` to `EventLogError` — not to `AppendError`.
- **D-02:** `AppendError` stays storage-layer-only (on `LogStore::append`). `EventLogError` wraps the storage error variants internally. Clean layer separation: storage errors vs. event-log-level errors.
- **D-03:** `EventLog` exposes `read_stream` and `read_all` as thin wrappers with the same signatures as `LogStore::read_stream` / `read_all`. Callers always read via `EventLog`, not the underlying store directly.
- **D-04:** This sets the pattern for Phase 7 (observers with read-your-writes consistency / inline catch-up), where reads through `EventLog` will gain catch-up logic. Callers going through the store directly would bypass that.
- **D-05:** `EventLog::append` signature mirrors `LogStore::append`: `(stream_id: &StreamId, events: Vec<NewEvent>, condition: AppendCondition) -> Result<GlobalSequenceId, EventLogError>`. No named helpers (`append_any`, `append_with_version`) — `AppendCondition` is explicit and self-documenting.
- **D-06:** Concurrency enforcement stays at the storage layer. `EventLog::append` delegates directly — no secondary version check at the `EventLog` level.
- **D-07:** `EventLog<S: LogStore>` — generic over the store, owns it. Static dispatch. No `Arc` at the `EventLog` level; sharing is handled by the store's own `Clone` (e.g., `InMemoryLogStore` is `Arc`-backed).
- **D-08:** `EventLog` lives in the `event-sourcing` core crate alongside `LogStore`.

### Claude's Discretion

- Module layout within the core crate for `EventLog` and `EventLogError`
- Whether `EventLog` derives `Clone` (it would naturally if `S: Clone`)
- Test structure and coverage scope

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| LOG-04 | Append rejects with concurrency conflict error if stream has advanced past expected version | Delegated to storage layer; `EventLog` converts `AppendError::ConcurrencyConflict` to `EventLogError::ConcurrencyConflict` |
| LOG-05 | Read events by stream ID — full stream or range by sequence number | `EventLog::read_stream` thin-wraps `LogStore::read_stream`; same `from`/`to` signature passes through |
| LOG-06 | Successful append returns the new sequence ID for use in consistent reads | `LogStore::append` already returns `GlobalSequenceId`; `EventLog::append` propagates it on `Ok` |
</phase_requirements>

---

## Summary

Phase 3 is structurally simple: `EventLog<S>` is a thin orchestration wrapper over `LogStore`. The hard work — atomic concurrency enforcement, sequence ID allocation, stream versioning — is fully implemented in `InMemoryLogStore` from Phase 2. Phase 3 adds a stable user-facing surface (`EventLog`) that callers interact with, deliberately decoupling them from the storage layer so Phase 6 and Phase 7 can enrich the `EventLog` layer without altering caller code.

The central task is defining `EventLogError` (a new, independent error type at the `EventLog` layer), implementing `EventLog<S: LogStore>` as a struct that owns `S`, and wiring `append`, `read_stream`, and `read_all` as pass-through delegates with appropriate error mapping. There is no novel algorithm here — the pattern is error mapping + thin forwarding. The only design judgment required is module layout and `Clone` derivation, both delegated to Claude's discretion.

Test coverage for Phase 3 focuses on the `EventLog` API contract: that `ConcurrencyConflict` surfaces correctly at the `EventLog` level, that `read_stream` and `read_all` pass through faithfully, and that the returned `GlobalSequenceId` is correct. Tests use `InMemoryLogStore` as the backing store — no mocking needed.

**Primary recommendation:** Add `event_log.rs` to the `event-sourcing` core crate containing `EventLog<S>` and `EventLogError`, with re-exports from `lib.rs`. `EventLog` derives `Clone` automatically via `#[derive(Clone)]` with a `where S: Clone` bound.

---

## Standard Stack

### Core (already in Cargo.toml — no new dependencies needed)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| thiserror | 2.0 | `#[derive(Error)]` for `EventLogError` | Already used for `AppendError` and `StoreError`; same pattern |
| futures-core | 0.3 | `Stream` trait for `EventStream` associated type | Already in core crate; read methods pass through `S::EventStream` unchanged |

Phase 3 requires **zero new dependencies**. All types and traits are already present in the workspace.

**No installation required.**

---

## Architecture Patterns

### Where `EventLog` Lives

`EventLog<S>` belongs in the `event-sourcing` core crate (D-08). The natural location is a new file `event-sourcing/src/event_log.rs`, declared as `pub mod event_log;` in `lib.rs` and re-exported at the crate root.

```
event-sourcing/src/
├── error.rs         # AppendError, StoreError, AppendCondition (existing)
│                    #   ADD: EventLogError here, OR in event_log.rs (discretionary)
├── event.rs         # StoredEvent (existing)
├── event_log.rs     # NEW: EventLog<S>, EventLogError
├── lib.rs           # ADD pub use event_log::{EventLog, EventLogError}
├── store.rs         # LogStore trait (existing)
└── types.rs         # StreamId, sequence IDs, NewEvent (existing)
```

The choice of where `EventLogError` lives (in `error.rs` alongside `AppendError` / `StoreError`, or co-located in `event_log.rs`) is delegated to Claude's discretion. Both are correct. Locating it in `event_log.rs` keeps the module self-contained and makes Phase 6 additions (`ConstraintViolation` variant) easier to locate.

### Pattern 1: Thin Wrapper with Error Mapping

`EventLog::append` calls `self.store.append(...)` and maps the `AppendError` variants to the corresponding `EventLogError` variants.

```rust
// Source: derived from existing AppendError pattern in event-sourcing/src/error.rs
pub async fn append(
    &self,
    stream_id: &StreamId,
    events: Vec<NewEvent>,
    condition: AppendCondition,
) -> Result<GlobalSequenceId, EventLogError> {
    self.store
        .append(stream_id, events, condition)
        .await
        .map_err(|e| match e {
            AppendError::ConcurrencyConflict { stream_id, expected, actual } => {
                EventLogError::ConcurrencyConflict { stream_id, expected, actual }
            }
            AppendError::StorageFailure(source) => {
                EventLogError::StorageFailure(source)
            }
        })
}
```

### Pattern 2: Read Pass-Through

`read_stream` and `read_all` forward directly to the store. The return type is `Result<S::EventStream, StoreError>` — the `StoreError` type is **not** wrapped in `EventLogError` here because Phase 7 will add logic around reads, and wrapping now would require unwrapping later.

```rust
// Source: derived from LogStore trait in event-sourcing/src/store.rs
pub async fn read_stream(
    &self,
    stream_id: &StreamId,
    from: StreamSequenceId,
    to: Option<StreamSequenceId>,
) -> Result<S::EventStream, StoreError> {
    self.store.read_stream(stream_id, from, to).await
}

pub async fn read_all(
    &self,
    from: GlobalSequenceId,
) -> Result<S::EventStream, StoreError> {
    self.store.read_all(from).await
}
```

**Note:** Read methods use `StoreError` directly (not `EventLogError`). This is consistent with D-03 ("thin wrappers with the same signatures") and preserves Phase 7 extensibility without imposing an error wrapping layer that would complicate that phase.

### Pattern 3: EventLogError Definition

```rust
// Source: mirrors AppendError pattern in event-sourcing/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error(
        "concurrency conflict: stream {stream_id} expected version {expected}, found {actual}"
    )]
    ConcurrencyConflict {
        stream_id: StreamId,
        expected: StreamSequenceId,
        actual: StreamSequenceId,
    },

    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),

    // Phase 6 will add:
    // ConstraintViolation { ... }
}
```

### Pattern 4: EventLog Struct and Clone

```rust
// Static dispatch, owns the store (D-07)
pub struct EventLog<S: LogStore> {
    store: S,
}

impl<S: LogStore> EventLog<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

// Clone derives naturally if S: Clone
impl<S: LogStore + Clone> Clone for EventLog<S> {
    fn clone(&self) -> Self {
        Self { store: self.store.clone() }
    }
}
// OR equivalently: #[derive(Clone)] with `where S: Clone` — both compile identically
```

`#[derive(Clone)]` on the struct is the idiomatic approach. Rust's derive macro generates `impl<S: LogStore + Clone> Clone for EventLog<S>` automatically when `S: Clone` — this is the correct semantic: cloning an `EventLog<InMemoryLogStore>` shares the same underlying store (because `InMemoryLogStore` is `Arc`-backed), which is the desired behavior.

### Anti-Patterns to Avoid

- **Double-enforcing concurrency at the `EventLog` level:** D-06 is explicit — no secondary version check. The storage layer is the authority.
- **Wrapping `StoreError` in `EventLogError` for read methods:** Breaks the "same signatures" contract of D-03 and creates unnecessary unwrapping for Phase 7.
- **Adding `Arc` to `EventLog`:** D-07 is explicit — sharing is the store's responsibility. `InMemoryLogStore` already handles this via its internal `Arc<RwLock<...>>`.
- **Named helper methods like `append_any` / `append_with_version`:** D-05 forbids these. `AppendCondition` is the explicit, self-documenting API.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error conversion boilerplate | Manual `impl From<AppendError> for EventLogError>` with match | Explicit match in `map_err` | `From` would make the conversion implicit and confuse the clean layer boundary; `map_err` with an explicit match makes the mapping visible and auditable |
| Async trait dispatching | Re-implementing async dispatch | Native `async fn` in the `impl` block | The trait is already defined with native `async fn`; calling it is just `.await` |

---

## Runtime State Inventory

Not applicable — Phase 3 is a greenfield addition to the core crate. No rename or migration involved.

---

## Common Pitfalls

### Pitfall 1: `EventLog` Associated Type Exposure

**What goes wrong:** Callers of `EventLog::read_stream` need to collect the stream. If `EventLog` doesn't expose `S::EventStream` in its public API surface (e.g., returns `impl Stream` or a boxed stream), callers may have trouble naming the return type for storage or iteration.

**Why it happens:** `impl Trait` in return position works well for functions but creates type-naming issues when the caller needs to store the stream in a struct field or pass it across `.await` boundaries.

**How to avoid:** Since `EventLog` is generic over `S`, the concrete return type `S::EventStream` is knowable at the call site. Use `-> Result<S::EventStream, StoreError>` as the return type on all read methods — don't box or erase. This matches D-03's "same signatures as `LogStore`" mandate and avoids the allocation.

**Warning signs:** If a test can't call `.collect::<Vec<_>>().await` on the return value without a `Box<dyn Stream>`, the return type has been over-abstracted.

### Pitfall 2: Forgetting `where S: Send + Sync` on Async Methods

**What goes wrong:** The `async fn` impls in `EventLog` need `S: Send + Sync` to satisfy the `Send` bound on the future. The `LogStore` trait already requires `Self: Send + Sync`, so this is inherited — but it must not be stripped by an overly narrow `where` clause.

**Why it happens:** Writing `impl<S: LogStore> EventLog<S>` without thinking about it is fine — `LogStore: Send + Sync` is in the trait definition. The pitfall is if someone adds a helper that loosens the bound.

**How to avoid:** Keep the trait bound as `S: LogStore` on `EventLog<S>` — `LogStore` already implies `Send + Sync` per the trait definition (`pub trait LogStore: Send + Sync`). No additional bounds needed.

**Warning signs:** Compiler error: "future cannot be sent between threads safely" or "`S` cannot be shared between threads safely".

### Pitfall 3: `#[derive(Clone)]` Requiring `S: Clone` Even When S Is `Arc`-backed

**What goes wrong:** `#[derive(Clone)]` on `EventLog<S>` generates a `Clone` impl that requires `S: Clone`. If a user has a custom `LogStore` implementation that doesn't implement `Clone`, `EventLog<S>` won't be `Clone` either — which is the correct behavior, but may surprise users.

**Why it happens:** The derive generates a bound `where S: Clone`, not `where S: LogStore + Clone`. This is intentional and correct — `EventLog` should only be `Clone` when the underlying store is.

**How to avoid:** Document this explicitly. `InMemoryLogStore` is `Clone` (Arc-backed), so Phase 3 tests work fine. Users implementing custom stores who want sharing should make their store `Clone`.

**Warning signs:** Test that clones `EventLog` fails to compile after switching to a custom non-Clone store.

### Pitfall 4: Test Coverage Gap on Concurrent Append Race

**What goes wrong:** Tests verify that `ConcurrencyConflict` is returned when the expected version doesn't match, but don't test the actual concurrent race (two tasks competing simultaneously).

**Why it happens:** The sequential "set version, then append with wrong version" test proves the logic but not the atomicity guarantee (success criterion #3: exactly one success, one conflict, no race window).

**How to avoid:** Write a tokio concurrent test using `tokio::spawn` to send two appends simultaneously against the same stream with the same `ExpectedVersion`. Assert one succeeds and one returns `ConcurrencyConflict`. This tests the `EventLog` layer specifically (not just the store layer).

**Warning signs:** No test with `tokio::join!` or multiple `spawn` calls.

---

## Code Examples

### Complete `EventLog` Struct and `append`

```rust
// event-sourcing/src/event_log.rs
use crate::error::{AppendCondition, AppendError, EventLogError, StoreError};
use crate::store::LogStore;
use crate::types::{GlobalSequenceId, NewEvent, StreamId, StreamSequenceId};

pub struct EventLog<S: LogStore> {
    store: S,
}

impl<S: LogStore> EventLog<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, EventLogError> {
        self.store
            .append(stream_id, events, condition)
            .await
            .map_err(|e| match e {
                AppendError::ConcurrencyConflict { stream_id, expected, actual } => {
                    EventLogError::ConcurrencyConflict { stream_id, expected, actual }
                }
                AppendError::StorageFailure(source) => {
                    EventLogError::StorageFailure(source)
                }
            })
    }

    pub async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
        to: Option<StreamSequenceId>,
    ) -> Result<S::EventStream, StoreError> {
        self.store.read_stream(stream_id, from, to).await
    }

    pub async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<S::EventStream, StoreError> {
        self.store.read_all(from).await
    }
}

impl<S: LogStore + Clone> Clone for EventLog<S> {
    fn clone(&self) -> Self {
        Self { store: self.store.clone() }
    }
}
```

### Test: Concurrency Conflict Surfaces at EventLog Level

```rust
// Verifies LOG-04 is observable through the EventLog API
#[tokio::test]
async fn append_returns_concurrency_conflict_at_event_log_level() {
    let store = InMemoryLogStore::new();
    let log = EventLog::new(store);
    let sid = StreamId::new("orders").unwrap();

    log.append(
        &sid,
        vec![NewEvent { event_type: "E1".into(), payload: serde_json::json!({}) }],
        AppendCondition::Any,
    )
    .await
    .unwrap();

    // Stream is at version 1; supply ExpectedVersion(0) — should conflict
    let result = log
        .append(
            &sid,
            vec![NewEvent { event_type: "E2".into(), payload: serde_json::json!({}) }],
            AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
        )
        .await;

    match result {
        Err(EventLogError::ConcurrencyConflict { expected, actual, .. }) => {
            assert_eq!(expected, StreamSequenceId::ZERO);
            assert_eq!(actual, StreamSequenceId::new(1));
        }
        other => panic!("expected ConcurrencyConflict, got {:?}", other),
    }
}
```

### Test: Successful Append Returns GlobalSequenceId (LOG-06)

```rust
#[tokio::test]
async fn append_returns_global_sequence_id() {
    let store = InMemoryLogStore::new();
    let log = EventLog::new(store);
    let sid = StreamId::new("orders").unwrap();

    let seq = log
        .append(
            &sid,
            vec![NewEvent { event_type: "E1".into(), payload: serde_json::json!({}) }],
            AppendCondition::Any,
        )
        .await
        .unwrap();

    assert_eq!(seq, GlobalSequenceId::new(1));
}
```

### Test: Concurrent Race (Success Criterion #3)

```rust
#[tokio::test]
async fn concurrent_appends_yield_exactly_one_conflict() {
    let store = InMemoryLogStore::new();
    let log = std::sync::Arc::new(EventLog::new(store));
    let sid = StreamId::new("orders").unwrap();

    // Establish initial state at version 0
    let log_a = log.clone();
    let log_b = log.clone();
    let sid_a = sid.clone();
    let sid_b = sid.clone();

    let (res_a, res_b) = tokio::join!(
        log_a.append(
            &sid_a,
            vec![NewEvent { event_type: "A".into(), payload: serde_json::json!({}) }],
            AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
        ),
        log_b.append(
            &sid_b,
            vec![NewEvent { event_type: "B".into(), payload: serde_json::json!({}) }],
            AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
        ),
    );

    let successes = [res_a.is_ok(), res_b.is_ok()].iter().filter(|&&b| b).count();
    let conflicts = [res_a.is_err(), res_b.is_err()].iter().filter(|&&b| b).count();
    assert_eq!(successes, 1, "exactly one append should succeed");
    assert_eq!(conflicts, 1, "exactly one append should conflict");
}
```

**Note on `Arc` in test:** The concurrent test needs `Arc<EventLog<S>>` because `tokio::join!` requires both futures to be `Send`. Since `EventLog` doesn't derive `Clone` unless `S: Clone`, and `InMemoryLogStore: Clone`, the test can alternatively use `let log2 = log.clone()` if `EventLog<InMemoryLogStore>` derives `Clone`. Both approaches work.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `async-trait` proc macro for `dyn Trait` | Native `async fn in trait` (stable 1.75, Dec 2023) | Rust 1.75 | No proc macro dep for static dispatch; Phase 3 follows the Phase 1/2 pattern of native `async fn` |
| Boxing the stream return type (`Box<dyn Stream>`) | Associated type `type EventStream: Stream` | Design decision in Phase 1 | Zero allocation on read; concrete type knowable at call site; Phase 3 passes `S::EventStream` through unchanged |

---

## Open Questions

None. The phase is fully specified by the locked decisions and the existing codebase. All types, traits, and patterns are established. Phase 3 is an additive composition of existing pieces.

---

## Environment Availability

Step 2.6: SKIPPED — Phase 3 is a pure code addition to the `event-sourcing` core crate. No external tools, services, databases, or CLIs are required beyond the Rust toolchain already present in the workspace.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` + `#[tokio::test]` |
| Config file | None — workspace Cargo.toml with `tokio` in `[workspace.dependencies]` |
| Quick run command | `cargo test -p event-sourcing` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LOG-04 | `EventLog::append` rejects with `EventLogError::ConcurrencyConflict` when stream advanced | unit | `cargo test -p event-sourcing append_returns_concurrency_conflict` | Wave 0 |
| LOG-04 | Concurrent race yields exactly one success and one conflict | integration | `cargo test -p event-sourcing concurrent_appends_yield_exactly_one_conflict` | Wave 0 |
| LOG-05 | `EventLog::read_stream` returns events in stream-sequence order | unit | `cargo test -p event-sourcing read_stream_returns_events_in_order` | Wave 0 |
| LOG-05 | `EventLog::read_stream` supports partial range (from, to) | unit | `cargo test -p event-sourcing read_stream_range` | Wave 0 |
| LOG-06 | Successful `EventLog::append` returns the new `GlobalSequenceId` | unit | `cargo test -p event-sourcing append_returns_global_sequence_id` | Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p event-sourcing`
- **Per wave merge:** `cargo test`
- **Phase gate:** `cargo test` green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `event-sourcing/src/event_log.rs` — new module, does not exist yet
- [ ] Tests are co-located in `event_log.rs` under `#[cfg(test)]` — no separate test file needed

*(No framework install gaps — `tokio` with `rt` + `macros` features is already in `[workspace.dependencies]` and `event-sourcing-logstore-inmemory`'s dev-deps. The core crate's `Cargo.toml` will need `tokio` added to `[dev-dependencies]` for the `#[tokio::test]` attribute in Phase 3 tests.)*

**Dev-dependency gap:** `event-sourcing/Cargo.toml` currently has no `[dev-dependencies]`. Phase 3 tests in the core crate need:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
event-sourcing-logstore-inmemory = { path = "../event-sourcing-logstore-inmemory" }
futures = "0.3"
serde_json = "1"
```

This is a Wave 0 task for the planner.

---

## Sources

### Primary (HIGH confidence)

- Existing codebase — `event-sourcing/src/error.rs`, `store.rs`, `types.rs`, `event.rs`, `lib.rs` — read directly; all type signatures and patterns verified
- Existing codebase — `event-sourcing-logstore-inmemory/src/lib.rs` — `InMemoryLogStore` implementation with concurrency tests; storage-layer guarantee confirmed
- `03-CONTEXT.md` (all locked decisions D-01 through D-08) — authoritative for all architecture choices
- `01-CONTEXT.md` and `02-CONTEXT.md` — upstream contracts confirmed
- `CLAUDE.md` — stack versions confirmed; `thiserror 2.0` pattern matches existing crate

### Secondary (MEDIUM confidence)

- Rust Reference on `#[derive(Clone)]` — automatic `where S: Clone` bound generation is standard Rust behavior (well-established, HIGH confidence from language semantics)

### Tertiary (LOW confidence)

None — all findings grounded in the actual codebase and locked decisions.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies; all in workspace already
- Architecture: HIGH — all patterns established in Phase 1/2; locked decisions remove all ambiguity
- Pitfalls: HIGH — derived from actual codebase patterns and Rust type system behavior
- Tests: HIGH — pattern mirrors Phase 2 test structure exactly

**Research date:** 2026-04-06
**Valid until:** 2026-05-06 (stable — no external dependencies or ecosystem churn)
