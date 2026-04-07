use crate::error::{AppendCondition, AppendError, StoreError};
use crate::event::StoredEvent;
use crate::query::Query;
use crate::store::LogStore;
use crate::types::{GlobalSequenceId, NewEvent};
use futures_core::Stream;

/// Error from EventLog operations. Distinct from AppendError and StoreError.
/// Provides the user-facing error surface for append operations at the EventLog level.
#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
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

/// Error from consuming an event stream with single() or similar operations.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("stream was empty, expected exactly one event")]
    Empty,
    #[error("stream had multiple events, expected exactly one")]
    Multiple,
    #[error("stream error: {0}")]
    Store(#[from] StoreError),
}

/// Orchestrates event log operations over a `LogStore` backend.
/// Provides a stable user-facing API that decouples callers from storage implementation details.
#[derive(Clone)]
pub struct EventLog<S: LogStore> {
    store: S,
}

impl<S: LogStore> EventLog<S> {
    /// Construct an EventLog wrapping the given store.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Append events with an optional concurrency check.
    /// Returns the new global sequence ID on success.
    /// Maps AppendError variants to EventLogError — explicit match for auditable layer separation.
    pub async fn append(
        &self,
        events: Vec<NewEvent>,
        condition: Option<AppendCondition>,
    ) -> Result<GlobalSequenceId, EventLogError> {
        self.store
            .append(events, condition)
            .await
            .map_err(|e| match e {
                AppendError::ConcurrencyConflict {
                    conflicting_position,
                    checked_after,
                } => EventLogError::ConcurrencyConflict {
                    conflicting_position,
                    checked_after,
                },
                AppendError::StorageFailure(source) => EventLogError::StorageFailure(source),
            })
    }

    /// Query events matching the given filter from a global sequence position.
    /// Returns StoreError directly — same contract as the underlying store.
    pub async fn query(
        &self,
        query: Query,
        from: GlobalSequenceId,
    ) -> Result<S::EventStream, StoreError> {
        self.store.query(query, from).await
    }

    /// Get the current highest global sequence ID.
    pub async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError> {
        self.store.current_sequence().await
    }
}

/// Extension trait providing ergonomic consumption methods over event streams.
/// Follows the `futures::StreamExt` pattern — blanket-impl over any compatible stream.
pub trait EventStreamExt: Stream<Item = Result<StoredEvent, StoreError>> {
    /// Consume the stream expecting exactly one event.
    /// Returns `StreamError::Empty` if no events, `StreamError::Multiple` if more than one.
    fn single(self) -> impl std::future::Future<Output = Result<StoredEvent, StreamError>> + Send
    where
        Self: Sized + Unpin + Send;

    /// Consume the stream returning the first event, or `None` if empty.
    fn first(
        self,
    ) -> impl std::future::Future<Output = Result<Option<StoredEvent>, StoreError>> + Send
    where
        Self: Sized + Unpin + Send;
}

/// Poll a stream for its next item using the futures_core low-level API.
fn poll_next_unpin<S>(
    stream: &mut S,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<Option<S::Item>>
where
    S: Stream + Unpin,
{
    use std::pin::Pin;
    Pin::new(stream).poll_next(cx)
}

/// Async helper: get the next item from an Unpin stream.
async fn stream_next<S>(stream: &mut S) -> Option<S::Item>
where
    S: Stream + Unpin,
{
    std::future::poll_fn(|cx| poll_next_unpin(stream, cx)).await
}

#[allow(clippy::manual_async_fn)]
impl<S> EventStreamExt for S
where
    S: Stream<Item = Result<StoredEvent, StoreError>> + Sized + Unpin + Send,
{
    fn single(
        mut self,
    ) -> impl std::future::Future<Output = Result<StoredEvent, StreamError>> + Send
    where
        Self: Sized + Unpin + Send,
    {
        async move {
            let first = stream_next(&mut self).await;
            match first {
                None => Err(StreamError::Empty),
                Some(Err(e)) => Err(StreamError::Store(e)),
                Some(Ok(event)) => {
                    let second = stream_next(&mut self).await;
                    match second {
                        None => Ok(event),
                        Some(Err(e)) => Err(StreamError::Store(e)),
                        Some(Ok(_)) => Err(StreamError::Multiple),
                    }
                }
            }
        }
    }

    fn first(
        mut self,
    ) -> impl std::future::Future<Output = Result<Option<StoredEvent>, StoreError>> + Send
    where
        Self: Sized + Unpin + Send,
    {
        async move {
            match stream_next(&mut self).await {
                None => Ok(None),
                Some(Err(e)) => Err(e),
                Some(Ok(event)) => Ok(Some(event)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GlobalSequenceId;
    use std::collections::HashSet;
    use std::time::SystemTime;

    fn make_stored_event(seq: u64, event_type: &str) -> StoredEvent {
        StoredEvent {
            global_sequence: GlobalSequenceId::new(seq),
            event_type: event_type.to_string(),
            payload: serde_json::json!({}),
            tags: HashSet::new(),
            timestamp: SystemTime::now(),
        }
    }

    #[test]
    fn event_log_error_concurrency_conflict_is_matchable() {
        let err = EventLogError::ConcurrencyConflict {
            conflicting_position: GlobalSequenceId::new(5),
            checked_after: GlobalSequenceId::ZERO,
        };
        match err {
            EventLogError::ConcurrencyConflict {
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
    fn event_log_error_storage_failure_wraps_source() {
        let source = std::io::Error::other("disk full");
        let err = EventLogError::StorageFailure(Box::new(source));
        match err {
            EventLogError::StorageFailure(_) => {}
            other => panic!("unexpected variant: {:?}", other),
        }
    }

    #[tokio::test]
    async fn single_one_event() {
        let event = make_stored_event(1, "OrderPlaced");
        let stream = futures::stream::iter(vec![Ok::<StoredEvent, StoreError>(event.clone())]);
        let result = stream.single().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().event_type, "OrderPlaced");
    }

    #[tokio::test]
    async fn single_empty() {
        let stream = futures::stream::iter(vec![] as Vec<Result<StoredEvent, StoreError>>);
        let result = stream.single().await;
        assert!(matches!(result, Err(StreamError::Empty)));
    }

    #[tokio::test]
    async fn single_multiple() {
        let e1 = make_stored_event(1, "E1");
        let e2 = make_stored_event(2, "E2");
        let stream = futures::stream::iter(vec![
            Ok::<StoredEvent, StoreError>(e1),
            Ok::<StoredEvent, StoreError>(e2),
        ]);
        let result = stream.single().await;
        assert!(matches!(result, Err(StreamError::Multiple)));
    }

    #[tokio::test]
    async fn first_non_empty() {
        let e1 = make_stored_event(1, "E1");
        let e2 = make_stored_event(2, "E2");
        let stream = futures::stream::iter(vec![
            Ok::<StoredEvent, StoreError>(e1),
            Ok::<StoredEvent, StoreError>(e2),
        ]);
        let result = stream.first().await;
        assert!(result.is_ok());
        let opt = result.unwrap();
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().event_type, "E1");
    }

    #[tokio::test]
    async fn first_empty() {
        let stream = futures::stream::iter(vec![] as Vec<Result<StoredEvent, StoreError>>);
        let result = stream.first().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
