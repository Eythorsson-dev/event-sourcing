//! Event schema primitives. Defines FieldType (macro-generated enum per D-01/D-02),
//! FieldDef, EventSchemaDef (per D-04), and the Event trait (per D-03).

use std::collections::HashSet;

use crate::types::EventType;

/// The scalar field types the projection engine understands.
/// Corresponds to D-01 and D-02 from Phase 4 context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FieldType {
    String,
    Integer,
    Decimal,
    Date,
    DateTime,
}

/// Description of a single field in an event payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub name: std::string::String,
    pub field_type: FieldType,
    /// If true, the field may be absent in an event payload (maps from Option<T> in Rust).
    /// Changing this is a schema conflict (D-07).
    #[serde(default)]
    pub optional: bool,
}

/// The serializable schema record for a single event type.
/// Records the event type name and all its fields. Corresponds to D-04.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSchemaDef {
    pub event_type: EventType,
    pub fields: Vec<FieldDef>,
}

impl EventSchemaDef {
    /// Strict compatibility (D-07): event_type must match AND field set must match
    /// (name + field_type + optional triples). Field ordering is ignored.
    /// Changing `optional` on a field is a conflict — it changes the payload contract.
    pub fn is_compatible_with(&self, other: &EventSchemaDef) -> bool {
        if self.event_type != other.event_type {
            return false;
        }
        let a: HashSet<(&str, &FieldType, bool)> = self
            .fields
            .iter()
            .map(|f| (f.name.as_str(), &f.field_type, f.optional))
            .collect();
        let b: HashSet<(&str, &FieldType, bool)> = other
            .fields
            .iter()
            .map(|f| (f.name.as_str(), &f.field_type, f.optional))
            .collect();
        a == b
    }
}

/// Bridge between compiled Rust event structs and the persisted EventSchemaDef.
/// Corresponds to D-03.
pub trait Event {
    fn event_type() -> &'static str;
    fn schema() -> EventSchemaDef;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event_type(s: &str) -> EventType {
        EventType::from(s)
    }

    #[test]
    fn fieldtype_serde_roundtrip() {
        let variants = [
            FieldType::String,
            FieldType::Integer,
            FieldType::Decimal,
            FieldType::Date,
            FieldType::DateTime,
        ];
        let expected_json = [
            r#""String""#,
            r#""Integer""#,
            r#""Decimal""#,
            r#""Date""#,
            r#""DateTime""#,
        ];

        for (variant, expected) in variants.iter().zip(expected_json.iter()) {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, *expected);
            let decoded: FieldType = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, *variant);
        }
    }

    #[test]
    fn eventschemadef_serde_roundtrip() {
        let original = EventSchemaDef {
            event_type: make_event_type("OrderPlaced"),
            fields: vec![
                FieldDef {
                    name: "order_id".to_owned(),
                    field_type: FieldType::String,
                    optional: false,
                },
                FieldDef {
                    name: "amount".to_owned(),
                    field_type: FieldType::Decimal,
                    optional: false,
                },
                FieldDef {
                    name: "note".to_owned(),
                    field_type: FieldType::String,
                    optional: true,
                },
            ],
        };

        let json = serde_json::to_string(&original).unwrap();
        let decoded: EventSchemaDef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn eventschemadef_deny_unknown_fields() {
        let result = serde_json::from_str::<EventSchemaDef>(
            r#"{"event_type":"X","fields":[],"extra":true}"#,
        );
        assert!(result.is_err(), "expected error for unknown field 'extra'");
    }

    #[test]
    fn is_compatible_with_true_when_equal() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::Integer,
                optional: false,
            }],
        };
        let b = a.clone();
        assert!(a.is_compatible_with(&b));
    }

    #[test]
    fn is_compatible_with_true_when_fields_reordered() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![
                FieldDef {
                    name: "x".to_owned(),
                    field_type: FieldType::Integer,
                    optional: false,
                },
                FieldDef {
                    name: "y".to_owned(),
                    field_type: FieldType::String,
                    optional: false,
                },
            ],
        };
        let b = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![
                FieldDef {
                    name: "y".to_owned(),
                    field_type: FieldType::String,
                    optional: false,
                },
                FieldDef {
                    name: "x".to_owned(),
                    field_type: FieldType::Integer,
                    optional: false,
                },
            ],
        };
        assert!(a.is_compatible_with(&b));
    }

    #[test]
    fn is_compatible_with_false_when_field_added() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::Integer,
                optional: false,
            }],
        };
        let b = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![
                FieldDef {
                    name: "x".to_owned(),
                    field_type: FieldType::Integer,
                    optional: false,
                },
                FieldDef {
                    name: "extra".to_owned(),
                    field_type: FieldType::String,
                    optional: false,
                },
            ],
        };
        assert!(!a.is_compatible_with(&b));
    }

    #[test]
    fn is_compatible_with_false_when_field_type_changed() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::Integer,
                optional: false,
            }],
        };
        let b = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::String,
                optional: false,
            }],
        };
        assert!(!a.is_compatible_with(&b));
    }

    #[test]
    fn is_compatible_with_false_when_event_type_differs() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![],
        };
        let b = EventSchemaDef {
            event_type: make_event_type("Bar"),
            fields: vec![],
        };
        assert!(!a.is_compatible_with(&b));
    }

    #[test]
    fn is_compatible_with_false_when_optional_changed() {
        let a = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::String,
                optional: false,
            }],
        };
        let b = EventSchemaDef {
            event_type: make_event_type("Foo"),
            fields: vec![FieldDef {
                name: "x".to_owned(),
                field_type: FieldType::String,
                optional: true,
            }],
        };
        assert!(!a.is_compatible_with(&b));
    }

    #[test]
    fn event_trait_manual_impl_compiles() {
        struct Foo;

        impl Event for Foo {
            fn event_type() -> &'static str {
                "Foo"
            }

            fn schema() -> EventSchemaDef {
                EventSchemaDef {
                    event_type: EventType::from("Foo"),
                    fields: vec![
                        FieldDef {
                            name: "id".to_owned(),
                            field_type: FieldType::String,
                            optional: false,
                        },
                        FieldDef {
                            name: "count".to_owned(),
                            field_type: FieldType::Integer,
                            optional: false,
                        },
                    ],
                }
            }
        }

        assert_eq!(Foo::event_type(), "Foo");
        assert_eq!(Foo::schema().fields.len(), 2);
    }
}
