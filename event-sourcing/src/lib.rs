//! Event sourcing core library — types, traits, and contracts.
//!
//! This crate defines the foundational types and the `LogStore` trait that all
//! storage implementations build on. No I/O — just contracts.

pub mod error;
pub mod event;
pub mod event_log;
pub mod projection;
pub mod query;
pub mod schema;
pub mod store;
pub mod types;

// Re-export primary public API at crate root for ergonomic imports
pub use error::{AppendCondition, AppendError, StoreError};
pub use event::StoredEvent;
pub use event_log::{EventLog, EventLogError, EventStreamExt, StreamError};
pub use query::{Criterion, Query, TagFilter};
pub use schema::{Event, EventSchemaDef, FieldDef, FieldType};
pub use store::LogStore;
pub use types::{EventType, GlobalSequenceId, InvalidEventType, InvalidTag, NewEvent, Tag};

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time verification: LogStore works as a generic bound
    fn _assert_logstore_bound<S: LogStore>(_store: &S) {}

    #[test]
    fn core_types_importable() {
        // Verify all public types are accessible from the crate root
        let _tag = Tag::new("course:c1").unwrap();
        let _gseq = GlobalSequenceId::new(1);
        let _new_event = NewEvent {
            event_type: EventType::from("test"),
            payload: serde_json::json!({}),
            tags: std::collections::HashSet::new(),
        };
        let _condition = AppendCondition {
            query: Query::all(),
            after: GlobalSequenceId::ZERO,
        };
        let _q = Query::match_tags([Tag::new("a").unwrap()]);
    }
}
