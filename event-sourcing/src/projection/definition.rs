use indexmap::IndexMap;
use serde_json::Value;

use crate::query::TagFilter;
use crate::types::EventType;

// ── HandlerSpec internal structs (each serializes as a single-key JSON object) ──

/// Copy field from event payload via JSON Path: `{ "from": "$.path" }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerFrom {
    pub from: String,
}

/// Set to a static literal or null: `{ "value": <literal> }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerValue {
    pub value: Value,
}

/// Add N to a numeric field: `{ "increment": N }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerIncrement {
    pub increment: f64,
}

/// Subtract N from a numeric field: `{ "decrement": N }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerDecrement {
    pub decrement: f64,
}

/// Add value at path to a numeric field: `{ "increment_by": "$.path" }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerIncrementBy {
    pub increment_by: String,
}

/// Subtract value at path from a numeric field: `{ "decrement_by": "$.path" }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandlerDecrementBy {
    pub decrement_by: String,
}

/// Handler for a single event on a scalar field.
///
/// Uses `#[serde(untagged)]` so each variant serializes as the inner struct's
/// JSON representation — a single-key object. For example:
/// - `HandlerSpec::from("$.x")` → `{"from":"$.x"}`
/// - `HandlerSpec::value(Value::Null)` → `{"value":null}`
/// - `HandlerSpec::increment(5.0)` → `{"increment":5.0}`
///
/// Unknown fields are rejected at the individual struct level via
/// `#[serde(deny_unknown_fields)]` on each inner struct.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum HandlerSpec {
    From(HandlerFrom),
    Value(HandlerValue),
    Increment(HandlerIncrement),
    Decrement(HandlerDecrement),
    IncrementBy(HandlerIncrementBy),
    DecrementBy(HandlerDecrementBy),
}

impl HandlerSpec {
    pub fn from_path(path: impl Into<String>) -> Self {
        HandlerSpec::From(HandlerFrom { from: path.into() })
    }

    pub fn value(v: Value) -> Self {
        HandlerSpec::Value(HandlerValue { value: v })
    }

    pub fn increment(n: f64) -> Self {
        HandlerSpec::Increment(HandlerIncrement { increment: n })
    }

    pub fn decrement(n: f64) -> Self {
        HandlerSpec::Decrement(HandlerDecrement { decrement: n })
    }

    pub fn increment_by(path: impl Into<String>) -> Self {
        HandlerSpec::IncrementBy(HandlerIncrementBy {
            increment_by: path.into(),
        })
    }

    pub fn decrement_by(path: impl Into<String>) -> Self {
        HandlerSpec::DecrementBy(HandlerDecrementBy {
            decrement_by: path.into(),
        })
    }

    /// Return the JSON Path string if this handler references one, for validation.
    fn path_str(&self) -> Option<&str> {
        match self {
            HandlerSpec::From(h) => Some(&h.from),
            HandlerSpec::IncrementBy(h) => Some(&h.increment_by),
            HandlerSpec::DecrementBy(h) => Some(&h.decrement_by),
            _ => None,
        }
    }
}

// ── Scalar field spec ────────────────────────────────────────────────────────

/// Scalar field spec within a ProjectionDefinition.
///
/// # Path validation
///
/// JSON path strings stored in `HandlerSpec::From`, `HandlerSpec::IncrementBy`, and
/// `HandlerSpec::DecrementBy` are **only validated when using [`ScalarFieldSpecBuilder::on`]**.
/// If you construct this struct directly (e.g., via `serde_json::from_value` or a struct
/// literal), path strings are not checked at construction time. Invalid paths will produce
/// a [`ProjectionError::InvalidPath`] at fold time via `evaluate_path`.
/// Use the builder path for guaranteed early validation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarFieldSpec {
    /// If true, the Rust field type is T; if false (default), Option<T>.
    #[serde(default)]
    pub required: bool,
    /// Default value for initial state (before any events fire).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Event type → handler mapping. Insertion order is preserved.
    pub events: IndexMap<EventType, HandlerSpec>,
}

// ── Object field spec ────────────────────────────────────────────────────────

