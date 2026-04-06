# Phase 2: In-Memory Log Store - Research

**Researched:** 2026-04-05
**Domain:** Rust in-memory storage implementation, async trait implementation, interior mutability patterns
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Drop the `dyn LogStore` requirement. No `async-trait` dependency. The Phase 1 trait design using native `async fn in trait` is kept as-is. Callers use generic `S: LogStore` bounds only.
- **D-02:** Phase 2 success criteria #5 ("pass as `dyn LogStore` without unsafe code") is explicitly relaxed. This was incompatible with Phase 1's native `async fn in trait` design.
- **D-03:** `InMemoryLogStore` uses Arc-backed interior storage (`Arc<RwLock<State>>`). `clone()` returns another handle to the **same** underlying store — not a copy. Tests can pass clones to multiple components and observe shared state without wrapping in `Arc` themselves.
- **D-04:** `read_stream` on a stream that has never received events returns `Ok(empty_stream)` — not an error. In an append-only log, "stream not found" and "stream is empty" are semantically identical (streams cannot be emptied or deleted). Callers check for emptiness, not errors.

### Claude's Discretion

- Internal concurrency primitive choice (`tokio::sync::RwLock` vs `std::sync::RwLock`)
- Internal data structure for event storage (e.g., `HashMap<StreamId, Vec<StoredEvent>>`)
- Global sequence counter strategy (inside lock vs `AtomicU64`)
- Concrete type used for `EventStream` associated type
- Test coverage scope and structure

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| STOR-02 | In-memory log store as separate crate (`event-sourcing-logstore-inmemory`) | Crate wiring pattern, LogStore trait impl, Arc/RwLock interior state, Vec event storage, futures_core stream output |
</phase_requirements>

---

## Summary

Phase 2 implements `InMemoryLogStore` — a fully in-memory, `LogStore`-conforming storage backend in a separate crate. Its primary purpose is to serve as the test harness for all subsequent phases: any phase that needs a storage backend to test against will use this crate. It is not a toy — it must correctly implement all `LogStore` methods including global monotonic sequence assignment, per-stream sequence tracking, and range-based reads.

The implementation centers on three design decisions that are now locked. First, the sharing model: `InMemoryLogStore` wraps `Arc<RwLock<InnerState>>` so that `clone()` shares the same backing store rather than copying it. This is the ergonomic pattern for test scenarios where multiple components need to observe each other's writes. Second, the async story: Phase 1 chose native `async fn in trait`, which means no `async-trait` dependency here — all `async fn` implementations are plain native syntax. Third, the empty-stream contract: reading a stream that has never been written to returns `Ok(empty)`, never an error.

The two discretionary areas with the highest implementation impact are (a) the concurrency primitive (`tokio::sync::RwLock` is recommended because this crate is fully async and blocking a thread inside an async executor under contention is a silent correctness hazard) and (b) the concrete `EventStream` associated type (a `futures::stream::Iter` wrapping a collected `Vec<Result<StoredEvent, StoreError>>` is the simplest correct answer — no heap-allocated async stream needed for in-memory reads).

