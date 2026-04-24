use serde_json::{Map, Value};
use serde_json_path::JsonPath;

use crate::event::StoredEvent;
use crate::projection::definition::{FieldSpec, HandlerSpec, ProjectionDefinition};
use crate::projection::error::ProjectionError;
use crate::types::{EventType, GlobalSequenceId};

/// Evaluate an RFC 9535 JSON Path expression against a serde_json::Value payload.
/// Returns the first matching node, or None if no match.
///
/// Callers (the builder) guarantee that `path` is a valid JsonPath string —
/// `unwrap()` here is safe because invalid paths are rejected at definition-build time.
pub(crate) fn evaluate_path(payload: &Value, path: &str) -> Option<Value> {
    let compiled = JsonPath::parse(path).ok()?;
    compiled.query(payload).first().cloned()
}

// ── ReadModel trait ──────────────────────────────────────────────────────────

/// Implemented by user-defined read model structs.
///
/// The struct must be `Deserialize` so that `ProjectionEngine::project` can
/// deserialize the raw `serde_json::Value` state into the typed model.
pub trait ReadModel: serde::de::DeserializeOwned {
    fn definition() -> ProjectionDefinition;
}

// ── ProjectionCheckpoint ─────────────────────────────────────────────────────

/// Opaque snapshot of projection fold state after processing `last_sequence`.
///
/// Callers store and restore this value to enable incremental folds —
/// they should not inspect or modify its internals.
pub struct ProjectionCheckpoint {
    pub(crate) raw_state: Value,
    pub(crate) last_sequence: GlobalSequenceId,
}

// ── ProjectionEngine ─────────────────────────────────────────────────────────

pub struct ProjectionEngine;

impl ProjectionEngine {
    /// Fold events into a `serde_json::Value` state using the provided definition.
    ///
    /// - Starts with initial state: all fields set to their `default` value if present, else `null`.
    /// - Events whose `event_type` is not referenced in any handler are silently skipped (D-22).
    /// - Returns `Err(ProjectionError::FieldNotFound)` if a required `From` path is absent.
    pub fn apply_raw(
        def: &ProjectionDefinition,
        events: impl Iterator<Item = StoredEvent>,
    ) -> Result<Value, ProjectionError> {
        let mut state = Value::Object(Map::new());
        Self::initialize_state(def, &mut state);

        for event in events {
            Self::apply_event(def, &event, &mut state)?;
        }
        Ok(state)
    }

    /// Typed convenience: calls `apply_raw` then deserializes into `M`.
    pub fn project<M: ReadModel>(
        events: impl Iterator<Item = StoredEvent>,
    ) -> Result<M, ProjectionError> {
        let def = M::definition();
        let raw = Self::apply_raw(&def, events)?;
        Ok(serde_json::from_value(raw)?)
    }

