use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use event_sourcing::{
    AppendCondition, AppendError, GlobalSequenceId, LogStore, NewEvent, StoredEvent, StoreError,
    StreamId, StreamSequenceId,
};
use futures::stream::{self, Iter};
use tokio::sync::RwLock;

/// The concrete event stream type returned by InMemoryLogStore read operations.
pub type InMemoryEventStream = Iter<std::vec::IntoIter<Result<StoredEvent, StoreError>>>;

struct InnerState {
    streams: HashMap<StreamId, Vec<StoredEvent>>,
    /// Next global sequence to assign (1-based; starts at 1). Incremented under write lock.
    next_global_seq: u64,
}

impl InnerState {
    fn new() -> Self {
        Self {
            streams: HashMap::new(),
            next_global_seq: 1,
        }
    }

    /// Returns the current highest global sequence ID, or ZERO if no events exist.
    fn current_sequence(&self) -> GlobalSequenceId {
        if self.next_global_seq == 1 {
            GlobalSequenceId::ZERO
        } else {
            GlobalSequenceId::new(self.next_global_seq - 1)
        }
    }
}

/// In-memory implementation of `LogStore`. Clone shares the same backing state (Arc-backed).
///
/// Designed as the reference implementation and test harness for all higher-level phases.
/// All state mutations are protected by a `tokio::sync::RwLock`. Global sequence IDs are
/// allocated under the write lock to prevent gaps on version conflicts.
#[derive(Clone)]
pub struct InMemoryLogStore {
    inner: Arc<RwLock<InnerState>>,
}

impl Default for InMemoryLogStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerState::new())),
        }
    }
}

impl InMemoryLogStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LogStore for InMemoryLogStore {
    type EventStream = InMemoryEventStream;

    async fn append(
        &self,
        stream_id: &StreamId,
        events: Vec<NewEvent>,
        condition: AppendCondition,
    ) -> Result<GlobalSequenceId, AppendError> {
        if events.is_empty() {
            // Nothing to append — return current sequence (no change).
            let state = self.inner.read().await;
            return Ok(state.current_sequence());
        }

        let mut state = self.inner.write().await;

        // Determine current stream version (0 if stream doesn't exist).
        let current_version = state
            .streams
            .get(stream_id)
            .map(|v| StreamSequenceId::new(v.len() as u64))
            .unwrap_or(StreamSequenceId::ZERO);

        // Check optimistic concurrency condition BEFORE allocating any sequence IDs.
        if let AppendCondition::ExpectedVersion(expected) = condition {
            if expected != current_version {
                return Err(AppendError::ConcurrencyConflict {
                    stream_id: stream_id.clone(),
                    expected,
                    actual: current_version,
                });
            }
        }

        // All events pass the version check — build stored events under the write lock.
        // We build the vec first (computing sequences from current state), then push all at once.
        let initial_stream_len = state
            .streams
            .get(stream_id)
            .map(|v| v.len() as u64)
            .unwrap_or(0);

        let mut stored_events = Vec::with_capacity(events.len());
        let mut last_global_seq = GlobalSequenceId::ZERO;

        for (i, new_event) in events.into_iter().enumerate() {
            let global_seq = GlobalSequenceId::new(state.next_global_seq);
            state.next_global_seq += 1;

            let stream_sequence = StreamSequenceId::new(initial_stream_len + i as u64 + 1);

            let stored = StoredEvent {
                global_sequence: global_seq,
                stream_id: stream_id.clone(),
                stream_sequence,
                event_type: new_event.event_type,
                payload: new_event.payload,
                timestamp: SystemTime::now(),
            };

            stored_events.push(stored);
            last_global_seq = global_seq;
        }

        // Push all stored events into the stream atomically.
        let stream_vec = state.streams.entry(stream_id.clone()).or_default();
        stream_vec.extend(stored_events);

        Ok(last_global_seq)
    }

    async fn read_stream(
        &self,
        stream_id: &StreamId,
        from: StreamSequenceId,
        to: Option<StreamSequenceId>,
    ) -> Result<Self::EventStream, StoreError> {
        let state = self.inner.read().await;

        let events: Vec<Result<StoredEvent, StoreError>> = state
            .streams
            .get(stream_id)
            .map(|v| {
                v.iter()
                    .filter(|e| {
                        e.stream_sequence >= from
                            && to.is_none_or(|end| e.stream_sequence <= end)
                    })
                    .cloned()
                    .map(Ok)
                    .collect()
            })
            .unwrap_or_default();

        // Drop the lock before returning — do not hold across await points.
        drop(state);

        Ok(stream::iter(events))
    }