**Primary recommendation:** Use `tokio::sync::RwLock<InnerState>` with `AtomicU64` for the global sequence counter. Return a `futures::stream::Iter<std::vec::IntoIter<Result<StoredEvent, StoreError>>>` as the `EventStream` associated type.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.x (^1.48) | Async runtime; `tokio::sync::RwLock` for guarding inner state | Project-mandated; only viable async runtime after async-std discontinuation |
| futures-core | 0.3 | `Stream` trait — required for `LogStore::EventStream` associated type bound | Phase 1 decision; lightweight dep without pulling in all of `futures` |
| futures | 0.3 | `stream::iter()` utility to construct a `Stream` from a `Vec` | Provides `stream::iter` and `StreamExt`; use only the needed adapter |
| thiserror | 2.0 | Error type derives (for `StoreError`, `AppendError` — defined in core but used here) | Project-mandated |
| event-sourcing | (workspace path dep) | Core types: `LogStore`, `StoredEvent`, `StreamId`, `GlobalSequenceId`, `StreamSequenceId`, `AppendError`, `StoreError` | This crate implements the trait defined there |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std::sync::atomic::AtomicU64 | stdlib | Global sequence counter with lock-free monotonic increment | Use in conjunction with `fetch_add(1, Ordering::SeqCst)` inside write path; avoids holding the RwLock for seq allocation |
| std::collections::HashMap | stdlib | Map from `StreamId` to `Vec<StoredEvent>` | Primary event storage structure inside `InnerState` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio::sync::RwLock` | `std::sync::RwLock` | `std::sync::RwLock::write()` blocks the OS thread; inside a tokio async context this starves the executor under contention. `tokio::sync::RwLock` yields to the executor instead. Use tokio's version. |
| `AtomicU64` for global seq | Sequence field inside RwLock state | Both are correct. `AtomicU64` avoids holding the write lock for sequence allocation, reducing contention when many concurrent appends race. Either works for in-memory; AtomicU64 is cleaner. |
| `futures::stream::iter` | Custom `Stream` impl | Rolling a custom `Stream` struct requires implementing `Poll`; `stream::iter` wraps any `IntoIterator` and is correct for synchronous, fully-buffered data. In-memory reads are synchronous — use `stream::iter`. |
| `futures` crate (full) | `futures-core` only | `futures-core` provides the `Stream` trait definition; `futures` adds utilities including `stream::iter`. If `stream::iter` or `StreamExt` is needed, depend on `futures`. If only the trait is referenced, `futures-core` suffices. |

**Installation:**
```toml
# event-sourcing-logstore-inmemory/Cargo.toml
[dependencies]
event-sourcing = { path = "../event-sourcing" }
futures = "0.3"
tokio = { version = "1", features = ["sync"] }
```

---

## Architecture Patterns

### Recommended Project Structure

```
event-sourcing-logstore-inmemory/
├── Cargo.toml
└── src/
    └── lib.rs          # InMemoryLogStore, InnerState, LogStore impl
```

Single-file is appropriate for this phase. If the `EventStream` type grows complex, extract to `src/stream.rs`. No binary — library crate only.

### Pattern 1: Arc-Backed Interior Mutability with Shared Clone

**What:** `InMemoryLogStore` is a thin newtype over `Arc<tokio::sync::RwLock<InnerState>>`. The `Clone` impl is derived or manual — it clones the `Arc`, not the data.

**When to use:** Any in-memory store that must be handed to multiple callers (test components, `EventLog`, etc.) without each caller owning their own isolated copy.

**Example:**
```rust
// Source: Phase 2 CONTEXT.md D-03; tokio::sync::RwLock docs
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct InMemoryLogStore {
    inner: Arc<RwLock<InnerState>>,
}

struct InnerState {
    streams: std::collections::HashMap<StreamId, Vec<StoredEvent>>,
    // AtomicU64 lives outside RwLock — no lock needed for seq allocation
}
```

### Pattern 2: AtomicU64 for Lock-Free Global Sequence

**What:** The global sequence counter lives outside the `RwLock` as an `Arc<AtomicU64>`. The append path increments it with `fetch_add(1, Ordering::SeqCst)` before acquiring the write lock.

**When to use:** When the sequence counter must be monotonically increasing even across concurrent appends. The `SeqCst` ordering guarantees that all threads see the same increment order.

**Example:**
```rust
// Source: stdlib std::sync::atomic docs
use std::sync::atomic::{AtomicU64, Ordering};

pub struct InMemoryLogStore {
    inner: Arc<RwLock<InnerState>>,
    next_global_seq: Arc<AtomicU64>,
}

// In append():
let global_seq = self.next_global_seq.fetch_add(1, Ordering::SeqCst);
let global_seq_id = GlobalSequenceId(global_seq + 1); // 1-based per Phase 1 D-06
```

> **Note:** `fetch_add` returns the old value before the increment. Add 1 to get the new value. Initialize the `AtomicU64` to 0; first append produces `GlobalSequenceId(1)`.

### Pattern 3: Constructing EventStream from Vec

**What:** After reading events from the `HashMap`, collect them into a `Vec`, release the lock, then wrap the vec in `futures::stream::iter`. The caller gets a `Stream` without holding the lock during iteration.

**When to use:** Any time a `Stream<Item = Result<StoredEvent, StoreError>>` must be returned from an in-memory read — always for this crate.

**Example:**
```rust
// Source: futures 0.3 docs — stream::iter
use futures::stream;

