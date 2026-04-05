# Phase 1 Plan: Workspace Setup and Core Types

**Created:** 2026-04-05
**Requirements:** STOR-01, LOG-01, LOG-02, LOG-03, LOG-07
**Decisions:** D-01 through D-18 (see 01-CONTEXT.md)

---

## Overview

Scaffold the Cargo workspace and define all core types, error enums, and the `LogStore` trait. No I/O, no implementations — just types and contracts. Every downstream phase imports from this foundation.

**Decision override:** Success criterion #4 in ROADMAP.md references `async-trait` and `dyn LogStore`. Decision D-11 (from context gathering) explicitly chose native `async fn in trait` with static dispatch. D-11 takes precedence as the deliberate refinement. The `dyn LogStore` escape hatch can be added later as an `AnyLogStore` wrapper (noted in 01-CONTEXT.md specifics).

---

## Plan Nodes

### Node 1: Workspace Scaffold

**Goal:** Cargo workspace compiles with all crate stubs.

**Files to create:**
- `/Cargo.toml` — workspace root with `[workspace]` members and `[workspace.dependencies]`
- `/event-sourcing/Cargo.toml` — core crate
- `/event-sourcing/src/lib.rs` — empty lib
- `/event-sourcing-logstore-inmemory/Cargo.toml` — placeholder, depends on `event-sourcing`
- `/event-sourcing-logstore-inmemory/src/lib.rs` — empty lib
- `/event-sourcing-logstore-sqlite/Cargo.toml` — placeholder, depends on `event-sourcing`
- `/event-sourcing-logstore-sqlite/src/lib.rs` — empty lib
- `/event-sourcing-commands/Cargo.toml` — placeholder, depends on `event-sourcing`
- `/event-sourcing-commands/src/lib.rs` — empty lib

**Workspace dependencies (D-17):**
- `serde = { version = "^1.0.220", features = ["derive"] }`
- `serde_json = "^1.0.149"`
- `thiserror = "2.0"`
- `futures-core = "0.3"`

**Dev dependencies (workspace level):**
- `tokio = { version = "^1.48", features = ["rt", "macros"] }` — for `#[tokio::test]`
- `serde_json` (also dev for tests)

**Verification:** `cargo check` passes for the entire workspace.

---

### Node 2: Stream Identity Types

**Goal:** `StreamId` newtype with explicit construction and validation (D-01, D-02, D-03).

**File:** `event-sourcing/src/types.rs` (new module)

**Types:**
```rust
/// Newtype for stream identity. Explicit construction only — no From/Into (D-01).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(String);

impl StreamId {
    /// Returns Err if the string is empty (D-02).
    pub fn new(value: impl Into<String>) -> Result<Self, EmptyStreamIdError>;

    /// Borrow the inner string.
    pub fn as_str(&self) -> &str;
}

/// Error returned when constructing a StreamId with an empty string.
#[derive(Debug, Clone, thiserror::Error)]
#[error("StreamId cannot be empty")]
pub struct EmptyStreamIdError;
```

