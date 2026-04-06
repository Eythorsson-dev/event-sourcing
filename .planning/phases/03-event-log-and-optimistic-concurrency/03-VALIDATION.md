---
phase: 3
slug: event-log-and-optimistic-concurrency
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-06
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test / tokio::test |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p event-sourcing` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p event-sourcing`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 3-01-01 | 01 | 0 | LOG-04 | build | `cargo build -p event-sourcing` | ❌ W0 | ⬜ pending |
| 3-01-02 | 01 | 1 | LOG-04 | unit | `cargo test -p event-sourcing append_with_condition` | ❌ W0 | ⬜ pending |
| 3-01-03 | 01 | 1 | LOG-04 | unit | `cargo test -p event-sourcing concurrency_conflict` | ❌ W0 | ⬜ pending |
| 3-02-01 | 02 | 1 | LOG-05 | unit | `cargo test -p event-sourcing returns_sequence_id` | ❌ W0 | ⬜ pending |
| 3-03-01 | 03 | 1 | LOG-06 | unit | `cargo test -p event-sourcing read_stream` | ❌ W0 | ⬜ pending |
| 3-03-02 | 03 | 1 | LOG-06 | unit | `cargo test -p event-sourcing read_stream_range` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `event-sourcing/src/log.rs` — `EventLog<S>` struct with `append` and `read` signatures
- [ ] `event-sourcing/src/error.rs` — `EventLogError` type with `ConcurrencyConflict` variant
- [ ] `event-sourcing/Cargo.toml` — `[dev-dependencies]` block with `tokio`, `event-sourcing-logstore-inmemory`, `futures`, `serde_json`
- [ ] `event-sourcing/src/lib.rs` — pub mod declarations for new modules

*Wave 0 must compile before Wave 1 tests can run.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Two concurrent appends — exactly one success | LOG-04 | Race window requires real concurrency; timing-sensitive | Spawn two tokio tasks appending simultaneously; assert one Ok, one ConcurrencyConflict |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