// After collecting from state (lock released):
let events: Vec<Result<StoredEvent, StoreError>> = collected
    .into_iter()
    .map(Ok)
    .collect();
stream::iter(events)
```

The concrete return type is `futures::stream::Iter<std::vec::IntoIter<Result<StoredEvent, StoreError>>>`. This must be the `type EventStream` associated type declaration in the `LogStore` impl block.

### Pattern 4: Range Reads with Stream Sequence Numbers

**What:** `read_stream` with a range filter retains events whose `stream_sequence_id` falls within `[from, to]`. Because `Vec<StoredEvent>` is stored in insertion order (always incrementing stream sequence), a slice or `filter` over the vec is sufficient — no binary search required for correctness, though binary search is an optional optimization.

**When to use:** Implementing `read_stream(stream_id, from: Option<StreamSequenceId>, to: Option<StreamSequenceId>)`.

**Example:**
```rust
// Filter by stream sequence range:
let filtered: Vec<StoredEvent> = events
    .iter()
    .filter(|e| {
        from.map_or(true, |f| e.stream_sequence_id >= f) &&
        to.map_or(true, |t| e.stream_sequence_id <= t)
    })
    .cloned()
    .collect();
```

### Anti-Patterns to Avoid

- **Holding the RwLock across an await point:** Never hold a `tokio::sync::RwLock` guard across `.await`. Acquire, read/write what you need, clone/collect, drop the guard, then do anything async. This is a compile-time error with `std::sync::RwLock` (non-`Send`) but silently miscompiles with `tokio::sync::RwLock` if not careful.
- **Returning a reference into locked state:** The associated `EventStream` type must be `'static` (or at least owned) — it cannot borrow from the lock guard. Always collect events into a `Vec` before returning the stream.
- **Using `std::sync::RwLock` in async code:** Blocks the tokio thread under contention. Use `tokio::sync::RwLock`.
- **Deriving `Clone` on a struct with `tokio::sync::RwLock` directly (not Arc-wrapped):** A bare `RwLock` cannot be cloned — it must be wrapped in `Arc` first. The pattern in D-03 is `Arc<RwLock<...>>`, which is cheaply cloneable.
- **Starting GlobalSequenceId from 0:** Phase 1 D-06 specifies sequences start from 1. Initialize `AtomicU64` to 0, first `fetch_add(1, ...) + 0 = 1`. Alternatively start at 0 and add 1 after fetch. Double-check the off-by-one.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Stream from iterator | Manual `Poll`-based `Stream` struct | `futures::stream::iter` | Correct for synchronous data; manual `Stream` impl is error-prone, requires nightly or careful unsafe, unnecessary here |
| Async-safe RwLock | Custom lock primitive | `tokio::sync::RwLock` | Correct executor integration, cancellation safety, widely tested |
| Monotonic counter | Manual mutex-guarded `u64` | `std::sync::atomic::AtomicU64` | Lock-free, correct under all orderings with `SeqCst`, zero extra dependency |

**Key insight:** The in-memory store is fundamentally synchronous data behind an async facade. Every "async" operation in the impl can be: acquire lock → operate on `HashMap`/`Vec` → release lock → return. No real async I/O occurs. The async signature is there to satisfy the trait contract, not because the work is actually concurrent.

---

## Common Pitfalls

### Pitfall 1: Holding Lock Guard Across Await
**What goes wrong:** A write guard or read guard is held while an `.await` is hit. With `tokio::sync::RwLock` this compiles but creates a deadlock if another task tries to acquire the lock on the same thread.
**Why it happens:** It looks natural to write `let guard = self.inner.write().await; /* do stuff */; guard.streams.insert(...)`. If anything async is interleaved before the guard drops, deadlock follows.
**How to avoid:** Scope the lock tightly. Acquire, operate on data, explicitly drop (or let scope end) before any `.await`. In practice: do all lock-protected work synchronously inside a block `{ let mut guard = ...; /* sync work */ }`, then proceed async outside.
**Warning signs:** Any `let guard = lock.write().await` followed by an `.await` expression before `drop(guard)`.

