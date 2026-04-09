---
title: KeyedTag type and TagFilter convenience constructors
area: tag-model
created: 2026-04-09
status: pending
---

# KeyedTag type and TagFilter helper methods

A `KeyedTag` struct formalizing the `key:value` tag convention, plus convenience constructors on `TagFilter` that build the correct tag string internally.

## Concept

```rust
// KeyedTag: key:value convenience constructor
let tag = KeyedTag::new("order", "o1")?.into(); // → Tag("order:o1")

// TagFilter helpers that accept a key rather than a raw prefix string:
TagFilter::key_equals("order", "o1")    // → Equals(Tag("order:o1"))
TagFilter::key_prefix("order")           // → StartsWith("order:")
```

## Appeal

- Callers no longer manually format `"order:"` prefix strings with separator
- `key_prefix("order")` is harder to misuse than `StartsWith("order:")` (separator included automatically)
- Observer routing (Phase 7) benefits most: extract instance ID from `KeyedTag` rather than string-splitting

## Why Deferred

`TagFilter` with raw `StartsWith(String)` is sufficient for Phase 03.2 and Phase 4. The ergonomic helpers are a nice-to-have once the broader API is stable. Introducing `KeyedTag` now would add a type before it's clear how widely it will be used.

## References

Raised during Phase 04 discuss-phase session 2026-04-09.
