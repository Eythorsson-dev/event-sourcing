use crate::error::{AppendCondition, AppendError, StoreError};
use crate::event::StoredEvent;
use crate::types::{GlobalSequenceId, NewEvent, StreamId, StreamSequenceId};

/// Trait for event log storage backends.
/// Uses native async fn in trait via `-> impl Future + Send` syntax (stable since Rust 1.75).
/// Supports static dispatch (`S: LogStore`) but NOT `dyn LogStore` — which is the explicit design decision.
///
/// Implementors: InMemoryLogStore (Phase 2), SqliteLogStore (Phase 8), or user-provided (STOR-01).
pub trait LogStore: Send + Sync {
    /// Stream of stored events returned by read operations.
    /// Yields Result to allow mid-stream errors.
    type EventStream: futures_core::Stream<Item = Result<StoredEvent, StoreError>> + Send;

    /// Append events to a stream with an optional concurrency check.
    /// Returns the new global sequence ID on success.
    /// Returns AppendError::ConcurrencyConflict if the stream has advanced past the expected version.
    fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> impl std::future::Future<Output = Result<GlobalSequenceId, AppendError>> + Send;

    /// Read events from a specific stream, optionally bounded by sequence range.
    /// `from` is inclusive. `to` is inclusive if provided (None = read to end).
    /// Returns Ok with an empty stream if the stream has never been written to.
    fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
        to: Option<StreamSequenceId>,
    ) -> impl std::future::Future<Output = Result<Self::EventStream, StoreError>> + Send;

    /// Read all events across all streams from a global sequence position.
    /// `from` is inclusive. Events returned sorted by global_sequence.
    fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> impl std::future::Future<Output = Result<Self::EventStream, StoreError>> + Send;

    /// Get the current highest global sequence ID. Returns ZERO if no events exist.
    fn current_sequence(
        &self,
    ) -> impl std::future::Future<Output = Result<GlobalSequenceId, StoreError>> + Send;

    /// Get the current version (highest stream sequence ID) for a specific stream.
    /// Returns ZERO if the stream has no events.
    fn stream_version(
        &self,
        stream_id: &StreamId,
    ) -> impl std::future::Future<Output = Result<StreamSequenceId, StoreError>> + Send;
}
