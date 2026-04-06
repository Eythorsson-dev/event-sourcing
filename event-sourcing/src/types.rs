use std::fmt;

/// Error returned when an invalid stream ID is provided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStreamId;

impl fmt::Display for InvalidStreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "stream ID must not be empty")
    }
}

impl std::error::Error for InvalidStreamId {}

/// Identifies an event stream. Always non-empty. Per D-01: explicit construction only, no From/Into.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(String);

impl StreamId {
    /// Creates a new StreamId. Returns Err(InvalidStreamId) if the string is empty. Per D-02.
    pub fn new(id: impl Into<String>) -> Result<StreamId, InvalidStreamId> {
        let id = id.into();
        if id.is_empty() {
            return Err(InvalidStreamId);
        }
        Ok(StreamId(id))
    }

    /// Returns the stream ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Global sequence ID across all streams. Monotonically increasing. Per D-05, D-06: starts at 1, zero = no events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalSequenceId(u64);

impl GlobalSequenceId {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for GlobalSequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Per-stream sequence number. Per D-05, D-06: starts at 1, zero = no events in stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamSequenceId(u64);

impl StreamSequenceId {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for StreamSequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An event submitted for appending. No sequence IDs or timestamp — those are assigned by the store.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_id_new_valid() {
        let sid = StreamId::new("orders").unwrap();
        assert_eq!(sid.as_str(), "orders");
    }

    #[test]
    fn stream_id_new_empty_returns_error() {
        let result = StreamId::new("");
        assert_eq!(result, Err(InvalidStreamId));
    }

    #[test]
    fn global_sequence_zero_is_zero() {
        assert_eq!(GlobalSequenceId::ZERO.get(), 0);
    }

    #[test]
    fn global_sequence_ord() {
        let a = GlobalSequenceId::new(1);
        let b = GlobalSequenceId::new(2);
        assert!(a < b);
    }

    #[test]
    fn stream_sequence_zero_is_zero() {
        assert_eq!(StreamSequenceId::ZERO.get(), 0);
    }

    #[test]
    fn sequence_ids_are_distinct_types() {
        // Compile-time check: GlobalSequenceId and StreamSequenceId cannot be mixed.
        let _g = GlobalSequenceId::new(1);
        let _s = StreamSequenceId::new(1);
        // If these were the same type, the following would compile — they don't:
        // let _bad: GlobalSequenceId = _s; // would fail to compile
    }

    #[test]
    fn new_event_can_be_constructed() {
        let event = NewEvent {
            event_type: "OrderPlaced".to_string(),
            payload: serde_json::json!({"order_id": "123"}),
        };
        assert_eq!(event.event_type, "OrderPlaced");
    }
}