**Design choice (Claude's discretion):** `StreamId::new()` returns `Result` rather than panicking — consistent with library design where callers handle errors.

**Verification:** Unit test constructs valid and empty `StreamId`, asserts `Eq`/`Hash` behavior.

---

### Node 3: Sequence ID Types

**Goal:** `GlobalSequenceId(u64)` and `StreamSequenceId(u64)` as separate newtypes (D-05, D-06).

**File:** `event-sourcing/src/types.rs` (same module as Node 2)

**Types:**
```rust
/// Global monotonically increasing sequence across all streams.
/// Starts from 1; 0 means "no events seen" (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalSequenceId(u64);

/// Per-stream sequence number.
/// Starts from 1; 0 means "beginning of time" (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamSequenceId(u64);
```

Both expose:
- `pub fn new(value: u64) -> Self`
- `pub fn as_u64(&self) -> u64`
- `Display` impl

**Verification:** Unit tests confirm ordering, zero semantics, and that the two types cannot be mixed at compile time.

---

### Node 4: StoredEvent

**Goal:** `StoredEvent` struct carrying all required metadata (D-04, D-07, D-08, D-09).

**File:** `event-sourcing/src/event.rs` (new module)

**Type:**
```rust
/// An event as stored in the log. Immutable after creation (LOG-01).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Global position across all streams (LOG-03, D-04).
    pub global_sequence_id: GlobalSequenceId,
    /// Position within its stream (LOG-02).
    pub stream_sequence_id: StreamSequenceId,
    /// Which stream this event belongs to.
    pub stream_id: StreamId,
    /// Discriminator for filtering/routing without deserializing payload (D-08).
    pub event_type: String,
    /// The event payload as opaque JSON (D-07).
    pub payload: serde_json::Value,
    /// Timestamp set by the store at append time (D-09).
    pub timestamp: std::time::SystemTime,
}
```

**No per-event UUID (D-04).** Events are identified by `(StreamId, StreamSequenceId)` or `GlobalSequenceId`.

**Verification:** Unit test constructs a `StoredEvent`, verifies all fields accessible, clones correctly.

---

### Node 5: Error Types

**Goal:** Typed error enums for append and read operations (D-14, D-15, LOG-07).

**File:** `event-sourcing/src/error.rs` (new module)

**Types:**
```rust
/// Error from an append operation (D-14).
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error("concurrency conflict: stream has advanced past expected version")]
    ConcurrencyConflict,

    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Error from a read operation (D-15).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

**Verification:** Unit test matches on `AppendError::ConcurrencyConflict` and `AppendError::StorageFailure` to confirm pattern matching works (LOG-07 success criterion).

---

### Node 6: LogStore Trait

**Goal:** Async storage trait with 5 methods, static dispatch, no `async-trait` (D-10, D-11, D-12, D-13).

**File:** `event-sourcing/src/store.rs` (new module)

**Dependencies:** `futures-core` for the `Stream` trait.

**Type:**
```rust
use futures_core::Stream;

/// The condition for an optimistic concurrency check on append.
pub enum AppendCondition {
    /// Append only if stream is at exactly this version.
    ExpectedVersion(StreamSequenceId),
    /// Append regardless of current stream version.
    Any,
}

/// Storage backend trait. Implementations live in separate crates (D-16).
/// Uses native async fn in trait with static dispatch (D-11).
pub trait LogStore: Send + Sync {
    /// The stream type returned by read operations (D-12).
    type EventStream: Stream<Item = Result<StoredEvent, StoreError>> + Send;

    /// Append events to a stream with an optimistic concurrency condition.
    /// Returns the GlobalSequenceId of the last appended event.
    async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, AppendError>;

    /// Read all events for a stream, optionally from a starting sequence (D-13).
    async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
    ) -> Result<Self::EventStream, StoreError>;

    /// Read all events across all streams from a global sequence position.
    async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError>;

    /// Get the current global sequence ID (highest assigned).
    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError>;

    /// Get the current version of a specific stream.
    async fn stream_version(
        &self,
        stream_id: &StreamId,
    ) -> Result<StreamSequenceId, StoreError>;
}
```

**Supporting type:**
```rust
/// Event data before it is stored (no sequence IDs or timestamp yet).
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}
```

**Note on D-12:** `Self::EventStream` is an associated type bounded by `Stream + Send`. This allows each implementation to return its own stream type (e.g., `vec::IntoIter` wrapper for in-memory, channel receiver for SQLite).

**Verification:** `cargo check` confirms trait compiles. A dummy struct with stub impl (in tests only) validates the trait is implementable.

---

### Node 7: Module Structure and Re-exports

**Goal:** Clean public API from `event-sourcing` core crate (D-18).

**File:** `event-sourcing/src/lib.rs`

**Structure:**
```
event-sourcing/src/
  lib.rs          — pub mod + re-exports
  types.rs        — StreamId, GlobalSequenceId, StreamSequenceId, EmptyStreamIdError
  event.rs        — StoredEvent, NewEvent
  error.rs        — AppendError, StoreError
  store.rs        — LogStore trait, AppendCondition
