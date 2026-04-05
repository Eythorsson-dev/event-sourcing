# Phase 1: Workspace Setup and Core Types - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-05
**Phase:** 01-workspace-setup-and-core-types
**Areas discussed:** Stream identity, Event type safety, LogStore trait surface, Crate wiring

---

## Stream Identity

### Stream ID type

| Option | Description | Selected |
|--------|-------------|----------|
| String newtype | StreamId(String) — flexible, callers construct from any naming scheme | ✓ |
| Typed composite | StreamId { category, id } — enforces structure but adds ceremony | |
| Plain String | No wrapper, no type safety | |

**User's choice:** String newtype
**Notes:** None

### Construction style

| Option | Description | Selected |
|--------|-------------|----------|
| From/Into + Display | Minimal friction, "orders-123".into() works | |
| Explicit constructor only | StreamId::new() — clear intent at every call site | ✓ |

**User's choice:** Explicit constructor only
**Notes:** None

### Sequence number representation

| Option | Description | Selected |
|--------|-------------|----------|
| Newtype u64 wrappers | GlobalSequenceId(u64) and StreamSequenceId(u64) — separate types prevent mixing | ✓ |
| Single SequenceId type | One type for both — simpler but no compile-time distinction | |
| Raw u64 | No wrappers, zero safety | |

**User's choice:** Newtype u64 wrappers
**Notes:** None

### Format constraints

| Option | Description | Selected |
|--------|-------------|----------|
| Non-empty only | StreamId::new("") returns error or panics, no other constraints | ✓ |
| No validation | Accept any String, even empty | |
| Strict format | Alphanumeric + dashes, max length | |

**User's choice:** Non-empty only
**Notes:** None

### GlobalSequenceId starting value

| Option | Description | Selected |
|--------|-------------|----------|
| Start from 1 | First event gets 1, 0 means "no events seen" | ✓ |
| Start from 0 | Standard array-style indexing | |

**User's choice:** Start from 1
**Notes:** None

### Per-event unique ID

| Option | Description | Selected |
|--------|-------------|----------|
| No event ID | Events identified by sequence numbers — no UUID overhead | ✓ |
| UUID v7 event ID | Useful for external references but adds dependency | |

**User's choice:** No event ID
**Notes:** None

### Timestamp

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, stored at append time | Useful for debugging, auditing, time-based queries | ✓ |
| No timestamp | Minimal StoredEvent, callers put timestamps in payloads | |
| Optional timestamp | Option<Timestamp>, stores can set it or not | |

**User's choice:** Yes, stored at append time
**Notes:** None

### Timestamp type

| Option | Description | Selected |
|--------|-------------|----------|
| chrono::DateTime<Utc> | Rich formatting, adds chrono dependency | |
| std::time::SystemTime | Zero dependencies, less ergonomic for display | ✓ |
| i64 millis since epoch | Primitive, no dependencies, loses type safety | |

**User's choice:** std::time::SystemTime
**Notes:** None

### Event type metadata

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, required | event_type: String on every StoredEvent, enables filtering without deserialization | ✓ |
| No, derive from payload | Event type only known after deserialization | |

**User's choice:** Yes, required
**Notes:** None

### Stream category type safety

Extended discussion about whether the library should enforce stream-event type safety at compile time. User wanted compile-time guarantee that e.g. UserRegistered can't be appended to an order stream. Three options explored:

1. **Option A: Single StreamId** — no compile-time distinction between stream categories
2. **Option B: Generic StreamId<T: StreamType>** — compile-time safety but generics infect entire API
3. **Option C: User enum with Into<StreamId>** — user concern about enum growing large

**Resolution:** Option A with the understanding that (1) constraints (Phase 6) enforce this at runtime, (2) users wrap StreamId in domain types for compile-time safety in their code, (3) can revisit later since StreamId is a newtype.

---

## Event Type Safety

### Payload type

| Option | Description | Selected |
|--------|-------------|----------|
| serde_json::Value | Opaque JSON, library never deserializes into user types | ✓ |
| Generic T: Serialize + DeserializeOwned | StoredEvent<T>, full compile-time safety | |
| Vec<u8> raw bytes | Format-agnostic, maximum flexibility | |

**User's choice:** serde_json::Value
**Notes:** User preferred compile-time safety but accepted that streams are heterogeneous (multiple event types per stream) making a single generic T impractical. Type safety lives in the deserialization/projection layer.

---

## LogStore Trait Surface

### Trait methods

Discussion established five methods: append, read_stream, read_all, current_sequence, stream_version. User agreed current_sequence and stream_version are worth including despite increasing the implementor burden.

### Return type for reads

Extended discussion about async iterators vs Vec:
1. Started with async `Stream` recommendation
2. User proposed `AnyLogStore` with `Box<dyn Any>` to avoid `Pin<Box<dyn Stream>>` — explored but identified that `dyn Any` downcast requires knowing concrete types
3. Concluded that `dyn LogStore` isn't needed — static dispatch via generics suffices
4. This unlocked associated types on the trait, removing need for `async-trait`

**Resolution:** Associated type `EventStream: Stream<Item = Result<StoredEvent, StoreError>>`, static dispatch, no `async-trait`.

### Error handling on reads

| Option | Description | Selected |
|--------|-------------|----------|
| Stream<Item = Result<...>> | Per-item errors, standard pattern in Rust async DB access | ✓ |
| Stream<Item = StoredEvent> | No recovery path on mid-stream errors | |
| Vec return, no streaming | No mid-stream errors but loses streaming benefit | |

**User's choice:** Result<EventStream, StoreError> with Stream<Item = Result<StoredEvent, StoreError>>
**Notes:** Separates "couldn't start reading" from "failed mid-read"

---

## Crate Wiring

### In-memory store location

| Option | Description | Selected |
|--------|-------------|----------|
| Separate crate | Core defines contracts only, zero implementations | ✓ |
| In core crate | Users get test store for free | |

**User's choice:** Separate crate
**Notes:** Per recommendation — core stays pure contracts

### Workspace dependencies

Shared via `[workspace.dependencies]` — confirmed without objection.

### Re-exports

| Option | Description | Selected |
|--------|-------------|----------|
| No re-exports | Explicit imports from each crate | ✓ |
| Re-export from core | event_sourcing::InMemoryLogStore | |

**User's choice:** No re-exports
**Notes:** None

---

## Claude's Discretion

- Internal module structure within core crate
- Exact derive traits on newtypes
- Whether StreamId::new() returns Result or panics on empty

## Deferred Ideas

None — discussion stayed within phase scope.
