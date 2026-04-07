use crate::types::{GlobalSequenceId, Tag};
use std::collections::HashSet;
use std::time::SystemTime;

/// A stored event with all metadata assigned by the log store at append time.
/// Events are immutable once stored. No mutation methods.
/// Identified by global_sequence. Tags classify the event for query-based reads.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub global_sequence: GlobalSequenceId,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub tags: HashSet<Tag>,
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_event_can_be_constructed() {
        let event = StoredEvent {
            global_sequence: GlobalSequenceId::new(1),
            event_type: "OrderPlaced".to_string(),
            payload: serde_json::json!({"order_id": "123"}),
            tags: [Tag::new("order:o1").unwrap()].into_iter().collect(),
            timestamp: SystemTime::now(),
        };
        assert_eq!(event.event_type, "OrderPlaced");
        assert_eq!(event.global_sequence.get(), 1);
        assert_eq!(event.tags.len(), 1);
    }
}