```

`lib.rs` re-exports all public types at crate root for ergonomic imports:
```rust
pub mod types;
pub mod event;
pub mod error;
pub mod store;

// Re-export primary types at crate root
pub use types::{StreamId, GlobalSequenceId, StreamSequenceId, EmptyStreamIdError};
pub use event::{StoredEvent, NewEvent};
pub use error::{AppendError, StoreError};
pub use store::{LogStore, AppendCondition};
```

**No re-exports from core into sibling crates (D-18).**

**Verification:** Placeholder sibling crate can `use event_sourcing::StreamId;` and compile.

---

### Node 8: Tests

**Goal:** Validate all success criteria.

**File:** `event-sourcing/tests/core_types.rs` (integration test)

**Test cases:**
1. **StreamId construction** — valid string succeeds, empty string returns `Err(EmptyStreamIdError)`
2. **StreamId equality/hash** — two `StreamId`s with same string are equal
3. **Sequence ID ordering** — `GlobalSequenceId(1) < GlobalSequenceId(2)`, same for `StreamSequenceId`
4. **Sequence ID type safety** — `GlobalSequenceId` and `StreamSequenceId` are distinct types (compile-time, not a runtime test)
5. **StoredEvent construction** — build a `StoredEvent` with all fields, verify access
6. **AppendError matching** — `match` on `ConcurrencyConflict` vs `StorageFailure` (LOG-07)
7. **LogStore trait implementability** — define a minimal stub impl in the test, verify it compiles
8. **Cross-crate import** — in `event-sourcing-logstore-inmemory/src/lib.rs`, add `use event_sourcing::LogStore;` to prove the import path works

**Verification:** `cargo test --workspace` passes.

---

## Execution Order

```
Node 1 (Workspace Scaffold)
  └──▶ Node 2 (StreamId) + Node 3 (SequenceIds)  [parallel]
         └──▶ Node 4 (StoredEvent)
               └──▶ Node 5 (Error Types)  [parallel with Node 4 if desired]
                     └──▶ Node 6 (LogStore Trait)
                           └──▶ Node 7 (Module Structure)
                                 └──▶ Node 8 (Tests)
```

Nodes 2+3 can run in parallel. Node 5 has no dependency on Node 4, so those could also be parallel. Node 6 depends on all type definitions. Node 8 is last.

---

## Success Criteria Checklist

| # | Criterion | Verified By |
|---|-----------|-------------|
| SC-1 | Cargo workspace compiles with core + placeholder sibling crates | Node 1: `cargo check` |
| SC-2 | `StoredEvent` carries global seq ID, stream seq, typed payload, no I/O | Node 4: construction test |
| SC-3 | `AppendError` distinguishes `ConcurrencyConflict` from `StorageFailure` | Node 5: match test |
| SC-4 | `LogStore` trait compiles with static dispatch, native async fn | Node 6: stub impl test |
| SC-5 | Sibling crates can import core types | Node 8: cross-crate import |

---

## Risk Notes

- **`futures-core` dependency:** Lightweight (trait definitions only, no runtime). Required for `Stream` bound on associated type. No alternative without reimplementing the trait.
- **No `dyn LogStore` support yet:** Intentional per D-11. If needed later, an `AnyLogStore` wrapper using `async-trait` can be added without changing the core trait.
- **`serde_json::Value` in `StoredEvent`:** Per D-07, streams are heterogeneous. Type safety comes from deserialization in projections, not from the storage model.

---

*Plan created: 2026-04-05*
*Phase: 01-workspace-setup-and-core-types*
