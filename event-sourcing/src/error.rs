use crate::query::Query;
use crate::types::GlobalSequenceId;

/// Condition for optimistic concurrency on append.
/// Semantics: fail if any events matching `query` have been appended after position `after`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppendCondition {
    pub query: Query,
    pub after: GlobalSequenceId,
}

/// Error from append operations. Distinguishes ConcurrencyConflict from StorageFailure.
/// Callers can match on variants.
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error(
        "concurrency conflict: event at position {conflicting_position} matched query after position {checked_after}"
    )]
    ConcurrencyConflict {
        conflicting_position: GlobalSequenceId,
        checked_after: GlobalSequenceId,
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
    use crate::query::Query;
    use crate::types::GlobalSequenceId;

    #[test]
    fn append_condition_can_be_constructed() {
        let _cond = AppendCondition {
            query: Query::all(),
            after: GlobalSequenceId::ZERO,
        };
    }

    #[test]
    fn append_condition_serde_roundtrip() {
        let original = AppendCondition {
            query: Query::all(),
            after: GlobalSequenceId::ZERO,
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: AppendCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn append_error_concurrency_conflict_can_be_matched() {
        let err = AppendError::ConcurrencyConflict {
            conflicting_position: GlobalSequenceId::new(5),
            checked_after: GlobalSequenceId::ZERO,
        };
        match err {
            AppendError::ConcurrencyConflict {
                conflicting_position,
                checked_after,
            } => {
                assert_eq!(conflicting_position, GlobalSequenceId::new(5));
                assert_eq!(checked_after, GlobalSequenceId::ZERO);
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
