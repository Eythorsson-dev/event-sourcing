---
title: Rename cleared_by to removed_by on object fields for DSL consistency
area: projection-dsl
suggested_phase: 5
created: 2026-04-26
---

# Rename `cleared_by` → `removed_by` on Object Fields

## Problem

List items use `removed_by` to signal item removal. Object fields use `cleared_by` to signal nulling the object. Two different keywords for semantically equivalent operations creates inconsistency in the DSL.

## Proposed Change

Rename `cleared_by` → `removed_by` everywhere:

- `ObjectFieldSpec.cleared_by` field → `ObjectFieldSpec.removed_by`
- `ProjectionDefinition` JSON schema: `"cleared_by"` key → `"removed_by"`
- `projection!` macro parser: `cleared_by` keyword → `removed_by`
- All tests that use `cleared_by` syntax
- Documentation and examples

## Context

Decided during Phase 4.2 discuss-phase (2026-04-26). List fields (Phase 5.1) will use `removed_by` for item removal. Objects should use the same keyword for clearing — the operations are semantically equivalent (trigger event → set to null).

## Scope

Breaking change to JSON schema and DSL syntax. Should be done before or during Phase 5 to avoid migrating tests twice.
