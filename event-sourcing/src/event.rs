use crate::types::{GlobalSequenceId, StreamId, StreamSequenceId};
use std::time::SystemTime;

/// A stored event with all metadata assigned by the log store at append time.
/// Events are immutable once stored. No mutation methods.
/// Per D-04: no per-event UUID. Identified by (StreamId, StreamSequenceId) or GlobalSequenceId.
/// Per D-07: serde_json::Value payload (heterogeneous streams).
/// Per D-08: event_type String for filtering without deserialization.
/// Per D-09: SystemTime timestamp set by store at append time.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub global_sequence: GlobalSequenceId,
    pub stream_id: StreamId,
    pub stream_sequence: StreamSequenceId,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_event_can_be_constructed() {
        let event = StoredEvent {
            global_sequence: GlobalSequenceId::new(1),
            stream_id: StreamId::new("orders").unwrap(),
            stream_sequence: StreamSequenceId::new(1),
            event_type: "OrderPlaced".to_string(),
            payload: serde_json::json!({"order_id": "123"}),
            timestamp: SystemTime::now(),
        };
        assert_eq!(event.event_type, "OrderPlaced");
        assert_eq!(event.global_sequence.get(), 1);
        assert_eq!(event.stream_sequence.get(), 1);
    }
}
