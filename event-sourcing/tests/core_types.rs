use std::collections::HashSet;
use std::time::SystemTime;

use event_sourcing::{
    AppendCondition, AppendError, EmptyStreamIdError, GlobalSequenceId, LogStore, NewEvent,
    StoreError, StoredEvent, StreamId, StreamSequenceId,
};

// ── Test 1: StreamId construction ───────────────────────────────────────────

#[test]
fn stream_id_valid_construction() {
    let id = StreamId::new("account-123").unwrap();
    assert_eq!(id.as_str(), "account-123");
}

#[test]
fn stream_id_empty_string_returns_error() {
    let result = StreamId::new("");
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Confirm it is EmptyStreamIdError via Display
    assert!(err.to_string().contains("empty"));
}

#[test]
fn stream_id_empty_error_type() {
    let result: Result<StreamId, EmptyStreamIdError> = StreamId::new("");
    assert!(matches!(result, Err(EmptyStreamIdError)));
}

// ── Test 2: StreamId equality / hash ────────────────────────────────────────

#[test]
fn stream_id_equality() {
    let a = StreamId::new("stream-1").unwrap();
    let b = StreamId::new("stream-1").unwrap();
    let c = StreamId::new("stream-2").unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn stream_id_hash_consistency() {
    let a = StreamId::new("stream-x").unwrap();
    let b = StreamId::new("stream-x").unwrap();
    let mut set = HashSet::new();
    set.insert(a);
    // Inserting the same value again should not grow the set
    assert!(!set.insert(b));
    assert_eq!(set.len(), 1);
}

// ── Test 3: Sequence ID ordering ────────────────────────────────────────────

#[test]
fn global_sequence_id_ordering() {
    let low = GlobalSequenceId::new(1);
    let high = GlobalSequenceId::new(2);
    assert!(low < high);
    assert!(high > low);
    assert_eq!(low, GlobalSequenceId::new(1));
}

#[test]
fn stream_sequence_id_ordering() {
    let low = StreamSequenceId::new(1);
    let high = StreamSequenceId::new(5);
    assert!(low < high);
    assert_eq!(low, StreamSequenceId::new(1));
}

#[test]
fn sequence_id_zero_semantics() {
    // Zero means "no events seen" — must be constructable and less than 1
    let zero = GlobalSequenceId::new(0);
    let one = GlobalSequenceId::new(1);
    assert!(zero < one);
    assert_eq!(zero.as_u64(), 0);
}

// ── Test 4: Sequence ID type safety (compile-time) ──────────────────────────
// GlobalSequenceId and StreamSequenceId are distinct types.
// The following would NOT compile (type mismatch):
//   let g: GlobalSequenceId = StreamSequenceId::new(1);  // ← compile error
// This is enforced at the type level — no runtime assertion needed.

#[test]
fn sequence_id_types_are_distinct() {
    let global = GlobalSequenceId::new(42);
    let stream = StreamSequenceId::new(42);
    // Same inner value, different types — cannot be compared directly
    assert_eq!(global.as_u64(), stream.as_u64()); // values match
    // But they are different types — assigning one to the other would not compile
}

// ── Test 5: StoredEvent construction ────────────────────────────────────────

#[test]
fn stored_event_construction_and_field_access() {
    let stream_id = StreamId::new("order-456").unwrap();
    let payload = serde_json::json!({ "amount": 100, "currency": "USD" });

    let event = StoredEvent {
        global_sequence_id: GlobalSequenceId::new(7),
        stream_sequence_id: StreamSequenceId::new(3),
        stream_id: stream_id.clone(),
        event_type: "OrderPlaced".to_string(),
        payload: payload.clone(),
        timestamp: SystemTime::now(),
    };

    assert_eq!(event.global_sequence_id, GlobalSequenceId::new(7));
    assert_eq!(event.stream_sequence_id, StreamSequenceId::new(3));
    assert_eq!(event.stream_id, stream_id);
    assert_eq!(event.event_type, "OrderPlaced");
    assert_eq!(event.payload["amount"], 100);

    // Clone works
    let cloned = event.clone();
    assert_eq!(cloned.event_type, "OrderPlaced");
}

// ── Test 6: AppendError matching ─────────────────────────────────────────────

#[test]
fn append_error_concurrency_conflict_matching() {
    let err = AppendError::ConcurrencyConflict;
    let matched = match err {
        AppendError::ConcurrencyConflict => "conflict",
        AppendError::StorageFailure(_) => "storage",
    };
    assert_eq!(matched, "conflict");
}

#[test]
fn append_error_storage_failure_matching() {
    let err = AppendError::StorageFailure(Box::new(std::io::Error::other("disk full")));
    let matched = match err {
        AppendError::ConcurrencyConflict => "conflict",
        AppendError::StorageFailure(_) => "storage",
    };
    assert_eq!(matched, "storage");
}

#[test]
fn append_error_display() {
    let err = AppendError::ConcurrencyConflict;
    assert!(err.to_string().contains("concurrency conflict"));
}

// ── Test 7: LogStore trait implementability ──────────────────────────────────

use futures_core::Stream;
use std::pin::Pin;

struct StubStore;

// A concrete stream type for the stub
type BoxedEventStream =
    Pin<Box<dyn Stream<Item = Result<StoredEvent, StoreError>> + Send>>;

impl LogStore for StubStore {
    type EventStream = BoxedEventStream;

    async fn append(
        &self,
        _stream_id: &StreamId,
        _events: Vec<NewEvent>,
        _condition: AppendCondition,
    ) -> Result<GlobalSequenceId, AppendError> {
        Ok(GlobalSequenceId::new(1))
    }

    async fn read_stream(
        &self,
        _stream_id: &StreamId,
        _from: StreamSequenceId,
    ) -> Result<Self::EventStream, StoreError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn read_all(
        &self,
        _from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError> {
        Ok(GlobalSequenceId::new(0))
    }

    async fn stream_version(
        &self,
        _stream_id: &StreamId,
    ) -> Result<StreamSequenceId, StoreError> {
        Ok(StreamSequenceId::new(0))
    }
}

#[tokio::test]
async fn log_store_trait_is_implementable() {
    let store = StubStore;
    let seq = store.current_sequence().await.unwrap();
    assert_eq!(seq.as_u64(), 0);
}

#[tokio::test]
async fn log_store_append_returns_sequence() {
    let store = StubStore;
    let stream_id = StreamId::new("test-stream").unwrap();
    let event = NewEvent {
        event_type: "TestEvent".to_string(),
        payload: serde_json::json!({}),
    };
    let result = store
        .append(&stream_id, vec![event], AppendCondition::Any)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_u64(), 1);
}