### Pitfall 2: Off-By-One in GlobalSequenceId Initialization
**What goes wrong:** First append returns `GlobalSequenceId(0)` instead of `GlobalSequenceId(1)`.
**Why it happens:** `AtomicU64::fetch_add(1, SeqCst)` returns the value *before* the add. If initialized to 0, the first call returns 0. Adding 1 afterward gives 1 — which is correct. But if code uses the raw return value, it gets 0.
**How to avoid:** Either (a) initialize `AtomicU64` to 0 and always add 1 to the return of `fetch_add` before wrapping in `GlobalSequenceId`, or (b) initialize to 1 and use the return value directly. Option (a) is more conventional.
**Warning signs:** Integration test where first appended event has `global_sequence_id == 0`.

### Pitfall 3: StreamSequenceId Reuse Across Streams
**What goes wrong:** Stream sequence numbers are accidentally global instead of per-stream.
**Why it happens:** Using the same counter for both global and stream sequences.
**How to avoid:** `StreamSequenceId` is derived from `streams[stream_id].len() + 1` at append time — the length of the existing vec before push, plus 1. This is naturally per-stream and starts at 1.
**Warning signs:** Two different streams each have events starting at `StreamSequenceId(1)`, which is correct. If both streams show the same sequence numbering as the global counter, that's wrong.

### Pitfall 4: Clone Semantics Confusion in Tests
**What goes wrong:** A test clones `InMemoryLogStore`, appends to the clone, but the original doesn't see the new events.
**Why it happens:** If `Clone` was implemented as deep-copy instead of `Arc` clone.
**How to avoid:** D-03 mandates `Arc<RwLock<...>>` interior. Derive `Clone` on the struct — `Arc::clone` is called, both handles point to the same data. Write a test that appends via a clone and reads back via the original.
**Warning signs:** Test isolation failures where a clone's writes are invisible to the original handle.

### Pitfall 5: read_all Global Ordering
**What goes wrong:** `read_all` returns events in stream-insertion order per stream but not in global sequence order across streams.
**Why it happens:** Iterating `HashMap::values()` and chaining vecs gives per-stream order, but streams may have interleaved appends globally.
**How to avoid:** `read_all` must sort by `GlobalSequenceId` before returning. Collect all events from all streams, sort by `global_sequence_id`, then wrap in `stream::iter`.
**Warning signs:** Multi-stream test where `read_all` returns stream A's events before stream B's even though stream B was appended to first.

---

## Code Examples

Verified patterns from project decisions and stdlib/tokio docs:

### InMemoryLogStore Struct Layout
```rust
// Source: Phase 2 CONTEXT.md D-03; tokio::sync docs
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct InMemoryLogStore {
    inner: Arc<RwLock<InnerState>>,
    next_seq: Arc<AtomicU64>,  // 0-initialized; first append = seq 1
}

struct InnerState {
    streams: HashMap<StreamId, Vec<StoredEvent>>,
}

impl Clone for InMemoryLogStore {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            next_seq: Arc::clone(&self.next_seq),
        }
    }
}

impl Default for InMemoryLogStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerState {
                streams: HashMap::new(),
            })),
            next_seq: Arc::new(AtomicU64::new(0)),
        }
    }
}
```

### LogStore impl skeleton (native async fn in trait)
```rust
// Source: Phase 1 CONTEXT.md D-11; Rust 1.75+ stable async fn in trait
use futures::stream::{self, Iter};
use std::vec::IntoIter;

type InMemoryEventStream = Iter<IntoIter<Result<StoredEvent, StoreError>>>;

impl LogStore for InMemoryLogStore {
    type EventStream = InMemoryEventStream;

    async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<serde_json::Value>,
        // ... other params from Phase 1 trait
    ) -> Result<GlobalSequenceId, AppendError> {
        let global_seq = self.next_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let global_sequence_id = GlobalSequenceId(global_seq);

        let mut guard = self.inner.write().await;
        let stream = guard.streams.entry(stream_id.clone()).or_default();
        let stream_seq = StreamSequenceId(stream.len() as u64 + 1);

        // construct StoredEvent, push to vec
        // ...

        Ok(global_sequence_id)
    }

    async fn read_stream(
        &self,
        stream_id: &StreamId,
        // from/to range params per Phase 1 trait
    ) -> Result<Self::EventStream, StoreError> {
        let guard = self.inner.read().await;
        let events: Vec<Result<StoredEvent, StoreError>> = guard
            .streams
            .get(stream_id)
            .map(|v| v.iter().cloned().map(Ok).collect())
            .unwrap_or_default(); // D-04: empty stream for unknown streams
        drop(guard); // release before returning
        Ok(stream::iter(events))
    }
}
```