/// Nested object field spec within a ProjectionDefinition.
///
/// # Path validation and conflict detection
///
/// Sub-field path strings are validated only when the fields are built via
/// [`ScalarFieldSpecBuilder::on`]. Direct struct construction (e.g., from deserialized JSON)
/// bypasses this check; invalid paths surface as [`ProjectionError::InvalidPath`] at fold time.
///
/// Conflict detection between `cleared_by` and sub-field handlers is only enforced by
/// [`ObjectFieldSpecBuilder::build`]. Structs constructed directly carry no such guarantee.
/// Use the builder for both guarantees.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectFieldSpec {
    /// Always "object". Serializes as `"type": "object"` in JSON.
    #[serde(rename = "type")]
    pub field_type: String,
    /// Event types that, when fired, clear the entire object to null.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleared_by: Vec<EventType>,
    /// Nested scalar fields. Insertion order is preserved.
    pub fields: IndexMap<String, ScalarFieldSpec>,
}

// ── FieldSpec (top-level discriminated union) ────────────────────────────────

/// A single top-level field — either a scalar or a nested object.
///
/// Uses `#[serde(untagged)]`. The deserializer tries `Object` first because
/// `ObjectFieldSpec` has a required `"type"` key that scalars don't; if that
/// fails it falls through to `Scalar`.
///
/// Note: `deny_unknown_fields` is NOT placed on the untagged enum itself —
/// that combination is unsupported in serde and panics at runtime. Unknown
/// fields are caught by `deny_unknown_fields` on each variant struct instead.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum FieldSpec {
    Object(ObjectFieldSpec),
    Scalar(ScalarFieldSpec),
}

// ── ProjectionDefinition ─────────────────────────────────────────────────────

/// The JSON-serializable description of what a projection computes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionDefinition {
    pub name: String,
    pub query: TagFilter,
    /// Top-level fields. IndexMap preserves insertion order for deterministic JSON.
    pub fields: IndexMap<String, FieldSpec>,
}

impl ProjectionDefinition {
    pub fn builder(name: impl Into<String>, query: TagFilter) -> ProjectionDefinitionBuilder {
        ProjectionDefinitionBuilder::new(name.into(), query)
    }
}

// ── Builders ─────────────────────────────────────────────────────────────────

pub struct ProjectionDefinitionBuilder {
    name: String,
    query: TagFilter,
    fields: IndexMap<String, FieldSpec>,
}

impl ProjectionDefinitionBuilder {
    pub fn new(name: String, query: TagFilter) -> Self {
        ProjectionDefinitionBuilder {
            name,
            query,
            fields: IndexMap::new(),
        }
    }

    /// Add a scalar field with its event handlers.
    pub fn scalar(mut self, name: impl Into<String>, spec: ScalarFieldSpec) -> Self {
        self.fields.insert(name.into(), FieldSpec::Scalar(spec));
        self
    }

    /// Add a nested object field.
    pub fn object(mut self, name: impl Into<String>, spec: ObjectFieldSpec) -> Self {
        self.fields.insert(name.into(), FieldSpec::Object(spec));
        self
    }

    pub fn build(self) -> ProjectionDefinition {
        ProjectionDefinition {
            name: self.name,
            query: self.query,
            fields: self.fields,
        }
    }
}

pub struct ScalarFieldSpecBuilder {
    required: bool,
    default: Option<Value>,
    events: IndexMap<EventType, HandlerSpec>,
}

