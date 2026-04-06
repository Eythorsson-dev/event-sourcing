---
phase: 2
slug: in-memory-log-store
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-04-05
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test / cargo nextest |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p event-sourcing-logstore-inmemory` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p event-sourcing-logstore-inmemory`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 0 | STOR-02 | unit | `cargo build -p event-sourcing-logstore-inmemory` | W0 | pending |
| 02-01-02 | 01 | 1 | STOR-02 | unit | `cargo test -p event-sourcing-logstore-inmemory test_append_and_read` | yes | pending |
| 02-01-03 | 01 | 1 | STOR-02 | unit | `cargo test -p event-sourcing-logstore-inmemory test_read_stream_range` | yes | pending |
| 02-01-04 | 01 | 1 | STOR-02 | unit | `cargo test -p event-sourcing-logstore-inmemory test_global_sequence` | yes | pending |
| 02-01-05 | 01 | 1 | STOR-02 | unit | `cargo test -p event-sourcing-logstore-inmemory test_append_expected_version_conflict` | yes | pending |

---

## Wave 0 Requirements

- [ ] `event-sourcing-logstore-inmemory/src/lib.rs` — crate scaffold with `InMemoryLogStore` implementing `LogStore`
- [ ] `event-sourcing-logstore-inmemory/Cargo.toml` — crate manifest with correct dependencies

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 10s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** satisfied
