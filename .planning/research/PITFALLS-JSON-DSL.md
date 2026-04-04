# Pitfalls: JSON DSL Design for Declarative Projection Definitions

**Domain:** Event sourcing projection engine, Rust library
**Researched:** 2026-04-04
**Scope:** JSON-serializable DSL for event projection definitions — expressiveness boundary, type safety, versioning, performance

---

## 1. The Expressiveness Creep Trap (Critical)

**What goes wrong:** The DSL starts simple — field mappings, filters, folds. Then a user needs a conditional. Then string formatting. Then arithmetic. Each addition seems reasonable in isolation. The result is a half-baked programming language with no standard library, no debugger, and no test framework. JSONata is the canonical example: it supports variables, lambups, recursion, and higher-order functions. Users regularly write expressions that no one else can read or maintain.

**Why it happens:** Every user request for expressiveness is individually legitimate. There is no natural stopping point that is visible from inside the design process. The Configuration Complexity Clock describes this exactly: managing configuration morphs into a full-blown DSL, which eventually becomes more cumbersome than the code it replaced.

**How to avoid:**
- Define the expressiveness boundary before writing the first operation: "projection definitions describe structure and routing, not logic." Logic belongs in Rust code that processes events before they reach the projection engine.
- The safe side of the boundary: field selection/mapping, type coercion (string → int), basic aggregation (sum, count, last), conditional inclusion of fields based on event type discriminant, nested object and list shapes.
- The unsafe side (keep out): string manipulation expressions, arithmetic beyond simple accumulation, branching on field values (not event types), user-defined functions, loops.
- Add an explicit `custom_handler` escape hatch from the start — a reference to a named Rust function — so users with genuinely complex needs have a clean path that does not require adding features to the DSL. Without this, pressure to extend the DSL is relentless.

---

## 2. Silent Data Loss on Type Mismatch (Critical)

**What goes wrong:** The projection definition says a field should be an integer. The event arrives with that field as a string (or absent). In a permissive deserializer this silently produces a wrong read model — the field is zero, or null, or missing. Users discover the corruption hours or days later when they query the projection.

**Why it happens:** JSON has no enforced schema. The definition and the event data are both JSON, but there is no static guarantee that the event's actual shape matches what the definition expects. Rust's serde is strict at struct deserialization boundaries but `serde_json::Value` is not — anything parses. A dynamically-interpreted DSL that walks `Value` trees will silently coerce or drop mismatched fields.

**How to avoid:**
- Validate projection definitions against a known event schema at registration time, not at projection execution time. If the event type is known to the Rust type system, use it.
- At execution time, treat every field access as a `Result`. Emit an explicit error (not a silent default) when a field is absent or has the wrong type. Propagate this as a projection execution error, not a silent skip.
- Do not use `serde_json::Value::as_str()` / `as_i64()` patterns without checking the return is `Some`. These return `None` on type mismatch, which is invisible to callers that unwrap carelessly.
- Consider a narrow runtime type system in the projection DSL itself — each field mapping declares an expected type, and the engine validates on every event. This surfaces modeling errors at development time, not in production.

---

## 3. Serde Unknown Fields and Forward Compatibility (Moderate)

**What goes wrong:** `#[serde(deny_unknown_fields)]` on `ProjectionDefinition` means that adding a new optional field to the struct in a library update breaks deserialization of all definitions stored with the old schema. Conversely, omitting `deny_unknown_fields` means a typo in a field name silently produces a definition missing that configuration — no error, just wrong behavior.

**Why it happens:** Serde's default behavior for structs is to ignore unknown fields, which is forward-compatible but masks typos. `deny_unknown_fields` catches typos but breaks forward compatibility. These goals conflict directly.

**How to avoid:**
- Do not use `deny_unknown_fields` on `ProjectionDefinition` or any of its nested types if stored definitions will outlive a single binary version.
- Instead, implement a post-deserialization validation step that checks for coherence (required fields present, no contradictory settings). This catches the class of errors that `deny_unknown_fields` would catch, without breaking forward compatibility.
- Known serde interaction pitfall: `deny_unknown_fields` is incompatible with `#[serde(flatten)]`. If `ProjectionDefinition` uses flattened enums or structs internally, applying `deny_unknown_fields` at any level that contains them will produce spurious errors even on valid input.
- Use `#[serde(default)]` on all optional fields so new fields added to the struct deserialize correctly from old stored JSON without error.

---

## 4. Stored Definitions Become Frozen at Their Schema Version (Critical)