impl ScalarFieldSpecBuilder {
    pub fn new() -> Self {
        ScalarFieldSpecBuilder {
            required: false,
            default: None,
            events: IndexMap::new(),
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default_value(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }

    /// Register a handler for an event type.
    ///
    /// Path strings in `HandlerSpec::From`, `HandlerSpec::IncrementBy`, and
    /// `HandlerSpec::DecrementBy` are validated using `serde_json_path::JsonPath::parse`
    /// at call time. An invalid path is a programming error — the method panics with a
    /// descriptive message including the invalid path string so the bug surfaces immediately
    /// in tests rather than silently producing broken projections at runtime.
    pub fn on(mut self, event_type: impl Into<EventType>, handler: HandlerSpec) -> Self {
        let event_type: EventType = event_type.into();
        if let Some(path) = handler.path_str() {
            serde_json_path::JsonPath::parse(path).unwrap_or_else(|e| {
                panic!(
                    "invalid JsonPath '{}' in handler for event '{}': {}",
                    path, event_type, e
                )
            });
        }
        self.events.insert(event_type, handler);
        self
    }

    pub fn build(self) -> ScalarFieldSpec {
        ScalarFieldSpec {
            required: self.required,
            default: self.default,
            events: self.events,
        }
    }
}

impl Default for ScalarFieldSpecBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ObjectFieldSpecBuilder {
    cleared_by: Vec<EventType>,
    fields: IndexMap<String, ScalarFieldSpec>,
}

impl ObjectFieldSpecBuilder {
    pub fn new() -> Self {
        ObjectFieldSpecBuilder {
            cleared_by: Vec::new(),
            fields: IndexMap::new(),
        }
    }

    pub fn cleared_by(mut self, event_type: impl Into<EventType>) -> Self {
        self.cleared_by.push(event_type.into());
        self
    }

    pub fn field(mut self, name: impl Into<String>, spec: ScalarFieldSpec) -> Self {
        self.fields.insert(name.into(), spec);
        self
    }

    pub fn build(self) -> ObjectFieldSpec {
        // Detect conflicting event registrations: an event type in both cleared_by and a
        // sub-field handler is ambiguous — cleared_by silently wins and the sub-field update
        // is dropped, which is almost certainly a programming error.
        for evt in &self.cleared_by {
            for (sub_name, scalar) in &self.fields {
                if scalar.events.contains_key(evt) {
                    panic!(
                        "event '{}' appears in both cleared_by and sub-field handler '{}'; \
                         this is ambiguous — remove it from one",
                        evt, sub_name
                    );
                }
            }
        }
        ObjectFieldSpec {
            field_type: "object".to_owned(),
            cleared_by: self.cleared_by,
            fields: self.fields,
        }
    }
}

impl Default for ObjectFieldSpecBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn customer_query() -> TagFilter {
        TagFilter::StartsWith("customer:".into())
    }

    // ── HandlerSpec round-trips ──────────────────────────────────────────────

    #[test]
    fn handler_from_roundtrip() {
        let handler = HandlerSpec::from_path("$.amount");
        let json = serde_json::to_string(&handler).unwrap();
        assert_eq!(json, r#"{"from":"$.amount"}"#);
        let decoded: HandlerSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, handler);
    }

    #[test]
    fn handler_value_null_roundtrip() {
        let handler = HandlerSpec::value(Value::Null);
        let json = serde_json::to_string(&handler).unwrap();
        assert_eq!(json, r#"{"value":null}"#);
        let decoded: HandlerSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, handler);
    }

    #[test]
    fn handler_increment_roundtrip() {
        let handler = HandlerSpec::increment(5.0);
        let json = serde_json::to_string(&handler).unwrap();
        assert_eq!(json, r#"{"increment":5.0}"#);
        let decoded: HandlerSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, handler);
    }

    // ── Full round-trip ──────────────────────────────────────────────────────

    #[test]
    fn projection_definition_full_roundtrip() {
        // CustomerView example from CONTEXT.md
        let def = ProjectionDefinition::builder("CustomerView", customer_query())
            .scalar(
                "name",
                ScalarFieldSpecBuilder::new()
                    .required()
                    .on("CustomerRegistered", HandlerSpec::from_path("$.name"))
                    .on("CustomerRenamed", HandlerSpec::from_path("$.name"))
                    .build(),
            )
            .scalar(
                "accountant_name",
                ScalarFieldSpecBuilder::new()
                    .on("AccountantAssigned", HandlerSpec::from_path("$.name"))
                    .on("AccountantRemoved", HandlerSpec::value(Value::Null))
                    .build(),
            )
            .object(
                "address",
                ObjectFieldSpecBuilder::new()
                    .cleared_by("AddressCleared")
                    .field(
                        "city",
                        ScalarFieldSpecBuilder::new()
                            .on("CustomerRegistered", HandlerSpec::from_path("$.city"))
                            .on("AddressChanged", HandlerSpec::from_path("$.city"))
                            .build(),
                    )
                    .field(
                        "postal_code",
                        ScalarFieldSpecBuilder::new()
                            .on(
                                "CustomerRegistered",
                                HandlerSpec::from_path("$.postal_code"),
                            )
                            .on("AddressChanged", HandlerSpec::from_path("$.postal_code"))
                            .build(),
                    )
                    .build(),
            )
            .build();

        let json = serde_json::to_string(&def).unwrap();
        let decoded: ProjectionDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, def);
    }

