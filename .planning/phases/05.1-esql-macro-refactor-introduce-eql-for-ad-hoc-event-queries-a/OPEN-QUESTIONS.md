# Phase 5.2 (ESQL Macro Refactor) — Open Questions

## OQ-01: Rename `query` keyword to `from`?

**Raised:** 2026-04-26

The primary stream declaration in the `projection!` DSL currently uses `query`:

```
projection CustomerView {
    query tag.starts_with("customer:") as c
    ...
}
```

**Question:** Should this be renamed to `from` for SQL alignment?

```
projection CustomerView {
    from tag.starts_with("customer:") as c
    ...
}
```

**Arguments for `from`:**
- More SQL-like — aligns with the SELECT/FROM/WHERE mental model
- `from` is directional ("reading from this stream") — arguably more readable

**Arguments against:**
- Breaking DSL change — all existing projections need updating
- `query` is already established in Phase 4/4.1 and used in `ProjectionDefinition.query` (the JSON field)
- Changing the DSL keyword doesn't change the JSON field name — creates a DSL/JSON mismatch unless the JSON field is also renamed

**Decision needed before:** Phase 5.2 planning. If renamed, both the DSL keyword and the `ProjectionDefinition.query` JSON field name should change together to stay consistent.