**What goes wrong:** `ProjectionDefinition` is serialized and stored (in a database, config file, or migration). The library ships a new version that renames a field, changes its type, or restructures a nested object. All previously stored definitions are now invalid. Either deserialization silently produces wrong results, or it hard-fails and the system cannot start.

**Why it happens:** JSON has no built-in versioning. The same bytes can be interpreted differently by different versions of the deserializer. Unlike event data (which in event sourcing is explicitly immutable and versioned), the projection definition feels like "just configuration" — versioning is an afterthought.

**How to avoid:**
- Add a `schema_version: u32` field to `ProjectionDefinition` from the start, even if v1 is the only version. This is the only reliable way to implement upcasting later.
- Write upcasters as pure functions: `fn upcast_v1_to_v2(v: serde_json::Value) -> serde_json::Value`. Deserialize as `Value` first, run the appropriate upcaster chain, then deserialize to the target struct. This is the same pattern used for event upcasting in event sourcing systems (Marten, EventStoreDB).
- Treat any rename or structural change to `ProjectionDefinition` as a breaking change requiring a new schema version. Additive changes (new optional fields with defaults) are non-breaking.
- Never store raw `ProjectionDefinition` structs without the version wrapper. Define a `StoredProjectionDefinition { schema_version: u32, definition: serde_json::Value }` as the persistent envelope.

---

## 5. Projection Rebuild Cost When Definitions Change (Moderate)

**What goes wrong:** A user changes a projection definition — adds a field, changes an aggregation. Because the existing read model was built under the old definition, it cannot be trusted. The entire event history must be replayed. For large event logs this takes hours. During the replay, the read model is stale or unavailable.

**Why it happens:** Unlike schema migrations in relational databases (which can be applied to existing data), a projection definition change means the new interpretation must be applied from event zero. This is fundamental to event sourcing — you cannot partially replay.

**How to avoid:**
- Make projection definitions immutable once active. A "changed" definition is actually a new projection. The library should support running two projections simultaneously during a blue-green cutover.
- Store a content hash of the `ProjectionDefinition` alongside the read model. On startup, compare the current definition hash to the stored hash. If they differ, flag the projection as stale rather than silently serving an inconsistent read model.
- This is especially relevant for this library's constraint projections: a constraint backed by a stale projection may silently fail to enforce invariants, which is worse than returning an error.
- Design the inline catch-up feature with this in mind: catch-up to a sequence ID is only valid for the current definition. A definition change invalidates all existing sequence-ID-based consistency guarantees.

---

## 6. Interpreter Performance on the Hot Path (Moderate)

**What goes wrong:** Every event append triggers one or more projection observers. Each observer interprets the JSON projection definition to determine what to do with the event. If the event log is high-throughput, this interpretation overhead compounds. The JMESPath PHP implementation benchmark showed a 7x-60x speed difference between interpreted AST traversal and compiled output for the same expressions.

**Why it happens:** Interpretation requires traversing the definition structure, dispatching on operation types, and performing dynamic field lookups — all on every event. Compiled code does this once at build time.

**Why it may not matter here:** Projection observers in this library are I/O-bound (they write to a database). The bottleneck is almost certainly the storage write, not the definition interpretation. JSONata's performance problem was at billions of events per day; event sourcing read models at typical application scale are much lower volume. The Reco case that rewrote JSONata for a 1000x speedup was evaluating thousands of distinct expressions against every message in a real-time pipeline — a very different workload.

**How to avoid:**
- Parse and compile the `ProjectionDefinition` once on registration, not on every event. Store the compiled representation (e.g., an AST or a struct of closures) internally. Never re-parse the JSON on each invocation.
- Benchmark only if the event log exceeds ~10K events/second sustained. At lower volumes, the overhead is not meaningful compared to I/O.
- Flag this as a potential future optimization: if the projection engine later supports runtime-defined projections (as stated in PROJECT.md), the compile-once approach becomes critical because user-defined projections cannot be statically compiled.

---

## 7. No Debuggability in the JSON Definition (Moderate)

**What goes wrong:** A projection produces wrong output. The user stares at the JSON definition trying to figure out why. There are no line numbers, no step-through debugging, no intermediate value inspection. JSONata and MongoDB aggregation pipeline both suffer from this — debugging complex expressions or multi-stage pipelines requires adding logging stages or console output, which are not real debugging tools.

