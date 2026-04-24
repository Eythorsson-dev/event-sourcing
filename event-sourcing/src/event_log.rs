use crate::error::{AppendCondition, AppendError, SchemaConflictError, StoreError};
use crate::event::StoredEvent;
use crate::query::Query;
use crate::schema::EventSchemaDef;
use crate::schema_store::{EventSchemaStore, NoOpEventSchemaStore};
use crate::store::LogStore;
use crate::types::{EventType, GlobalSequenceId, NewEvent};
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

    #[error("schema conflict for event type '{event_type}'")]
    SchemaConflict {
        event_type: EventType,
        expected: EventSchemaDef,
        actual: EventSchemaDef,
    },
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
///
/// The second type parameter `E` is the `EventSchemaStore` implementation used for schema
/// tracking. Defaults to `NoOpEventSchemaStore` so existing `EventLog<S>` call sites compile
/// unchanged (D-09 from Phase 4 context).
#[derive(Clone)]
pub struct EventLog<S: LogStore, E: EventSchemaStore = NoOpEventSchemaStore> {
    store: S,
    schema_store: E,
}

impl<S: LogStore, E: EventSchemaStore> EventLog<S, E> {
    /// Construct an EventLog wrapping the given store and a custom schema store.
    /// Use `EventLog::new(store)` for the common case where schema tracking is not needed.
    pub fn with_schema_store(store: S, schema_store: E) -> Self {
        Self {
            store,
            schema_store,
        }
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
                AppendError::SchemaConflict {
                    event_type,
                    expected,
                    actual,
                } => EventLogError::SchemaConflict {
                    event_type,
                    expected,
                    actual,
                },
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

    /// Startup schema validation check (D-06, D-08).
    ///
    /// Compares all `EventSchemaDef`s in `code_schemas` against every persisted schema in the
    /// schema store. Returns the first `SchemaConflictError::Conflict` found, or `Ok(())` if
    /// all schemas are compatible (or no persisted schemas exist yet).
    ///
    /// This is explicit and opt-in — the library never panics. The application calls this at
    /// startup and handles the result.
    pub async fn validate_schemas(
        &self,
        code_schemas: impl IntoIterator<Item = EventSchemaDef>,
    ) -> Result<(), SchemaConflictError> {
        let persisted = self
            .schema_store
            .fetch_all()
            .await
            .map_err(|e| SchemaConflictError::StorageFailure(e.to_string()))?;

        let persisted_map: std::collections::HashMap<String, EventSchemaDef> = persisted
            .into_iter()
            .map(|s| (s.event_type.to_string(), s))
            .collect();

        for code_schema in code_schemas {
            if let Some(persisted_schema) = persisted_map.get(code_schema.event_type.as_str()) {
                if !persisted_schema.is_compatible_with(&code_schema) {
                    return Err(SchemaConflictError::Conflict {
                        event_type: code_schema.event_type.clone(),
                        expected: persisted_schema.clone(),
                        actual: code_schema,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Convenience constructors for `EventLog<S, NoOpEventSchemaStore>`.
/// The single-argument `new(store)` constructor preserves backward compatibility —
/// existing call sites that only pass a store continue to compile unchanged (D-09).
impl<S: LogStore> EventLog<S, NoOpEventSchemaStore> {
    /// Construct an EventLog with no schema tracking.
    /// Backward-compatible single-argument constructor.
    pub fn new(store: S) -> Self {
        Self {
            store,
            schema_store: NoOpEventSchemaStore,
        }
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
    use crate::schema::{EventSchemaDef, FieldDef, FieldType};
    use crate::types::{EventType, GlobalSequenceId};
    use std::collections::HashSet;
    use std::time::SystemTime;

    fn make_stored_event(seq: u64, event_type: &str) -> StoredEvent {
        StoredEvent {
            global_sequence: GlobalSequenceId::new(seq),
            event_type: EventType::from(event_type),
            payload: serde_json::json!({}),
            tags: HashSet::new(),
            timestamp: SystemTime::now(),
        }
    }

    fn make_schema(event_type: &str, fields: Vec<FieldDef>) -> EventSchemaDef {
        EventSchemaDef {
            event_type: EventType::from(event_type),
            fields,
        }
    }

    fn field(name: &str) -> FieldDef {
        FieldDef {
            name: name.to_owned(),
            field_type: FieldType::String,
            optional: false,
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
        assert_eq!(result.unwrap().event_type.as_str(), "OrderPlaced");
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
        assert_eq!(opt.unwrap().event_type.as_str(), "E1");
    }

    #[tokio::test]
    async fn first_empty() {
        let stream = futures::stream::iter(vec![] as Vec<Result<StoredEvent, StoreError>>);
        let result = stream.first().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Minimal mock LogStore for unit tests that don't need real storage.
    use crate::error::AppendCondition;
    use crate::store::LogStore;
    use futures::stream;
    use futures::stream::Iter;

    struct NullLogStore;

    impl LogStore for NullLogStore {
        type EventStream = Iter<std::vec::IntoIter<Result<StoredEvent, StoreError>>>;

        async fn append(
            &self,
            _events: Vec<NewEvent>,
            _condition: Option<AppendCondition>,
        ) -> Result<GlobalSequenceId, AppendError> {
            Ok(GlobalSequenceId::ZERO)
        }

        async fn query(
            &self,
            _query: Query,
            _from: GlobalSequenceId,
        ) -> Result<Self::EventStream, StoreError> {
            Ok(stream::iter(vec![]))
        }

        async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError> {
            Ok(GlobalSequenceId::ZERO)
        }
    }

    // validate_schemas tests

    #[tokio::test]
    async fn validate_schemas_ok_when_no_persisted() {
        let log = EventLog::with_schema_store(NullLogStore, NoOpEventSchemaStore);
        let schema_a = make_schema("OrderPlaced", vec![field("id")]);
        let result = log.validate_schemas([schema_a]).await;
        assert!(result.is_ok());
    }

    #[test]
    fn event_log_compiles_with_default_type_param() {
        // Verify EventLog<S> (one type param, default E) compiles with single-arg new.
        let _log: EventLog<NullLogStore> = EventLog::new(NullLogStore);
    }
}
