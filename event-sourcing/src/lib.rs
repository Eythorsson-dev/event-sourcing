//! Event sourcing core library — types, traits, and contracts.
//!
//! This crate defines the foundational types and the `LogStore` trait that all
//! storage implementations build on. No I/O — just contracts.

pub mod error;
pub mod event;
pub mod event_log;
pub mod store;
pub mod types;

// Re-export primary public API at crate root for ergonomic imports
pub use error::{AppendCondition, AppendError, StoreError};
pub use event::StoredEvent;
pub use event_log::{EventLog, EventLogError};
pub use store::LogStore;
pub use types::{GlobalSequenceId, InvalidStreamId, NewEvent, StreamId, StreamSequenceId};

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time verification: LogStore works as a generic bound
    fn _assert_logstore_bound<S: LogStore>(_store: &S) {}

    #[test]
    fn core_types_importable() {
        // Verify all public types are accessible from the crate root
        let _sid = StreamId::new("test").unwrap();
        let _gseq = GlobalSequenceId::new(1);
        let _sseq = StreamSequenceId::new(1);
        let _new_event = NewEvent {
            event_type: "test".to_string(),
            payload: serde_json::json!({}),
        };
        let _condition = AppendCondition::Any;
        let _condition2 = AppendCondition::ExpectedVersion(StreamSequenceId::new(1));
    }
}
