//! End-to-end integration test: `projection!` → `ProjectionEngine::project` → typed result.
//!
//! Run with: cargo test -p event-sourcing --features macros --test projection_e2e

use event_sourcing::{
    projection, EventType, GlobalSequenceId, ProjectionEngine, StoredEvent,
};
use std::collections::HashSet;
use std::time::SystemTime;

// ── CustomerView projection defined via macro ────────────────────────────────

projection! {
    projection CustomerView {
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

// ── Helper ───────────────────────────────────────────────────────────────────

fn make_stored_event(event_type: &str, payload: serde_json::Value) -> StoredEvent {
    StoredEvent {
        global_sequence: GlobalSequenceId::new(1),
        event_type: EventType::from(event_type),
        payload,
        tags: HashSet::new(),
        timestamp: SystemTime::now(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn e2e_projection_scalar_fields() {
    let events = vec![
        make_stored_event(
            "CustomerRegistered",
            serde_json::json!({"name": "Alice", "city": "London", "postal_code": "EC1"}),
        ),
        make_stored_event("CustomerRenamed", serde_json::json!({"name": "Alicia"})),
    ];
    let result: CustomerView = ProjectionEngine::project::<CustomerView>(events.into_iter())
        .expect("projection should succeed");
    assert_eq!(result.name, "Alicia", "last name event wins");
    assert_eq!(result.accountant_name, None, "no accountant assigned");
    assert!(result.address.is_some(), "address should be populated");
    assert_eq!(
        result.address.as_ref().unwrap().city.as_deref(),
        Some("London")
    );
    assert_eq!(
        result.address.as_ref().unwrap().postal_code.as_deref(),
        Some("EC1")
    );
}

#[test]
fn e2e_projection_clear_object() {
    let events = vec![
        make_stored_event(
            "CustomerRegistered",
            serde_json::json!({"name": "Bob", "city": "Paris", "postal_code": "75001"}),
        ),
        make_stored_event("AddressCleared", serde_json::json!({})),
    ];
    let result: CustomerView = ProjectionEngine::project::<CustomerView>(events.into_iter())
        .expect("projection should succeed");
    assert_eq!(result.name, "Bob");
    assert!(result.address.is_none(), "address should be cleared to None");
}

#[test]
fn e2e_projection_accountant_assigned_then_removed() {
    let events = vec![
        make_stored_event(
            "CustomerRegistered",
            serde_json::json!({"name": "Carol", "city": "Berlin", "postal_code": "10115"}),
        ),
        make_stored_event("AccountantAssigned", serde_json::json!({"name": "Dave"})),
        make_stored_event("AccountantRemoved", serde_json::json!({})),
    ];
    let result: CustomerView = ProjectionEngine::project::<CustomerView>(events.into_iter())
        .expect("projection should succeed");
    assert_eq!(result.accountant_name, None, "accountant should be removed");
}
