<!-- GSD:project-start source:PROJECT.md -->
## Project

**Event Sourcing**

An unopinionated Rust event sourcing library built around a constrained event log and a projection engine. Instead of aggregates, it uses dynamic consistency boundaries with read models, optimistic concurrency, and sequence-ID-based consistency guarantees. Designed as a workspace of crates — core library, storage trait implementations, and an optional command layer.

**Core Value:** The projection engine is the heart — it powers read models, validates constraints, and enables multi-stream joins, all from a single declarative definition that serializes to JSON.

### Constraints

- **Language**: Rust — library crate, not a binary
- **Type Safety**: Strong compile-time type safety for events, projections, and constraints
- **Unopinionated**: No forced patterns — users compose building blocks as they see fit
- **Crate Separation**: Storage implementations and commands are separate crates, not features
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### Core Technologies
| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| serde | 1.0 (^1.0.220) | Serialize/deserialize events and `ProjectionDefinition` to/from JSON | De facto standard in the Rust ecosystem; zero-cost derive macros; every database and transport crate speaks serde; no realistic alternative |
| serde_json | 1.0 (^1.0.149) | JSON encoding for `ProjectionDefinition` and event payloads | Only production-grade JSON crate for Rust; backed by serde-rs team; version 1.0.149 as of Jan 2026 |
| tokio | 1.x (^1.48) | Async runtime for async storage trait impls and the SQLite logstore | Dominant runtime; async-std was officially discontinued March 2025; 20k+ crates depend on tokio; LTS branch 1.47.x supported until Sep 2026 |
| thiserror | 2.0 | Define structured, matchable error types for library consumers | Library crates must expose typed errors callers can `match` on; thiserror eliminates boilerplate without hiding structure; anyhow is for applications, not libraries |
| rusqlite | 0.38 | SQLite access in `logstore-sqlite` crate | Lightweight synchronous wrapper; `bundled` feature compiles SQLite into the binary (zero system dependency); purpose-built for SQLite only — appropriate for the sqlite crate |
### Supporting Libraries
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio-rusqlite | 0.5 | Async wrapper for rusqlite | Required in `logstore-sqlite` to expose async storage trait without blocking the tokio executor; spawns rusqlite calls onto a dedicated thread |
| rusqlite_migration | 2.x | SQLite schema migrations | Use in `logstore-sqlite` to manage the events table schema; uses SQLite `user_version` (lighter than migration tables); no external CLI needed |
| async-trait | 0.1 | `#[async_trait]` attribute for `dyn`-compatible async traits | Required whenever the storage abstraction trait needs `dyn Trait` polymorphism; native `async fn in trait` (stable since Rust 1.75) cannot yet be used as `dyn Trait` — async_trait fills this gap until Rust stabilizes async dyn dispatch |
| syn | 2.x | Parse Rust token streams in proc macros | Only needed if projection definitions are exposed via a derive macro; syn 2.x is a full rewrite with improved error messages and `syn::Error` support |
| quote | 1.0 | Generate Rust code from proc macro token streams | Companion to syn; needed only if a derive macro is built for `ProjectionDefinition` |
| proc-macro2 | 1.0 | Token stream abstraction for proc macros | Foundation for syn and quote; required for any proc-macro crate |
| uuid | 1.x | Generate event and stream IDs | Use `v7` (time-ordered, sortable) for event IDs so they sort lexicographically in the log; use `v4` only if order is irrelevant |
### Development Tools
| Tool | Purpose | Notes |
|------|---------|-------|
| cargo test | Unit and integration testing | Use `#[tokio::test]` for async tests; tokio's test macro handles runtime setup |
| cargo clippy | Lint enforcement | Enforce as CI gate; catches common API design errors, unnecessary allocations |
| cargo fmt | Formatting | Use `rustfmt.toml` at workspace root; share config across all crates |
| cargo nextest | Faster test runner | Optional but recommended; parallel-by-default, better output for workspaces |
| proptest | Property-based testing | Useful for testing projection engine correctness with random event sequences; supports async tokio executor |
## Installation
# Workspace root Cargo.toml — [workspace.dependencies]
# In event-sourcing-logstore-sqlite/Cargo.toml
# In event-sourcing-macros/Cargo.toml (if proc-macro crate is added)
## Alternatives Considered
| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| tokio | smol | If minimal runtime footprint is paramount and the library targets embedded/constrained environments; smol is the closest viable alternative after async-std's discontinuation |
| rusqlite + tokio-rusqlite | sqlx (SQLite driver) | If users want compile-time checked queries or need a single crate supporting multiple databases; sqlx and rusqlite cannot coexist in one dep graph (native lib conflict), so the logstore-sqlite crate must commit to one |
| thiserror | snafu | snafu offers richer context-attach ergonomics and location tracking; appropriate if the error API grows very large; thiserror is simpler and adequate for this library's scope |
| async-trait macro | native async fn in trait | Usable today for static dispatch (`impl Trait`) where `dyn` is not required; drop async-trait from any trait that never needs object-safe dynamic dispatch |
| uuid v7 | ulid | ulid is sortable and human-readable but less standardized; uuid v7 achieves the same time-ordering with the well-supported `uuid` crate; the `ulid` crate has serde integration but lower adoption |
| serde_json Value | custom enum | For the internal `ProjectionDefinition` representation, a typed Rust enum is preferable over `serde_json::Value`; `Value` is only appropriate for the raw event payload field stored verbatim |
## What NOT to Use
| Avoid | Why | Use Instead |
|-------|-----|-------------|
| async-std | Officially discontinued March 2025; no longer maintained | tokio (dominant) or smol (lightweight) |
| anyhow | anyhow erases error types — library consumers cannot match on the error variant; it belongs in application code | thiserror for all library error types |
| Diesel | ORM-level abstraction is wrong for an event log; Diesel's schema macro fights against the append-only log model; requires a separate migration CLI | rusqlite with raw SQL for the SQLite logstore |
| SeaORM | Same ORM mismatch as Diesel; heavy async ORM adds unnecessary abstraction over what is fundamentally a sequential append log | rusqlite + tokio-rusqlite |
| sqlx in the same crate as rusqlite | Both link the same native SQLite library; Cargo rejects duplicate native library linkers in the same dep graph — this is a hard incompatibility | Choose one; logstore-sqlite uses rusqlite; users who want sqlx write their own logstore impl against the storage trait |
| syn 1.x | syn 2.x is a breaking rewrite with improved error reporting and a different API; mixing 1.x and 2.x causes compile failures in proc-macro crates | syn 2.x |
| Tokio features = ["full"] in a library | Pulls in every tokio subsystem (net, process, signal, fs) as mandatory deps, making the library much heavier than needed | Specify only needed features: `rt`, `rt-multi-thread`, `macros`, `sync` |
## Stack Patterns by Variant
- Depend on tokio only as an optional dev-dependency (for tests), not a required dependency
- Use `async fn` in traits with native stable syntax where static dispatch suffices
- Use `#[async_trait]` only on the storage trait if `dyn LogStore` is needed at runtime
- Keep serde as a required dep (Serialize/Deserialize are core to the ProjectionDefinition contract)
- No database dep; use `std::sync::RwLock` or `tokio::sync::RwLock` for the in-memory store
- Tokio is a full dependency here (the impl must be async)
- Prefer `tokio::sync::RwLock` over `std::sync::RwLock` to avoid blocking the async executor on contended reads
- rusqlite 0.38 with `bundled` feature — eliminates system SQLite version mismatch issues
- tokio-rusqlite wraps every rusqlite call in `spawn_blocking` transparently
- rusqlite_migration for schema management — use embedded SQL strings, not external files
- Enable WAL mode (`PRAGMA journal_mode=WAL`) at connection open time for concurrent reads alongside appends
- Must be its own crate with `proc-macro = true` in Cargo.toml — Rust requires proc-macro crates to be separate
- Convention: name it `event-sourcing-derive` or `event-sourcing-macros`
- Re-export the derive macros from the main `event-sourcing` crate behind a `macros` feature flag so users do not need to add the derive crate directly
## Version Compatibility
| Package | Compatible With | Notes |
|---------|-----------------|-------|
| rusqlite 0.38 | libsqlite3-sys 0.32 | The `bundled` feature removes the system lib requirement entirely |
| tokio-rusqlite 0.5 | tokio 1.x, rusqlite 0.31+ | Confirm exact rusqlite version pin in tokio-rusqlite's Cargo.toml before locking |
| rusqlite 0.38 | sqlx (any) | INCOMPATIBLE — both link `libsqlite3-sys`; do not use in the same crate or workspace binary |
| async-trait 0.1 | tokio 1.x | Compatible; async-trait is runtime-agnostic |
| serde 1.0.220+ | serde_json 1.0.149 | serde_json 1.0.149 requires serde >=1.0.220 |
| syn 2.x | proc-macro2 1.x, quote 1.x | syn 2.x depends on proc-macro2 ^1.0.67 and quote ^1.0.28; all resolve automatically |
## Sources
- [serde-rs/serde — GitHub](https://github.com/serde-rs/serde) — version confirmed HIGH confidence
- [serde_json 1.0.149 — docs.rs](https://docs.rs/crate/serde_json/latest) — version confirmed HIGH confidence
- [tokio 1.48 — docs.rs](https://docs.rs/crate/tokio/latest/source/README.md) — latest 1.x series confirmed HIGH confidence
- [Choosing Your Async Champion: Tokio vs async-std in 2025 — Medium/Rustaceans](https://medium.com/rustaceans/choosing-your-async-champion-tokio-vs-async-std-in-2025-a142d3899b66) — async-std discontinuation MEDIUM confidence (corroborated by multiple sources)
- [The State of Async Rust: Runtimes — corrode.dev](https://corrode.dev/blog/async/) — runtime landscape MEDIUM confidence
- [rusqlite 0.38.0 — crates.io](https://crates.io/crates/rusqlite/) — version confirmed HIGH confidence
- [tokio-rusqlite — lib.rs](https://lib.rs/crates/tokio-rusqlite) — async wrapper pattern confirmed MEDIUM confidence
- [rusqlite_migration 2.4.1 — docs.rs](https://docs.rs/crate/rusqlite_migration/latest) — migration library confirmed MEDIUM confidence
- [Announcing async fn and RPITIT in traits — Rust Blog](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits/) — dyn limitation confirmed HIGH confidence (official source)
- [async-trait — crates.io](https://crates.io/crates/async-trait) — still needed for dyn dispatch HIGH confidence
- [Rust Error Handling: thiserror vs anyhow — DEV Community](https://dev.to/leapcell/rust-error-handling-compared-anyhow-vs-thiserror-vs-snafu-2003) — library vs application error pattern MEDIUM confidence
- [Rust ORMs in 2026 — Medium](https://aarambhdevhub.medium.com/rust-orms-in-2026-diesel-vs-sqlx-vs-seaorm-vs-rusqlite-which-one-should-you-actually-use-706d0fe912f3) — rusqlite/sqlx comparison MEDIUM confidence
- [sqlx/rusqlite semver hazard — GitHub Discussion](https://github.com/launchbadge/sqlx/discussions/3295) — native lib conflict confirmed HIGH confidence
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.

## README

README.md is written for curious developers who want to know what this library is, the problem it solves, and why they should care. Keep it focused on motivation and value — not implementation details.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
