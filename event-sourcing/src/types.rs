use std::fmt;

/// Newtype for stream identity. Explicit construction only — no From/Into (D-01).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(String);

impl StreamId {
    /// Returns `Err` if the string is empty (D-02).
    pub fn new(value: impl Into<String>) -> Result<Self, EmptyStreamIdError> {
        let s = value.into();
        if s.is_empty() {
            Err(EmptyStreamIdError)
        } else {
            Ok(StreamId(s))
        }
    }

    /// Borrow the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when constructing a `StreamId` with an empty string.
#[derive(Debug, Clone, thiserror::Error)]
#[error("StreamId cannot be empty")]
pub struct EmptyStreamIdError;

/// Global monotonically increasing sequence across all streams.
/// Starts from 1; 0 means "no events seen" (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalSequenceId(u64);

impl GlobalSequenceId {
    pub fn new(value: u64) -> Self {
        GlobalSequenceId(value)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for GlobalSequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Per-stream sequence number.
/// Starts from 1; 0 means "beginning of time" (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamSequenceId(u64);

impl StreamSequenceId {
    pub fn new(value: u64) -> Self {
        StreamSequenceId(value)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for StreamSequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