    async fn read_all(
        &self,
        from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError> {
        let state = self.inner.read().await;

        let mut all_events: Vec<StoredEvent> = state
            .streams
            .values()
            .flat_map(|v| v.iter().cloned())
            .filter(|e| e.global_sequence >= from)
            .collect();

        // Sort by global sequence for deterministic ordering across streams.
        all_events.sort_by_key(|e| e.global_sequence);

        let results: Vec<Result<StoredEvent, StoreError>> =
            all_events.into_iter().map(Ok).collect();

        // Drop the lock before returning.
        drop(state);

        Ok(stream::iter(results))
    }

    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError> {
        let state = self.inner.read().await;
        Ok(state.current_sequence())
    }

    async fn stream_version(
        &self,
        stream_id: &StreamId,
    ) -> Result<StreamSequenceId, StoreError> {
        let state = self.inner.read().await;
        let version = state
            .streams
            .get(stream_id)
            .map(|v| StreamSequenceId::new(v.len() as u64))
            .unwrap_or(StreamSequenceId::ZERO);
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn test_event(event_type: &str) -> NewEvent {
        NewEvent {
            event_type: event_type.to_string(),
            payload: serde_json::json!({"test": true}),
        }
    }

    fn stream_id(name: &str) -> StreamId {
        StreamId::new(name).unwrap()
    }

    #[tokio::test]
    async fn test_append_and_read_stream() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![
                    test_event("OrderPlaced"),
                    test_event("OrderConfirmed"),
                    test_event("OrderShipped"),
                ],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let event_stream = store
            .read_stream(&sid, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(1)
        );
        assert_eq!(
            events[1].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(2)
        );
        assert_eq!(
            events[2].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(3)
        );
        assert_eq!(events[0].as_ref().unwrap().event_type, "OrderPlaced");
        assert_eq!(events[1].as_ref().unwrap().event_type, "OrderConfirmed");
        assert_eq!(events[2].as_ref().unwrap().event_type, "OrderShipped");
    }

    #[tokio::test]
    async fn test_read_stream_range() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![
                    test_event("E1"),
                    test_event("E2"),
                    test_event("E3"),
                    test_event("E4"),
                    test_event("E5"),
                ],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let event_stream = store
            .read_stream(
                &sid,
                StreamSequenceId::new(2),
                Some(StreamSequenceId::new(4)),
            )
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(2)
        );
        assert_eq!(
            events[1].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(3)
        );
        assert_eq!(
            events[2].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(4)
        );
    }

    #[tokio::test]
    async fn test_read_stream_from_only() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![
                    test_event("E1"),
                    test_event("E2"),
                    test_event("E3"),
                    test_event("E4"),
                    test_event("E5"),
                ],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let event_stream = store
            .read_stream(&sid, StreamSequenceId::new(3), None)
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(3)
        );
        assert_eq!(
            events[2].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(5)
        );
    }

    #[tokio::test]
    async fn test_global_sequence_monotonic() {
        let store = InMemoryLogStore::new();
        let sa = stream_id("a");
        let sb = stream_id("b");

        store
            .append(&sa, vec![test_event("A1")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sb, vec![test_event("B1")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sa, vec![test_event("A2")], AppendCondition::Any)
            .await
            .unwrap();

        let event_stream = store.read_all(GlobalSequenceId::new(1)).await.unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        let g1 = events[0].as_ref().unwrap().global_sequence;
        let g2 = events[1].as_ref().unwrap().global_sequence;
        let g3 = events[2].as_ref().unwrap().global_sequence;
        assert!(g1 < g2);
        assert!(g2 < g3);
    }

    #[tokio::test]
    async fn test_read_all_global_order() {
        let store = InMemoryLogStore::new();
        let sa = stream_id("stream-a");
        let sb = stream_id("stream-b");

        store
            .append(&sa, vec![test_event("A1"), test_event("A2")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sb, vec![test_event("B1"), test_event("B2")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sa, vec![test_event("A3")], AppendCondition::Any)
            .await
            .unwrap();

        let event_stream = store.read_all(GlobalSequenceId::new(1)).await.unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 5);
        // Verify they are in global order
        for i in 0..events.len() - 1 {
            let curr = events[i].as_ref().unwrap().global_sequence;
            let next = events[i + 1].as_ref().unwrap().global_sequence;
            assert!(curr < next, "expected global order at index {}", i);
        }
    }

    #[tokio::test]
    async fn test_read_all_from() {
        let store = InMemoryLogStore::new();
        let sa = stream_id("stream-a");
        let sb = stream_id("stream-b");

        // Append 5 events across streams
        store
            .append(&sa, vec![test_event("A1"), test_event("A2")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sb, vec![test_event("B1"), test_event("B2"), test_event("B3")], AppendCondition::Any)
            .await
            .unwrap();

        let event_stream = store.read_all(GlobalSequenceId::new(3)).await.unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        for e in &events {
            assert!(e.as_ref().unwrap().global_sequence >= GlobalSequenceId::new(3));
        }
    }

    #[tokio::test]
    async fn test_clone_shares_state() {
        let store = InMemoryLogStore::new();
        let clone = store.clone();
        let sid = stream_id("orders");

        // Append via clone
        clone
            .append(&sid, vec![test_event("OrderPlaced")], AppendCondition::Any)
            .await
            .unwrap();

        // Read via original — should see the appended event
        let event_stream = store
            .read_stream(&sid, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_ref().unwrap().event_type, "OrderPlaced");
    }

    #[tokio::test]
    async fn test_read_unknown_stream_empty() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("nonexistent");

        let event_stream = store
            .read_stream(&sid, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn test_current_sequence_empty() {
        let store = InMemoryLogStore::new();
        let seq = store.current_sequence().await.unwrap();
        assert_eq!(seq, GlobalSequenceId::ZERO);
    }

    #[tokio::test]
    async fn test_current_sequence_after_append() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![test_event("E1"), test_event("E2"), test_event("E3")],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let seq = store.current_sequence().await.unwrap();
        assert_eq!(seq, GlobalSequenceId::new(3));
    }

    #[tokio::test]
    async fn test_stream_version_empty() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("nonexistent");

        let version = store.stream_version(&sid).await.unwrap();
        assert_eq!(version, StreamSequenceId::ZERO);
    }

    #[tokio::test]
    async fn test_stream_version_after_append() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![test_event("E1"), test_event("E2"), test_event("E3")],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let version = store.stream_version(&sid).await.unwrap();
        assert_eq!(version, StreamSequenceId::new(3));
    }

    #[tokio::test]
    async fn test_append_expected_version_success() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![test_event("E1"), test_event("E2")],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        // Append with ExpectedVersion(2) — should succeed (stream is at version 2)
        let result = store
            .append(
                &sid,
                vec![test_event("E3")],
                AppendCondition::ExpectedVersion(StreamSequenceId::new(2)),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_append_expected_version_conflict() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![test_event("E1"), test_event("E2")],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        // Try to append with ExpectedVersion(1) — stream is at version 2, should fail
        let result = store
            .append(
                &sid,
                vec![test_event("E3")],
                AppendCondition::ExpectedVersion(StreamSequenceId::new(1)),
            )
            .await;

        match result {
            Err(AppendError::ConcurrencyConflict { expected, actual, .. }) => {
                assert_eq!(expected, StreamSequenceId::new(1));
                assert_eq!(actual, StreamSequenceId::new(2));
            }
            other => panic!("expected ConcurrencyConflict, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_append_expected_version_zero_on_new_stream() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        // Append with ExpectedVersion(0) on a new (empty) stream — should succeed
        let result = store
            .append(
                &sid,
                vec![test_event("E1")],
                AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
            )
            .await;

        assert!(result.is_ok());
        let version = store.stream_version(&sid).await.unwrap();
        assert_eq!(version, StreamSequenceId::new(1));
    }

    #[tokio::test]
    async fn test_append_batch_atomicity() {
        let store = InMemoryLogStore::new();
        let sid = stream_id("orders");

        store
            .append(
                &sid,
                vec![test_event("E1"), test_event("E2"), test_event("E3")],
                AppendCondition::Any,
            )
            .await
            .unwrap();

        let event_stream = store
            .read_stream(&sid, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events: Vec<_> = event_stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        // Verify sequential stream sequences
        for (i, e) in events.iter().enumerate() {
            assert_eq!(
                e.as_ref().unwrap().stream_sequence,
                StreamSequenceId::new(i as u64 + 1)
            );
        }
        // Verify sequential global sequences
        let g1 = events[0].as_ref().unwrap().global_sequence;
        let g2 = events[1].as_ref().unwrap().global_sequence;
        let g3 = events[2].as_ref().unwrap().global_sequence;
        assert_eq!(g1.get() + 1, g2.get());
        assert_eq!(g2.get() + 1, g3.get());
    }

    #[tokio::test]
    async fn test_multiple_streams_independent_sequences() {
        let store = InMemoryLogStore::new();
        let sa = stream_id("a");
        let sb = stream_id("b");

        store
            .append(&sa, vec![test_event("A1"), test_event("A2")], AppendCondition::Any)
            .await
            .unwrap();
        store
            .append(&sb, vec![test_event("B1")], AppendCondition::Any)
            .await
            .unwrap();

        // Stream "a" should have stream sequences 1 and 2
        let stream_a = store
            .read_stream(&sa, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events_a: Vec<_> = stream_a.collect::<Vec<_>>().await;
        assert_eq!(events_a.len(), 2);
        assert_eq!(
            events_a[0].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(1)
        );
        assert_eq!(
            events_a[1].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(2)
        );

        // Stream "b" should have stream sequence 1
        let stream_b = store
            .read_stream(&sb, StreamSequenceId::new(1), None)
            .await
            .unwrap();
        let events_b: Vec<_> = stream_b.collect::<Vec<_>>().await;
        assert_eq!(events_b.len(), 1);
        assert_eq!(
            events_b[0].as_ref().unwrap().stream_sequence,
            StreamSequenceId::new(1)
        );
    }
}
