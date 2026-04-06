---
phase: 02-in-memory-log-store
plan: 01
subsystem: logstore-inmemory
tags: [in-memory, logstore, storage, testing]
dependency_graph:
  requires: []
  provides: [InMemoryLogStore, LogStore trait, core types]
  affects: [all future phases that need a LogStore]
tech_stack:
  added: [tokio sync, futures stream]
  patterns: [Arc<RwLock<T>> for shared mutable state, native async fn in trait, write lock for atomic append]
key_files:
  created:
    - Cargo.toml
    - event-sourcing/Cargo.toml
    - event-sourcing/src/lib.rs
    - event-sourcing/src/types.rs
    - event-sourcing/src/event.rs
    - event-sourcing/src/error.rs
    - event-sourcing/src/store.rs
    - event-sourcing-logstore-inmemory/Cargo.toml
    - event-sourcing-logstore-inmemory/src/lib.rs
    - event-sourcing-logstore-sqlite/Cargo.toml
    - event-sourcing-logstore-sqlite/src/lib.rs
    - event-sourcing-commands/Cargo.toml
    - event-sourcing-commands/src/lib.rs
  modified: []
key_decisions:
  - "Append acquires write lock first, then allocates global sequence IDs under the lock — prevents sequence gaps on ConcurrencyConflict"
  - "Native async fn in trait (-> impl Future + Send) used instead of async-trait macro — supports static dispatch, not dyn dispatch"
  - "Empty stream returned for unknown streams (not StoreError) — per D-04 design decision"
  - "next_global_seq stored inside InnerState, not as separate AtomicU64 — simpler and avoids gaps"
metrics:
  duration_seconds: 249
  completed_date: "2026-04-06"
  tasks_completed: 2
  files_created: 13
  files_modified: 0
requirements_satisfied: [STOR-02]
---

# Phase 02 Plan 01: InMemoryLogStore Summary

**One-liner:** In-memory LogStore with Arc<RwLock> backing, 5-method trait impl, and 17-test suite covering append, range reads, global ordering, clone sharing, and optimistic concurrency.

## What Was Built

Implemented the `event-sourcing-logstore-inmemory` crate as the reference LogStore implementation and test harness for all subsequent phases.

This plan also bootstrapped the entire workspace (deviation from Phase 2 plan scope — Phase 1 prerequisites were absent):
- Workspace `Cargo.toml` with 4 members and centralized `[workspace.dependencies]`
- Core `event-sourcing` crate with all types, errors, and the `LogStore` trait
- Placeholder `event-sourcing-logstore-sqlite` and `event-sourcing-commands` crates

The `InMemoryLogStore` implementation:
- `Arc<RwLock<InnerState>>` backing — `Clone` shares the same state
- Append acquires write lock before sequence allocation — no gaps on version conflict
- `current_sequence` and `stream_version` return ZERO for empty state
- `read_stream` on unknown stream returns `Ok(empty)` not `Err`
- `read_all` sorts by global sequence across all streams

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| Task 1 + Task 2 (combined) | `3d18e0f` | feat(02-01): implement InMemoryLogStore with full LogStore trait |

## Verification Results

| Check | Result |
|-------|--------|
| `cargo build -p event-sourcing-logstore-inmemory` | PASS |
| `cargo test -p event-sourcing-logstore-inmemory` | PASS — 17/17 |
| `cargo test --workspace` | PASS — 30/30 |
| `cargo clippy --workspace` | PASS — 0 warnings |
| InMemoryLogStore implements all 5 LogStore methods | Yes |
| No async-trait dependency | Yes |
| Clone shares state (Arc-backed) | Yes |
| Empty stream for unknown reads | Yes |
| Global sequence monotonically increasing | Yes |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Phase 1 workspace + core types prerequisite was absent**
- **Found during:** Task 1 startup — `event-sourcing/src/store.rs` did not exist
- **Issue:** Phase 1 plans (01-01, 01-02) were written but never executed; workspace had no Rust code
- **Fix:** Implemented Phase 1 content inline (workspace Cargo.toml, core crate types, LogStore trait, placeholder sibling crates) before implementing Phase 2
- **Files modified:** All `Cargo.toml` files, `event-sourcing/src/*.rs`
- **Commit:** `3d18e0f`

**2. [Rule 1 - Bug] Borrow conflict in append loop (mutable + immutable borrow of InnerState)**
- **Found during:** Task 1 compilation
- **Issue:** `state.streams.entry(...).or_default()` held a mutable borrow; accessing `state.next_global_seq` inside the loop caused E0502/E0499
- **Fix:** Compute all stored events into a local Vec first (reading state fields), then extend the stream vec after the loop
- **Files modified:** `event-sourcing-logstore-inmemory/src/lib.rs`
- **Commit:** `3d18e0f`

**3. [Rule 1 - Bug] Clippy warning: `map_or(true, ...)` should use `is_none_or`**
- **Found during:** Task 2 clippy check
- **Issue:** `to.map_or(true, |end| e.stream_sequence <= end)` flagged by clippy::unnecessary_map_or
- **Fix:** Changed to `to.is_none_or(|end| e.stream_sequence <= end)`
- **Files modified:** `event-sourcing-logstore-inmemory/src/lib.rs`
- **Commit:** `3d18e0f`

## Known Stubs

None — all 5 LogStore methods are fully implemented and tested.

## Self-Check: PASSED
