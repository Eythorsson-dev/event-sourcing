use crate::types::{GlobalSequenceId, StreamId, StreamSequenceId};

/// An event as stored in the log. Immutable after creation (LOG-01).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Global position across all streams (LOG-03, D-04).
    pub global_sequence_id: GlobalSequenceId,
    /// Position within its stream (LOG-02).
    pub stream_sequence_id: StreamSequenceId,
    /// Which stream this event belongs to.
    pub stream_id: StreamId,
    /// Discriminator for filtering/routing without deserializing payload (D-08).
    pub event_type: String,
    /// The event payload as opaque JSON (D-07).
    pub payload: serde_json::Value,
    /// Timestamp set by the store at append time (D-09).
    pub timestamp: std::time::SystemTime,
}

/// Event data before it is stored (no sequence IDs or timestamp yet).
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}
