use crate::error::{AppendCondition, AppendError, StoreError};
use crate::store::LogStore;
use crate::types::{GlobalSequenceId, NewEvent, StreamId, StreamSequenceId};

/// Error from EventLog operations. Distinct from AppendError and StoreError.
/// Provides the user-facing error surface for append operations at the EventLog level.
#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
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

/// Orchestrates event log operations over a `LogStore` backend.
/// Provides a stable user-facing API that decouples callers from storage implementation details.
/// Future enrichment (constraints, observers, catch-up reads) can be added here without changing caller code.
#[derive(Clone)]
pub struct EventLog<S: LogStore> {
    store: S,
}

impl<S: LogStore> EventLog<S> {
    /// Construct an EventLog wrapping the given store.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Append events to a stream with an optional concurrency check.
    /// Returns the new global sequence ID on success.
    /// Maps AppendError variants to EventLogError — explicit match for auditable layer separation.
    pub async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, EventLogError> {
        self.store
            .append(stream_id, events, condition)
            .await
            .map_err(|e| match e {
                AppendError::ConcurrencyConflict {
                    stream_id,
                    expected,
                    actual,
                } => EventLogError::ConcurrencyConflict {
                    stream_id,
                    expected,
                    actual,
                },
                AppendError::StorageFailure(source) => EventLogError::StorageFailure(source),
            })
    }

    /// Read events from a specific stream, optionally bounded by sequence range.
    /// `from` is inclusive. `to` is inclusive if provided (None = read to end).
    /// Returns StoreError directly — same contract as the underlying store.
    pub async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
        to: Option<StreamSequenceId>,
    ) -> Result<S::EventStream, StoreError> {
        self.store.read_stream(stream_id, from, to).await
    }

    /// Read all events across all streams from a global sequence position.
    /// `from` is inclusive. Events returned sorted by global_sequence.
    /// Returns StoreError directly — same contract as the underlying store.
    pub async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<S::EventStream, StoreError> {
        self.store.read_all(from).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_error_concurrency_conflict_is_matchable() {
        let err = EventLogError::ConcurrencyConflict {
            stream_id: StreamId::new("orders").unwrap(),
            expected: StreamSequenceId::new(1),
            actual: StreamSequenceId::new(3),
        };
        match err {
            EventLogError::ConcurrencyConflict {
                stream_id,
                expected,
                actual,
            } => {
                assert_eq!(stream_id.as_str(), "orders");
                assert_eq!(expected, StreamSequenceId::new(1));
                assert_eq!(actual, StreamSequenceId::new(3));
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn event_log_error_storage_failure_wraps_source() {
        let source = std::io::Error::other("disk full");
        let err = EventLogError::StorageFailure(Box::new(source));
        match err {
            EventLogError::StorageFailure(_) => {}
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[test]
    fn event_log_error_display_includes_stream_id_and_versions() {
        let err = EventLogError::ConcurrencyConflict {
            stream_id: StreamId::new("orders").unwrap(),
            expected: StreamSequenceId::new(0),
            actual: StreamSequenceId::new(1),
        };
        let msg = err.to_string();
        assert!(msg.contains("orders"), "display should include stream_id");
        assert!(msg.contains('0'), "display should include expected version");
        assert!(msg.contains('1'), "display should include actual version");
    }
}