**Why it happens:** JSON is a data format, not a program. Tooling for debugging programs (breakpoints, variable inspection, stack traces) does not exist for JSON-encoded logic.

**How to avoid:**
- Return rich, structured errors from the projection engine: `ProjectionError::FieldNotFound { field: "user_id", event_type: "OrderPlaced", definition_path: "$.output.user_id.source" }` not `"field not found"`.
- Implement a dry-run or trace mode that processes a single event through a definition and returns the intermediate state at each step. This is the practical substitute for a debugger.
- Keep definition operations simple enough that a developer can mentally trace execution. If mental tracing requires more than one page of reading, the definition has crossed the expressiveness boundary into custom code territory.

---

## 8. Multi-Stream Join Definitions Are Exponentially More Complex (Moderate)

**What goes wrong:** Single-stream projections are manageable in a JSON DSL. Multi-stream joins require the definition to encode join keys, merge strategies, and ordering semantics. MongoDB's `$lookup` is the reference example: it looks simple in documentation, is the most expensive aggregation stage in practice, and becomes extremely difficult to reason about when nested or chained.

**Why it happens:** Joins have implicit semantics that are hard to capture declaratively: what happens when the joined stream has no matching event? What is the ordering guarantee? Which stream drives the output cardinality? These questions have multiple valid answers, and a JSON DSL cannot express which answer the user wants without becoming very verbose.

**How to avoid:**
- In the JSON definition, limit join semantics to: left join on a key field, with explicit handling for the "no match" case (null field, omitted field, or error). Do not attempt to encode inner join / outer join / cross join variants in v1.
- Make the join key explicit and typed in the definition: `{ "join": { "stream": "user-events", "on": "user_id", "if_missing": "omit_field" } }`. Implicit joins based on field name matching are a source of silent bugs.
- Flag multi-stream constraint projections (from PROJECT.md) as the highest-risk area: a constraint that silently produces wrong results due to a mis-specified join is more dangerous than a read model that produces wrong results.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Defining `ProjectionDefinition` struct | No schema version field → migration impossible later | Add `schema_version: u32` in the first commit |
| Implementing field mapping operations | Silently ignoring type mismatches | Return `Result`, never use `as_str()`/`as_i64()` without checking `Some` |
| Multi-stream join support | Join semantics underspecified in JSON | Restrict to left-join-on-key only; document what "no match" does |
| Constraint projection integration | Stale projection silently used for constraint check | Hash definition; reject constraint checks against stale projections |
| Inline catch-up feature | Catch-up invalidated by definition change | Pair sequence-ID guarantee with definition hash, not just ID |
| Future runtime projection definitions | Re-parsing JSON on every event | Compile definition to internal representation once on registration |
| Expressiveness requests from users | Gradual DSL complexity growth | Ship `custom_handler` escape hatch in v1; redirect all complex logic there |

---

## Sources

- JSONata performance analysis: [Reco case study](https://www.reco.ai/blog/we-rewrote-jsonata-with-ai) and [NearForm JSONata performance dilemma](https://nearform.com/insights/the-jsonata-performance-dilemma/)
- JMESPath interpreted vs compiled: [JMESPath PHP AstRuntime vs CompilerRuntime benchmarks](https://jmespath.org/specification.html)
- MongoDB aggregation pitfalls: [MongoDB community forum on common mistakes](https://www.mongodb.com/community/forums/t/what-are-some-of-the-biggest-mistakes-people-make-in-aggregation-pipelines/11803)
- Configuration complexity clock / Turing completeness: [Increment magazine on Turing incompleteness advantages](https://increment.com/programming-languages/turing-incomplete-advantages/)
- Escape hatch pattern: [Dagster DSL design](https://dagster.io/blog/scale-and-standardize-data-pipelines-with-dsl)
- Event projection schema versioning and upcasting: [Marten events versioning](https://martendb.io/events/versioning.html), [Event-Driven.io simple versioning patterns](https://event-driven.io/en/simple_events_versioning_patterns/)
- Projection rebuild and blue-green deployment: [Dennis Doomen on projection schema changes](https://www.linkedin.com/pulse/ugly-event-sourcing-projection-schema-changes-dennis-doomen)
- Serde unknown fields pitfalls: [serde-rs issues #2121](https://github.com/serde-rs/serde/issues/2121), [serde-rs issues #1600](https://github.com/serde-rs/serde/issues/1600)
- GraphQL complexity at scale: [WunderGraph six-year retrospective](https://wundergraph.com/blog/six-year-graphql-recap)
