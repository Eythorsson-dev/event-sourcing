# Phase 1 Research: Workspace Setup and Core Types

**Researched:** 2026-04-05
**Confidence:** HIGH — greenfield Rust workspace, all patterns well-established
**Status:** RESEARCH COMPLETE

---

## 1. Cargo Workspace Structure

### Workspace Layout

The workspace needs 4 crates from the roadmap:
- `event-sourcing` — core library (types, traits, engine)
- `event-sourcing-logstore-inmemory` — in-memory LogStore impl (Phase 2, placeholder now)
- `event-sourcing-logstore-sqlite` — SQLite LogStore impl (Phase 8, placeholder now)
- `event-sourcing-commands` — optional command layer (Phase 9, placeholder now)

**Recommended directory structure:**
```
event-sourcing/                     # workspace root
├── Cargo.toml                      # [workspace] + [workspace.dependencies]
├── event-sourcing/                  # core crate
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── event-sourcing-logstore-inmemory/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── event-sourcing-logstore-sqlite/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
└── event-sourcing-commands/
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

### Workspace Dependencies (D-17)

`[workspace.dependencies]` in root Cargo.toml centralizes version management:
- `serde = { version = "1.0.220", features = ["derive"] }` — required for ProjectionDefinition
- `serde_json = "1.0.149"` — required for event payloads (Value type)
- `thiserror = "2.0"` — error derive for AppendError, StoreError
- `futures-core = "0.3"` — for `Stream` trait in LogStore return types

Placeholder crates only need `event-sourcing` as a dependency initially.

### Crate Naming

Rust crate names use hyphens in Cargo.toml (`event-sourcing`) but underscores in code (`event_sourcing`). The `lib` name in Cargo.toml should match: `name = "event_sourcing"` etc.

---

## 2. Core Type Design

### StreamId (D-01, D-02, D-03)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(String);
```

- `StreamId::new()` validates non-empty — should return `Result<StreamId, InvalidStreamId>` for library ergonomics (panicking constructors are unfriendly in library code)
- No `From<&str>` — construction always explicit
- Users wrap in domain types (e.g., `struct OrderId(StreamId)`) for compile-time safety

### Sequence ID Types (D-05, D-06)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalSequenceId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamSequenceId(u64);
```

- Both start from 1; zero = "no events"
- `Copy` is appropriate — these are u64 wrappers
- `Ord` enables range queries and comparisons
- Consider providing `next()` method for incrementing

### StoredEvent (D-07, D-08, D-09)

```rust
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub global_sequence: GlobalSequenceId,
    pub stream_id: StreamId,
    pub stream_sequence: StreamSequenceId,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: std::time::SystemTime,
}
```

- `serde_json::Value` for payload — streams are heterogeneous (D-07)
- `event_type: String` enables filtering without deserialization (D-08)
- `SystemTime` set by store at append time, not by caller (D-09)
- No per-event UUID (D-04) — events identified by `(StreamId, StreamSequenceId)` or `GlobalSequenceId`

**serde derives:** `StoredEvent` should derive `Serialize, Deserialize` for testing and potential storage serialization, but this is at Claude's discretion.

---

## 3. Error Types (D-14, D-15)

### AppendError

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error("concurrency conflict: stream {stream_id} expected version {expected}, found {actual}")]
    ConcurrencyConflict {
        stream_id: StreamId,
        expected: StreamSequenceId,
        actual: StreamSequenceId,
    },

    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

- Two variants per D-14: `ConcurrencyConflict` and `StorageFailure`
- `StorageFailure` wraps a boxed error — implementations provide their own error types
- Uses `thiserror` for Display/Error derive

### StoreError

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage error: {0}")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

- Separate from AppendError per D-15
- Used for read operations (connection failures, mid-stream errors)

---

## 4. LogStore Trait (D-10, D-11, D-12, D-13)

### Design Decision: Static Dispatch (D-11)

Per CONTEXT.md D-11: native `async fn in trait` with static dispatch (`S: LogStore`). No `async-trait` dependency.

**However**, success criterion 4 says: "The `LogStore` trait is declared with `async-trait`, compiles against `dyn LogStore`"

This is a **conflict** between CONTEXT.md decisions and ROADMAP.md success criteria. The CONTEXT.md was written after the roadmap and represents the user's explicit decisions during discuss-phase:

- D-11 explicitly says: "Static dispatch via generics (`S: LogStore`). No `dyn LogStore` requirement, no `async-trait` dependency. Native `async fn in trait`."
- D-12 says: Associated type `type EventStream: Stream<Item = Result<StoredEvent, StoreError>>` for read methods, using `futures_core::Stream`.

**Recommendation:** Follow CONTEXT.md decisions (D-11) as they are the user's most recent explicit choices. The success criteria may need updating, but the planner should implement what the user decided.

### Trait Shape (D-10, D-12, D-13)

Five methods per D-10:

```rust
pub trait LogStore: Send + Sync {
    type EventStream: futures_core::Stream<Item = Result<StoredEvent, StoreError>>;