### Range Filtering in read_stream
```rust
// Source: Phase 2 CONTEXT.md success criteria #3
let events: Vec<Result<StoredEvent, StoreError>> = guard
    .streams
    .get(stream_id)
    .map(|v| {
        v.iter()
            .filter(|e| {
                from.map_or(true, |f: StreamSequenceId| e.stream_sequence_id >= f) &&
                to.map_or(true, |t: StreamSequenceId| e.stream_sequence_id <= t)
            })
            .cloned()
            .map(Ok)
            .collect()
    })
    .unwrap_or_default();
```

### Cargo.toml for the crate
```toml
# event-sourcing-logstore-inmemory/Cargo.toml
[package]
name = "event-sourcing-logstore-inmemory"
version = "0.1.0"
edition = "2021"

[dependencies]
event-sourcing = { path = "../event-sourcing" }
futures = "0.3"
tokio = { version = "1", features = ["sync"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `async-trait` macro required for any async trait | Native `async fn in trait` stable (Rust 1.75, Dec 2023) | Rust 1.75 (Dec 2023) | No boxing overhead for static dispatch; `dyn Trait` still requires workaround |
| `async-std` as tokio alternative | tokio only — async-std discontinued | March 2025 | No alternative async runtime to choose |
| `futures::Stream` from `futures` crate as primary dep | `futures-core` for just the trait, `futures` for utilities | Ongoing split since 0.3 | `futures-core` is the minimal dep for trait bounds; `futures` adds adapters |

**Deprecated/outdated:**
- `async-trait` macro: Still valid for `dyn Trait` use cases, but not needed here per D-01. Do not add as a dependency.
- `async-std`: Discontinued March 2025. Do not use.

---

## Open Questions

1. **Exact LogStore trait signature (Phase 1 must complete first)**
   - What we know: 5 methods (append, read_stream, read_all, current_sequence, stream_version), native async fn in trait, `type EventStream: Stream<Item = Result<StoredEvent, StoreError>>`
   - What's unclear: Exact parameter signatures for `append` (does it take `expected_version`? event payloads as `Vec<serde_json::Value>` or `Vec<StoredEvent>`?), exact parameter signatures for range reads
   - Recommendation: Read Phase 1 implementation artifacts before writing the `impl LogStore` block. The trait contract is ground truth.

2. **futures crate feature flags**
   - What we know: `futures::stream::iter` is in the `futures` crate
   - What's unclear: Whether `futures` with default features is required or if `futures = { version = "0.3", default-features = false, features = ["std"] }` suffices
   - Recommendation: Start with `futures = "0.3"` (default features). Trim features only if compile time becomes a concern.

3. **Timestamp source for StoredEvent**
   - What we know: Phase 1 D-09 says `std::time::SystemTime` set by the store at append time
   - What's unclear: Whether `InMemoryLogStore::append` sets the timestamp or the `StoredEvent` constructor does
   - Recommendation: `InMemoryLogStore::append` calls `SystemTime::now()` and passes it to `StoredEvent::new()` or equivalent constructor. The store is responsible, not the caller.

---

## Environment Availability

Step 2.6: SKIPPED — this phase is code-only, implementing a Rust crate with no external dependencies beyond the Rust toolchain and Cargo. All dependencies are Cargo-managed. The environment must have `rustc` and `cargo` available, but verifying these is prerequisite to the whole project and outside the scope of this phase.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness + `#[tokio::test]` for async tests |
| Config file | none — `cargo test` / `cargo nextest run` from workspace root |
| Quick run command | `cargo test -p event-sourcing-logstore-inmemory` |
| Full suite command | `cargo nextest run -p event-sourcing-logstore-inmemory` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| STOR-02 (SC-1) | `InMemoryLogStore` implements `LogStore` and compiles | compile-check | `cargo build -p event-sourcing-logstore-inmemory` | Wave 0 |
| STOR-02 (SC-2) | Append events to stream; read back in insertion order | unit | `cargo test -p event-sourcing-logstore-inmemory test_append_and_read_stream` | Wave 0 |
| STOR-02 (SC-3) | Range read returns only requested slice | unit | `cargo test -p event-sourcing-logstore-inmemory test_read_stream_range` | Wave 0 |
| STOR-02 (SC-4) | Global seq ID monotonically increasing across streams | unit | `cargo test -p event-sourcing-logstore-inmemory test_global_sequence_monotonic` | Wave 0 |
| STOR-02 (SC-5) | (Relaxed per D-01/D-02 — static dispatch only, no dyn test needed) | manual-only | N/A — criterion dropped | N/A |
| D-03 | Clone shares state — appends via clone visible on original | unit | `cargo test -p event-sourcing-logstore-inmemory test_clone_shares_state` | Wave 0 |
| D-04 | read_stream on unknown stream returns Ok(empty) | unit | `cargo test -p event-sourcing-logstore-inmemory test_read_unknown_stream_empty` | Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo build -p event-sourcing-logstore-inmemory` (compile check)
- **Per wave merge:** `cargo test -p event-sourcing-logstore-inmemory`
- **Phase gate:** Full test suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `event-sourcing-logstore-inmemory/src/lib.rs` — main implementation file (does not exist yet — Phase 1 not executed)
- [ ] `event-sourcing-logstore-inmemory/Cargo.toml` — crate manifest
- [ ] Test functions listed above — all in `src/lib.rs` under `#[cfg(test)]` module