    // ── deny_unknown_fields ──────────────────────────────────────────────────

    #[test]
    fn projection_definition_deny_unknown_fields() {
        let result = serde_json::from_str::<ProjectionDefinition>(
            r#"{"name":"X","query":{"StartsWith":"a"},"fields":{},"extra":true}"#,
        );
        assert!(
            result.is_err(),
            "expected error for unknown field 'extra', got: {:?}",
            result
        );
    }

    #[test]
    fn scalar_deny_unknown() {
        let result = serde_json::from_str::<ScalarFieldSpec>(r#"{"events":{},"bogus":1}"#);
        assert!(
            result.is_err(),
            "expected error for unknown field 'bogus', got: {:?}",
            result
        );
    }

    // ── Insertion order ──────────────────────────────────────────────────────

    #[test]
    fn insertion_order_preserved() {
        let def = ProjectionDefinition::builder("Test", customer_query())
            .scalar(
                "z",
                ScalarFieldSpecBuilder::new()
                    .on("E", HandlerSpec::from_path("$.z"))
                    .build(),
            )
            .scalar(
                "a",
                ScalarFieldSpecBuilder::new()
                    .on("E", HandlerSpec::from_path("$.a"))
                    .build(),
            )
            .scalar(
                "m",
                ScalarFieldSpecBuilder::new()
                    .on("E", HandlerSpec::from_path("$.m"))
                    .build(),
            )
            .build();

        let json = serde_json::to_string(&def).unwrap();
        let decoded: ProjectionDefinition = serde_json::from_str(&json).unwrap();
        let keys: Vec<&str> = decoded.fields.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["z", "a", "m"]);
    }

    // ── Builder equivalence ──────────────────────────────────────────────────

    #[test]
    fn builder_produces_equivalent_to_manual() {
        let via_builder = ProjectionDefinition::builder("OrderSummary", customer_query())
            .scalar(
                "total",
                ScalarFieldSpecBuilder::new()
                    .required()
                    .on("OrderPlaced", HandlerSpec::from_path("$.total"))
                    .build(),
            )
            .build();

        // Manually construct the equivalent struct
        let mut events: IndexMap<EventType, HandlerSpec> = IndexMap::new();
        events.insert(
            EventType::from("OrderPlaced"),
            HandlerSpec::From(HandlerFrom {
                from: "$.total".to_owned(),
            }),
        );
        let mut fields: IndexMap<String, FieldSpec> = IndexMap::new();
        fields.insert(
            "total".to_owned(),
            FieldSpec::Scalar(ScalarFieldSpec {
                required: true,
                default: None,
                events,
            }),
        );
        let manual = ProjectionDefinition {
            name: "OrderSummary".to_owned(),
            query: customer_query(),
            fields,
        };

        assert_eq!(via_builder, manual);
    }

    // ── Path validation ──────────────────────────────────────────────────────

    #[test]
    fn builder_panics_on_invalid_path() {
        let result = std::panic::catch_unwind(|| {
            ScalarFieldSpecBuilder::new()
                .on("SomeEvent", HandlerSpec::from_path("not-a-valid-path"))
        });
        assert!(result.is_err(), "expected panic for invalid JsonPath");
        if let Err(e) = result {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(
                msg.contains("not-a-valid-path"),
                "panic message should include the invalid path, got: {:?}",
                msg
            );
        }
    }
}
