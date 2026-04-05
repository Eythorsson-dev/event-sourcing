---
phase: 1
slug: workspace-setup-and-core-types
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-05
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in Rust test framework) |
| **Config file** | none — Cargo.toml handles test configuration |
| **Quick run command** | `cargo test -p event-sourcing` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p event-sourcing`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | STOR-01 | — | N/A | compile | `cargo build --workspace` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | LOG-01, LOG-02, LOG-03 | — | N/A | unit | `cargo test -p event-sourcing` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | LOG-07 | — | N/A | unit | `cargo test -p event-sourcing` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `event-sourcing/tests/types_test.rs` — stubs for StreamId, sequence IDs, StoredEvent
- [ ] `event-sourcing/tests/errors_test.rs` — stubs for AppendError, StoreError matching

*Existing infrastructure covers all phase requirements after Wave 0.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| LogStore trait compiles with generic bounds | STOR-01 | Compile-time check | `cargo build -p event-sourcing` must succeed |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
