use crate::types::{StreamId, StreamSequenceId};

/// Condition for optimistic concurrency on append. Used by LogStore::append.
#[derive(Debug, Clone)]
pub enum AppendCondition {
    /// Stream must be at exactly this version.
    ExpectedVersion(StreamSequenceId),
    /// No version check — append unconditionally.
    Any,
}

/// Error from append operations. Distinguishes ConcurrencyConflict from StorageFailure.
/// Callers can match on variants (LOG-07).
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error(
        "concurrency conflict: stream {stream_id} expected version {expected}, found {actual}"
    )]
    ConcurrencyConflict {
        stream_id: StreamId,
        expected: StreamSequenceId,
        actual: StreamSequenceId,
    },

    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Error from read operations (connection failures, mid-stream errors). Separate from AppendError.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage error: {0}")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_condition_any_can_be_constructed() {
        let _cond = AppendCondition::Any;
    }

    #[test]
    fn append_condition_expected_version_can_be_constructed() {
        let _cond = AppendCondition::ExpectedVersion(StreamSequenceId::new(2));
    }

    #[test]
    fn append_error_concurrency_conflict_can_be_matched() {
        let err = AppendError::ConcurrencyConflict {
            stream_id: StreamId::new("orders").unwrap(),
            expected: StreamSequenceId::new(1),
            actual: StreamSequenceId::new(2),
        };
        match err {
            AppendError::ConcurrencyConflict {
                expected, actual, ..
            } => {
                assert_eq!(expected, StreamSequenceId::new(1));
                assert_eq!(actual, StreamSequenceId::new(2));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn store_error_storage_can_be_matched() {
        let err = StoreError::Storage(Box::new(std::io::Error::other("test")));
        match err {
            StoreError::Storage(_) => {}
        }
    }
}
