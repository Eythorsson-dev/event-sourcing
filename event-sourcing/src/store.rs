use crate::error::{AppendCondition, AppendError, StoreError};
use crate::event::StoredEvent;
use crate::query::Query;
use crate::types::{GlobalSequenceId, NewEvent};

/// Trait for event log storage backends.
/// Uses native async fn in trait via `-> impl Future + Send` syntax (stable since Rust 1.75).
/// Supports static dispatch (`S: LogStore`) but NOT `dyn LogStore` — which is the explicit design decision.
///
/// Implementors: InMemoryLogStore (Phase 2), SqliteLogStore (Phase 8), or user-provided.
pub trait LogStore: Send + Sync {
    /// Stream of stored events returned by query operations.
    /// Yields Result to allow mid-stream errors.
    type EventStream: futures_core::Stream<Item = Result<StoredEvent, StoreError>> + Send;

    /// Append events with an optional concurrency check.
    /// Returns the new highest global sequence ID on success.
    /// Returns AppendError::ConcurrencyConflict if any event matching the condition's query
    /// was appended after the specified position.
    fn append(
        &self,
        events: Vec<NewEvent>,
        condition: Option<AppendCondition>,
    ) -> impl std::future::Future<Output = Result<GlobalSequenceId, AppendError>> + Send;

    /// Query events matching the given filter from a global sequence position.
    /// `from` is inclusive. Returns a lazy async stream sorted by global_sequence.
    fn query(
        &self,
        query: Query,
        from: GlobalSequenceId,
    ) -> impl std::future::Future<Output = Result<Self::EventStream, StoreError>> + Send;

    /// Get the current highest global sequence ID. Returns ZERO if no events exist.
    fn current_sequence(
        &self,
    ) -> impl std::future::Future<Output = Result<GlobalSequenceId, StoreError>> + Send;
}