---

## Project Constraints (from CLAUDE.md)

All directives from `CLAUDE.md` that constrain this phase:

| Directive | Impact on Phase 2 |
|-----------|-------------------|
| Language: Rust — library crate, not binary | `event-sourcing-logstore-inmemory` must have `[lib]` in Cargo.toml, no `[[bin]]` |
| Crate separation: storage implementations are separate crates, not features | `InMemoryLogStore` lives in `event-sourcing-logstore-inmemory`, never in `event-sourcing` core |
| `tokio::sync::RwLock` preferred in async crates | Use `tokio::sync::RwLock` over `std::sync::RwLock` for the inner state guard |
| Do not use `anyhow` | All error types derive from `thiserror` (errors defined in core crate; used here) |
| Do not use `async-trait` unless `dyn Trait` is needed | D-01 confirms: no `async-trait` in this crate |
| Do not use `tokio features = ["full"]` in a library | Specify only `features = ["sync"]` in the `tokio` dep (dev-deps can add `rt`, `macros`) |
| `futures-core` for `Stream` trait; `futures` for utilities | Dep split: `futures-core` in core crate, `futures` in this crate if `stream::iter` is needed |
| No re-exports from core | Tests use `use event_sourcing::LogStore; use event_sourcing_logstore_inmemory::InMemoryLogStore;` |

---

## Sources

### Primary (HIGH confidence)

- Phase 2 `02-CONTEXT.md` — Locked decisions D-01 through D-04; Claude's Discretion areas
- Phase 1 `01-CONTEXT.md` — LogStore trait contract D-10 through D-18; type definitions
- `CLAUDE.md` — Stack versions, `tokio::sync::RwLock` preference, forbidden patterns
- Rust Reference / stdlib docs (`std::sync::atomic::AtomicU64`, `std::collections::HashMap`) — stdlib patterns, HIGH confidence from training
- tokio 1.x docs (`tokio::sync::RwLock`) — async RwLock API, HIGH confidence from training

### Secondary (MEDIUM confidence)

- futures 0.3 docs (`futures::stream::iter`) — MEDIUM confidence from training; verify exact feature flag needed when implementing
- Rust 1.75 stable `async fn in trait` blog post (Dec 2023) — confirmed stable; dyn limitation confirmed

### Tertiary (LOW confidence)

- None — all claims are either derived from project decisions (locked) or stdlib/well-known crate patterns

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all dependencies are project-mandated or stdlib
- Architecture: HIGH — patterns follow directly from locked decisions (D-03, D-04) and Phase 1 trait contract
- Pitfalls: HIGH — tokio lock-across-await is well-documented; off-by-one in atomics is a known pattern

**Research date:** 2026-04-05
**Valid until:** 2026-05-05 (stable crates; dependencies unlikely to shift)
