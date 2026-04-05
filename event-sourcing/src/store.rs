use futures_core::Stream;

use crate::error::{AppendError, StoreError};
use crate::event::{NewEvent, StoredEvent};
use crate::types::{GlobalSequenceId, StreamId, StreamSequenceId};

/// The condition for an optimistic concurrency check on append.
pub enum AppendCondition {
    /// Append only if stream is at exactly this version.
    ExpectedVersion(StreamSequenceId),
    /// Append regardless of current stream version.
    Any,
}

/// Storage backend trait. Implementations live in separate crates (D-16).
/// Uses native async fn in trait with static dispatch (D-11).
/// Send bounds on futures are not expressible with native async fn in trait;
/// callers use static dispatch (`impl LogStore`) where Send is required.
#[allow(async_fn_in_trait)]
pub trait LogStore: Send + Sync {
    /// The stream type returned by read operations (D-12).
    type EventStream: Stream<Item = Result<StoredEvent, StoreError>> + Send;

    /// Append events to a stream with an optimistic concurrency condition.
    /// Returns the `GlobalSequenceId` of the last appended event.
    async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, AppendError>;

    /// Read all events for a stream, optionally from a starting sequence (D-13).
    async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
    ) -> Result<Self::EventStream, StoreError>;

    /// Read all events across all streams from a global sequence position.
    async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError>;

    /// Get the current global sequence ID (highest assigned).
    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError>;

    /// Get the current version of a specific stream.
    async fn stream_version(
        &self,
        stream_id: &StreamId,
    ) -> Result<StreamSequenceId, StoreError>;
}