    async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, AppendError>;

    async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
        to: Option<StreamSequenceId>,
    ) -> Result<Self::EventStream, StoreError>;

    async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError>;

    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError>;

    async fn stream_version(
        &self,
        stream_id: &StreamId,
    ) -> Result<StreamSequenceId, StoreError>;
}
```

- `read_stream` and `read_all` return `Result<Self::EventStream, StoreError>` per D-13
- The associated type `EventStream` uses `futures_core::Stream` per D-12
- `NewEvent` is the pre-storage event type (no sequence IDs or timestamp yet)

### NewEvent Type

Events submitted for append don't have sequence IDs or timestamps yet:

```rust
pub struct NewEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}
```

### AppendCondition

```rust
pub enum AppendCondition {
    /// Stream must be at exactly this version
    ExpectedVersion(StreamSequenceId),
    /// Any version is acceptable (no concurrency check)
    Any,
}
```

---

## 5. Module Organization

Recommended internal module structure for the core crate:

```
event-sourcing/src/
├── lib.rs          # Re-exports public API
├── types.rs        # StreamId, GlobalSequenceId, StreamSequenceId, NewEvent
├── event.rs        # StoredEvent
├── error.rs        # AppendError, StoreError
├── store.rs        # LogStore trait, AppendCondition
```

This keeps each concern in its own file while keeping the public API flat via re-exports from `lib.rs`.

---

## 6. Placeholder Sibling Crates

Phase 1 only creates placeholder crates — they compile but have no logic:

- `event-sourcing-logstore-inmemory/src/lib.rs` — empty, depends on `event-sourcing`
- `event-sourcing-logstore-sqlite/src/lib.rs` — empty, depends on `event-sourcing`
- `event-sourcing-commands/src/lib.rs` — empty, depends on `event-sourcing`

Each has a minimal Cargo.toml with only the `event-sourcing` workspace dependency.

---

## 7. Testing Strategy

Success criterion 5: "In-memory unit tests can import core types from `event-sourcing` and the blank storage trait from the same crate"

Tests should live in `event-sourcing/tests/` (integration tests) or inline `#[cfg(test)]` modules:

- Test that `StreamId::new("")` fails
- Test that `StreamId::new("orders")` succeeds
- Test that `GlobalSequenceId` and `StreamSequenceId` are distinct types (compile-time)
- Test that `AppendError` variants can be matched
- Test that `StoredEvent` can be constructed with all fields
- Test that `LogStore` trait can be referenced as a generic bound

No async tests needed in Phase 1 — there's no I/O.

---

## 8. Dependency Versions

From CLAUDE.md recommended stack:
- `serde = { version = "^1.0.220", features = ["derive"] }`
- `serde_json = "^1.0.149"`
- `thiserror = "2.0"`
- `futures-core = "0.3"` — lightweight, only the `Stream` trait

**Not needed in Phase 1:**
- `tokio` — no async runtime needed (trait is defined, not executed)
- `async-trait` — not used per D-11
- `rusqlite` — Phase 8
- `uuid` — no per-event UUID per D-04

---

## 9. Key Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Success criteria conflict with CONTEXT.md on `async-trait` / `dyn LogStore` | MEDIUM | Follow CONTEXT.md (user's explicit decisions); note discrepancy in plan |
| `async fn in trait` without `async-trait` means no `dyn LogStore` | LOW | D-11 explicitly chose this; `dyn` support deferred per CONTEXT.md specifics section |
| `futures_core::Stream` associated type adds complexity | LOW | Well-established pattern; `futures-core` is a minimal dependency |
| Placeholder crates may have dependency version issues | LOW | Use `[workspace.dependencies]` to ensure consistency |

---

## Validation Architecture

### Requirement Coverage

| Req ID | What to Validate | How |
|--------|------------------|-----|
| STOR-01 | `LogStore` trait exists with all 5 methods | `grep` for trait definition, method signatures |
| LOG-01 | Events are immutable (no mutation methods on StoredEvent) | Check `StoredEvent` has no `&mut self` methods |
| LOG-02 | Per-stream sequence numbers exist | `StreamSequenceId` type exists, used in `StoredEvent` |
| LOG-03 | Global sequence ID exists, in event model | `GlobalSequenceId` type exists, used in `StoredEvent` |
| LOG-07 | Error types distinguish concurrency from storage | `AppendError` has both `ConcurrencyConflict` and `StorageFailure` variants |

### Compilation Checks

- `cargo build` succeeds for entire workspace
- `cargo test` passes (type construction tests)
- `cargo clippy` has no warnings

---

*Phase: 01-workspace-setup-and-core-types*
*Research completed: 2026-04-05*