    /// Incremental fold from an optional checkpoint.
    ///
    /// `checkpoint = None` performs a full replay from scratch.
    /// Returns the typed model and a new checkpoint capturing the updated state.
    pub fn project_from<M: ReadModel>(
        checkpoint: Option<ProjectionCheckpoint>,
        new_events: impl Iterator<Item = StoredEvent>,
    ) -> Result<(M, ProjectionCheckpoint), ProjectionError> {
        let def = M::definition();
        let mut state = match checkpoint {
            Some(cp) => cp.raw_state,
            None => {
                let mut s = Value::Object(Map::new());
                Self::initialize_state(&def, &mut s);
                s
            }
        };
        let mut last_sequence = GlobalSequenceId::ZERO;
        for event in new_events {
            last_sequence = event.global_sequence;
            Self::apply_event(&def, &event, &mut state)?;
        }
        let model: M = serde_json::from_value(state.clone())?;
        Ok((
            model,
            ProjectionCheckpoint {
                raw_state: state,
                last_sequence,
            },
        ))
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Populate `state` with initial values from field defaults.
    fn initialize_state(def: &ProjectionDefinition, state: &mut Value) {
        let obj = state.as_object_mut().expect("state must be an Object");
        for (field_name, field_spec) in &def.fields {
            match field_spec {
                FieldSpec::Scalar(scalar) => {
                    let initial = scalar.default.clone().unwrap_or(Value::Null);
                    obj.insert(field_name.clone(), initial);
                }
                FieldSpec::Object(object) => {
                    // Initialize as a nested Object with all sub-fields set to their defaults
                    let mut nested = Map::new();
                    for (sub_name, sub_spec) in &object.fields {
                        let initial = sub_spec.default.clone().unwrap_or(Value::Null);
                        nested.insert(sub_name.clone(), initial);
                    }
                    obj.insert(field_name.clone(), Value::Object(nested));
                }
            }
        }
    }

    /// Apply a single event to the current state following all matching handlers.
    fn apply_event(
        def: &ProjectionDefinition,
        event: &StoredEvent,
        state: &mut Value,
    ) -> Result<(), ProjectionError> {
        for (field_name, field_spec) in &def.fields {
            match field_spec {
                FieldSpec::Scalar(scalar) => {
                    if let Some(handler) = scalar.events.get(&event.event_type) {
                        let field_val = state
                            .as_object_mut()
                            .expect("state must be an Object")
                            .entry(field_name.clone())
                            .or_insert(Value::Null);
                        Self::apply_handler(
                            field_val,
                            handler,
                            &event.payload,
                            field_name,
                            !scalar.required, // optional = not required
                            &event.event_type,
                        )?;
                    }
                }
                FieldSpec::Object(object) => {
                    // Check cleared_by first
                    if object.cleared_by.contains(&event.event_type) {
                        state
                            .as_object_mut()
                            .expect("state must be an Object")
                            .insert(field_name.clone(), Value::Null);
                        // Still process nested field handlers below even after a clear?
                        // No — cleared_by semantics: the entire object is null; skip sub-fields.
                        continue;
                    }
                    // Apply sub-field handlers
                    for (sub_name, sub_spec) in &object.fields {
                        if let Some(handler) = sub_spec.events.get(&event.event_type) {
                            // Ensure nested object exists in state
                            let nested_obj = state
                                .as_object_mut()
                                .expect("state must be an Object")
                                .entry(field_name.clone())
                                .or_insert_with(|| Value::Object(Map::new()));
                            // If the parent object was previously cleared (null), rebuild it
                            if nested_obj.is_null() {
                                *nested_obj = Value::Object(Map::new());
                            }
                            let sub_val = nested_obj
                                .as_object_mut()
                                .expect("nested state must be an Object")
                                .entry(sub_name.clone())
                                .or_insert(Value::Null);
                            Self::apply_handler(
                                sub_val,
                                handler,
                                &event.payload,
                                sub_name,
                                !sub_spec.required,
                                &event.event_type,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Apply a `HandlerSpec` to a mutable field value.
    ///
    /// `optional`: if true, an absent `From` path sets the field to `null` instead of erroring.
    fn apply_handler(
        field_val: &mut Value,
        handler: &HandlerSpec,
        payload: &Value,
        field_name: &str,
        optional: bool,
        event_type: &EventType,
    ) -> Result<(), ProjectionError> {
        match handler {
            HandlerSpec::From(h) => match evaluate_path(payload, &h.from) {
                Some(v) => *field_val = v,
                None => {
                    if optional {
                        *field_val = Value::Null;
                    } else {
                        return Err(ProjectionError::FieldNotFound {
                            event_type: event_type.clone(),
                            path: h.from.clone(),
                        });
                    }
                }
            },
            HandlerSpec::Value(h) => {
                *field_val = h.value.clone();
            }
            HandlerSpec::Increment(h) => {
                let current = Self::as_f64(field_val, field_name)?;
                *field_val = Value::from(current + h.increment);
            }
            HandlerSpec::Decrement(h) => {
                let current = Self::as_f64(field_val, field_name)?;
                *field_val = Value::from(current - h.decrement);
            }
            HandlerSpec::IncrementBy(h) => {
                let current = Self::as_f64(field_val, field_name)?;
                let delta = evaluate_path(payload, &h.increment_by)
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| ProjectionError::TypeMismatch {
                        field: field_name.to_owned(),
                        actual: "non-numeric value at path".to_owned(),
                    })?;
                *field_val = Value::from(current + delta);
            }
            HandlerSpec::DecrementBy(h) => {
                let current = Self::as_f64(field_val, field_name)?;
                let delta = evaluate_path(payload, &h.decrement_by)
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| ProjectionError::TypeMismatch {
                        field: field_name.to_owned(),
                        actual: "non-numeric value at path".to_owned(),
                    })?;
                *field_val = Value::from(current - delta);
            }
        }
        Ok(())
    }

    /// Extract an f64 from the current field value, returning TypeMismatch if not numeric.
    fn as_f64(val: &Value, field_name: &str) -> Result<f64, ProjectionError> {
        // A null field treated as 0.0 for arithmetic (sensible default for accumulators)
        if val.is_null() {
            return Ok(0.0);
        }
        val.as_f64().ok_or_else(|| ProjectionError::TypeMismatch {
            field: field_name.to_owned(),
            actual: format!("{}", val),
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::definition::{
        ObjectFieldSpecBuilder, ProjectionDefinition, ScalarFieldSpecBuilder,
    };
    use crate::query::TagFilter;
    use serde::Deserialize;
    use serde_json::json;
    use std::collections::HashSet;
    use std::time::SystemTime;

    fn make_event(event_type: &str, payload: Value) -> StoredEvent {
        StoredEvent {
            global_sequence: GlobalSequenceId::new(1),
            event_type: EventType::from(event_type),
            payload,
            tags: HashSet::new(),
            timestamp: SystemTime::now(),
        }
    }

    fn make_event_seq(event_type: &str, payload: Value, seq: u64) -> StoredEvent {
        StoredEvent {
            global_sequence: GlobalSequenceId::new(seq),
            event_type: EventType::from(event_type),
            payload,
            tags: HashSet::new(),
            timestamp: SystemTime::now(),
        }
    }

    fn customer_def() -> ProjectionDefinition {
        ProjectionDefinition::builder("CustomerView", TagFilter::StartsWith("customer:".into()))
            .scalar(
                "name",
                ScalarFieldSpecBuilder::new()
                    .required()
                    .on("CustomerRegistered", HandlerSpec::from_path("$.name"))
                    .build(),
            )
            .build()
    }

    // ── Path evaluator (from Task 2, kept here for completeness) ────────────

    #[test]
    fn path_simple_field() {
        let payload = json!({"amount": 42});
        let result = evaluate_path(&payload, "$.amount");
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn path_nested() {
        let payload = json!({"address": {"city": "London"}});
        let result = evaluate_path(&payload, "$.address.city");
        assert_eq!(result, Some(json!("London")));
    }

    #[test]
    fn path_missing_field() {
        let payload = json!({"x": 1});
        let result = evaluate_path(&payload, "$.y");
        assert_eq!(result, None);
    }

    #[test]
    fn path_null_value() {
        let payload = json!({"name": null});
        let result = evaluate_path(&payload, "$.name");
        assert_eq!(result, Some(Value::Null));
    }

    #[test]
    fn path_invalid_expression() {
        let payload = json!({"x": 1});
        let result = evaluate_path(&payload, "not-a-path");
        assert_eq!(result, None);
    }

    // ── apply_raw tests ──────────────────────────────────────────────────────

    #[test]
    fn apply_raw_sets_scalar_from_event() {
        let def = customer_def();
        let events = vec![make_event("CustomerRegistered", json!({"name": "Alice"}))];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["name"], json!("Alice"));
    }

    #[test]
    fn apply_raw_multiple_events_last_wins() {
        let def = customer_def();
        let events = vec![
            make_event("CustomerRegistered", json!({"name": "Alice"})),
            make_event("CustomerRegistered", json!({"name": "Bob"})),
        ];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["name"], json!("Bob"));
    }

    #[test]
    fn apply_raw_value_null_clears_field() {
        let def = ProjectionDefinition::builder("Test", TagFilter::StartsWith("test:".into()))
            .scalar(
                "accountant",
                ScalarFieldSpecBuilder::new()
                    .on("AssignAccountant", HandlerSpec::from_path("$.name"))
                    .on("RemoveAccountant", HandlerSpec::value(Value::Null))
                    .build(),
            )
            .build();

        let events = vec![
            make_event("AssignAccountant", json!({"name": "Carol"})),
            make_event("RemoveAccountant", json!({})),
        ];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["accountant"], Value::Null);
    }

    #[test]
    fn apply_raw_increment() {
        let def =
            ProjectionDefinition::builder("Counter", TagFilter::StartsWith("counter:".into()))
                .scalar(
                    "count",
                    ScalarFieldSpecBuilder::new()
                        .default_value(json!(0.0))
                        .on("Incremented", HandlerSpec::increment(5.0))
                        .build(),
                )
                .build();

        let events = vec![
            make_event("Incremented", json!({})),
            make_event("Incremented", json!({})),
        ];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["count"].as_f64().unwrap(), 10.0);
    }

    #[test]
    fn apply_raw_nested_object_populated() {
        let def = ProjectionDefinition::builder(
            "CustomerView",
            TagFilter::StartsWith("customer:".into()),
        )
        .object(
            "address",
            ObjectFieldSpecBuilder::new()
                .field(
                    "city",
                    ScalarFieldSpecBuilder::new()
                        .on("CustomerRegistered", HandlerSpec::from_path("$.city"))
                        .build(),
                )
                .field(
                    "postal_code",
                    ScalarFieldSpecBuilder::new()
                        .on(
                            "CustomerRegistered",
                            HandlerSpec::from_path("$.postal_code"),
                        )
                        .build(),
                )
                .build(),
        )
        .build();

        let events = vec![make_event(
            "CustomerRegistered",
            json!({"city": "London", "postal_code": "EC1"}),
        )];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["address"]["city"], json!("London"));
        assert_eq!(state["address"]["postal_code"], json!("EC1"));
    }

    #[test]
    fn apply_raw_cleared_by() {
        let def = ProjectionDefinition::builder(
            "CustomerView",
            TagFilter::StartsWith("customer:".into()),
        )
        .object(
            "address",
            ObjectFieldSpecBuilder::new()
                .cleared_by("AddressCleared")
                .field(
                    "city",
                    ScalarFieldSpecBuilder::new()
                        .on("CustomerRegistered", HandlerSpec::from_path("$.city"))
                        .build(),
                )
                .build(),
        )
        .build();

        let events = vec![
            make_event("CustomerRegistered", json!({"city": "London"})),
            make_event("AddressCleared", json!({})),
        ];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["address"], Value::Null);
    }

    #[test]
    fn apply_raw_skips_unknown_event_types() {
        let def = customer_def();
        let events = vec![
            make_event("CustomerRegistered", json!({"name": "Alice"})),
            make_event("SomeOtherEvent", json!({"irrelevant": "data"})),
        ];
        // Should not error — SomeOtherEvent is simply ignored (D-22)
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["name"], json!("Alice"));
    }

    #[test]
    fn apply_raw_default_value_used_before_events() {
        let def = ProjectionDefinition::builder("Test", TagFilter::StartsWith("test:".into()))
            .scalar(
                "status",
                ScalarFieldSpecBuilder::new()
                    .default_value(json!("pending"))
                    .on("StatusChanged", HandlerSpec::from_path("$.status"))
                    .build(),
            )
            .build();

        // No events — default should be present
        let state = ProjectionEngine::apply_raw(&def, std::iter::empty()).unwrap();
        assert_eq!(state["status"], json!("pending"));
    }

    #[test]
    fn apply_raw_optional_field_absent_yields_null() {
        let def = ProjectionDefinition::builder("Test", TagFilter::StartsWith("test:".into()))
            .scalar(
                "middle_name",
                ScalarFieldSpecBuilder::new()
                    // required = false (default) → optional = true
                    .on(
                        "CustomerRegistered",
                        HandlerSpec::from_path("$.middle_name"),
                    )
                    .build(),
            )
            .build();

        // Payload has no middle_name — optional field should become null, not error
        let events = vec![make_event("CustomerRegistered", json!({"name": "Alice"}))];
        let state = ProjectionEngine::apply_raw(&def, events.into_iter()).unwrap();
        assert_eq!(state["middle_name"], Value::Null);
    }

    #[test]
    fn apply_raw_required_field_absent_yields_error() {
        let def = ProjectionDefinition::builder("Test", TagFilter::StartsWith("test:".into()))
            .scalar(
                "name",
                ScalarFieldSpecBuilder::new()
                    .required()
                    .on("CustomerRegistered", HandlerSpec::from_path("$.name"))
                    .build(),
            )
            .build();

        // Payload has no name — required field → FieldNotFound error
        let events = vec![make_event("CustomerRegistered", json!({"other": "field"}))];
        let result = ProjectionEngine::apply_raw(&def, events.into_iter());
        assert!(
            matches!(result, Err(ProjectionError::FieldNotFound { .. })),
            "expected FieldNotFound error, got: {:?}",
            result
        );
    }

    // ── project<M> tests ─────────────────────────────────────────────────────

    #[derive(Debug, Deserialize, PartialEq)]
    struct CustomerModel {
        name: String,
    }

    impl ReadModel for CustomerModel {
        fn definition() -> ProjectionDefinition {
            ProjectionDefinition::builder(
                "CustomerModel",
                TagFilter::StartsWith("customer:".into()),
            )
            .scalar(
                "name",
                ScalarFieldSpecBuilder::new()
                    .required()
                    .on("CustomerRegistered", HandlerSpec::from_path("$.name"))
                    .build(),
            )
            .build()
        }
    }

    #[test]
    fn project_returns_typed_model() {
        let events = vec![make_event("CustomerRegistered", json!({"name": "Alice"}))];
        let model: CustomerModel = ProjectionEngine::project(events.into_iter()).unwrap();
        assert_eq!(model.name, "Alice");
    }

    // ── project_from tests ───────────────────────────────────────────────────

    #[test]
    fn project_from_checkpoint_incremental() {
        // Build a definition with a counter that tracks event count
        #[derive(Debug, Deserialize, PartialEq)]
        struct CounterModel {
            count: f64,
        }

        impl ReadModel for CounterModel {
            fn definition() -> ProjectionDefinition {
                ProjectionDefinition::builder(
                    "CounterModel",
                    TagFilter::StartsWith("counter:".into()),
                )
                .scalar(
                    "count",
                    ScalarFieldSpecBuilder::new()
                        .default_value(json!(0.0))
                        .on("Ticked", HandlerSpec::increment(1.0))
                        .build(),
                )
                .build()
            }
        }

        let events_1_2 = vec![
            make_event_seq("Ticked", json!({}), 1),
            make_event_seq("Ticked", json!({}), 2),
        ];
        let event_3 = vec![make_event_seq("Ticked", json!({}), 3)];

        // Full replay of 2 events → take checkpoint
        let (_, checkpoint) =
            ProjectionEngine::project_from::<CounterModel>(None, events_1_2.into_iter()).unwrap();
        assert_eq!(checkpoint.last_sequence, GlobalSequenceId::new(2));

        // Incremental: checkpoint + event 3
        let (model, new_cp) =
            ProjectionEngine::project_from::<CounterModel>(Some(checkpoint), event_3.into_iter())
                .unwrap();
        assert_eq!(model.count, 3.0);
        assert_eq!(new_cp.last_sequence, GlobalSequenceId::new(3));

        // Verify matches full replay of all 3
        let all_events = vec![
            make_event_seq("Ticked", json!({}), 1),
            make_event_seq("Ticked", json!({}), 2),
            make_event_seq("Ticked", json!({}), 3),
        ];
        let full_model: CounterModel = ProjectionEngine::project(all_events.into_iter()).unwrap();
        assert_eq!(model, full_model);
    }
}
