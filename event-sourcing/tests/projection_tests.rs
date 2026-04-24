//! Integration tests for the `projection!` macro.
//!
//! These tests verify that the macro parses the Phase 4 DSL syntax correctly
//! and emits a `#[derive(Deserialize)]` struct and `impl ReadModel`.
//!
//! Run with: cargo test -p event-sourcing --test projection_tests

use event_sourcing::query::TagFilter;
use event_sourcing::{
    FieldSpec, HandlerSpec, ObjectFieldSpecBuilder, ProjectionDefinition, ReadModel,
    ScalarFieldSpecBuilder,
};
use event_sourcing_macros::projection;

// ── Test 1: Basic struct generation ─────────────────────────────────────────

projection! {
    projection Foo {
        query tag.starts_with("foo:") as f

        name: f.Created.name
    }
}

#[test]
fn projection_macro_generates_struct() {
    let def = Foo::definition();
    assert_eq!(def.name, "Foo");
}

// ── Test 2: Optional vs required fields ─────────────────────────────────────

projection! {
    projection Bar {
        query tag.starts_with("bar:") as b

        required_name: b.Created.name
        optional_name?: b.Renamed.name
    }
}

#[test]
fn projection_macro_optional_field() {
    let def = Bar::definition();
    let fields = &def.fields;
    match fields.get("required_name").unwrap() {
        FieldSpec::Scalar(s) => assert!(s.required, "required_name should be required"),
        _ => panic!("expected scalar"),
    }
    match fields.get("optional_name").unwrap() {
        FieldSpec::Scalar(s) => assert!(!s.required, "optional_name should not be required"),
        _ => panic!("expected scalar"),
    }
}

// ── Test 3: Object field generates companion struct ──────────────────────────

projection! {
    projection Baz {
        query tag.starts_with("baz:") as b

        address? {
            city:        b.Created.city
            postal_code: b.Created.postal_code
        }
    }
}

#[test]
fn projection_macro_object_field() {
    let def = Baz::definition();
    match def.fields.get("address").unwrap() {
        FieldSpec::Object(o) => {
            assert!(o.fields.contains_key("city"), "should have city sub-field");
            assert!(
                o.fields.contains_key("postal_code"),
                "should have postal_code sub-field"
            );
        }
        _ => panic!("expected object field spec"),
    }
}

// ── Test 4: cleared_by on object field ───────────────────────────────────────

projection! {
    projection Qux {
        query tag.starts_with("qux:") as q

        address? {
            city: q.Created.city
        } cleared_by q.AddressCleared
    }
}

#[test]
fn projection_macro_cleared_by() {
    let def = Qux::definition();
    match def.fields.get("address").unwrap() {
        FieldSpec::Object(o) => {
            assert_eq!(o.cleared_by.len(), 1, "cleared_by should have 1 entry");
            assert_eq!(
                o.cleared_by[0].as_str(),
                "AddressCleared",
                "cleared_by should be AddressCleared"
            );
        }
        _ => panic!("expected object field spec"),
    }
}

// ── Test 5: Macro output equals manually built ProjectionDefinition ──────────

projection! {
    projection CustomerViewMacro {
        query tag.starts_with("customer:") as c

        name:  c.CustomerRegistered.name
             | c.CustomerRenamed.name

        accountant_name?:  c.AccountantAssigned.name
                         | c.AccountantRemoved = null

        address? {
            city:        c.CustomerRegistered.city
                       | c.AddressChanged.city
            postal_code: c.CustomerRegistered.postal_code
                       | c.AddressChanged.postal_code
        } cleared_by c.AddressCleared
    }
}

#[test]
fn projection_macro_definition_matches_builder() {
    let via_macro = CustomerViewMacro::definition();

    let via_builder = ProjectionDefinition::builder(
        "CustomerViewMacro",
        TagFilter::StartsWith("customer:".to_string()),
    )
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
            .on(
                "AccountantRemoved",
                HandlerSpec::value(serde_json::Value::Null),
            )
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

    assert_eq!(
        via_macro, via_builder,
        "macro-generated definition should equal manually built definition"
    );
}
