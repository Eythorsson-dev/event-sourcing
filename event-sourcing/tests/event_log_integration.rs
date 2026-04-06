use event_sourcing::{
    AppendCondition, EventLog, EventLogError, GlobalSequenceId, NewEvent, StreamId,
    StreamSequenceId,
};
use event_sourcing_logstore_inmemory::InMemoryLogStore;
use futures::StreamExt;

fn new_event(event_type: &str) -> NewEvent {
    NewEvent {
        event_type: event_type.to_string(),
        payload: serde_json::json!({}),
    }
}

fn make_log() -> EventLog<InMemoryLogStore> {
    EventLog::new(InMemoryLogStore::new())
}

#[tokio::test]
async fn new_constructs_without_panic() {
    let _log = make_log();
}

#[tokio::test]
async fn append_any_returns_global_sequence_id_1_for_first_event() {
    let log = make_log();
    let stream_id = StreamId::new("orders").unwrap();
    let result = log
        .append(&stream_id, vec![new_event("OrderPlaced")], AppendCondition::Any)
        .await;
    assert_eq!(result.unwrap(), GlobalSequenceId::new(1));
}

#[tokio::test]
async fn append_expected_version_conflict_returns_error() {
    let log = make_log();
    let stream_id = StreamId::new("orders").unwrap();

    // First append succeeds — stream is now at version 1
    log.append(&stream_id, vec![new_event("OrderPlaced")], AppendCondition::Any)
        .await
        .unwrap();

    // Second append with ExpectedVersion(ZERO) should conflict
    let result = log
        .append(
            &stream_id,
            vec![new_event("OrderShipped")],
            AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
        )
        .await;

    match result {
        Err(EventLogError::ConcurrencyConflict {
            expected, actual, ..
        }) => {
            assert_eq!(expected, StreamSequenceId::ZERO);
            assert_eq!(actual, StreamSequenceId::new(1));
        }
        other => panic!("expected ConcurrencyConflict, got {:?}", other),
    }
}

#[tokio::test]
async fn read_stream_returns_events_in_stream_sequence_order() {
    let log = make_log();
    let stream_id = StreamId::new("orders").unwrap();

    log.append(&stream_id, vec![new_event("OrderPlaced")], AppendCondition::Any)
        .await
        .unwrap();
    log.append(&stream_id, vec![new_event("OrderShipped")], AppendCondition::Any)
        .await
        .unwrap();
    log.append(&stream_id, vec![new_event("OrderDelivered")], AppendCondition::Any)
        .await
        .unwrap();

    let stream = log
        .read_stream(&stream_id, StreamSequenceId::new(1), None)
        .await
        .unwrap();
    let events: Vec<_> = stream.collect().await;
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].stream_sequence, StreamSequenceId::new(1));
    assert_eq!(events[1].stream_sequence, StreamSequenceId::new(2));
    assert_eq!(events[2].stream_sequence, StreamSequenceId::new(3));
    assert_eq!(events[0].event_type, "OrderPlaced");
    assert_eq!(events[1].event_type, "OrderShipped");
    assert_eq!(events[2].event_type, "OrderDelivered");
}

#[tokio::test]
async fn read_stream_with_range_returns_bounded_events() {
    let log = make_log();
    let stream_id = StreamId::new("orders").unwrap();

    for event_type in &["E1", "E2", "E3", "E4", "E5"] {
        log.append(&stream_id, vec![new_event(event_type)], AppendCondition::Any)
            .await
            .unwrap();
    }

    let stream = log
        .read_stream(
            &stream_id,
            StreamSequenceId::new(2),
            Some(StreamSequenceId::new(3)),
        )
        .await
        .unwrap();
    let events: Vec<_> = stream.collect().await;
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].stream_sequence, StreamSequenceId::new(2));
    assert_eq!(events[1].stream_sequence, StreamSequenceId::new(3));
}

#[tokio::test]
async fn read_all_returns_events_across_streams_sorted_by_global_sequence() {
    let log = make_log();
    let stream_a = StreamId::new("stream-a").unwrap();
    let stream_b = StreamId::new("stream-b").unwrap();

    log.append(&stream_a, vec![new_event("EventA1")], AppendCondition::Any)
        .await
        .unwrap();
    log.append(&stream_b, vec![new_event("EventB1")], AppendCondition::Any)
        .await
        .unwrap();
    log.append(&stream_a, vec![new_event("EventA2")], AppendCondition::Any)
        .await
        .unwrap();

    let stream = log.read_all(GlobalSequenceId::new(1)).await.unwrap();
    let events: Vec<_> = stream.collect().await;
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].global_sequence, GlobalSequenceId::new(1));
    assert_eq!(events[1].global_sequence, GlobalSequenceId::new(2));
    assert_eq!(events[2].global_sequence, GlobalSequenceId::new(3));
    assert_eq!(events[0].event_type, "EventA1");
    assert_eq!(events[1].event_type, "EventB1");
    assert_eq!(events[2].event_type, "EventA2");
}

#[tokio::test]
async fn concurrent_appends_with_same_expected_version_yield_one_success_one_conflict() {
    use std::sync::Arc;

    let store = InMemoryLogStore::new();
    let log = Arc::new(EventLog::new(store));
    let stream_id = StreamId::new("orders").unwrap();
    let stream_id2 = stream_id.clone();

    let log1 = Arc::clone(&log);
    let log2 = Arc::clone(&log);

    let (result1, result2): (Result<GlobalSequenceId, EventLogError>, Result<GlobalSequenceId, EventLogError>) = tokio::join!(
        async move {
            log1.append(
                &stream_id,
                vec![new_event("OrderPlaced")],
                AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
            )
            .await
        },
        async move {
            log2.append(
                &stream_id2,
                vec![new_event("OrderCancelled")],
                AppendCondition::ExpectedVersion(StreamSequenceId::ZERO),
            )
            .await
        }
    );

    let successes = [&result1, &result2].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&result1, &result2]
        .iter()
        .filter(|r| matches!(r, Err(EventLogError::ConcurrencyConflict { .. })))
        .count();

    assert_eq!(successes, 1, "exactly one append should succeed");
    assert_eq!(conflicts, 1, "exactly one append should conflict");
}

#[tokio::test]
async fn append_returns_incrementing_global_sequence_ids() {
    let log = make_log();
    let stream_id = StreamId::new("orders").unwrap();

    let seq1 = log
        .append(&stream_id, vec![new_event("E1")], AppendCondition::Any)
        .await
        .unwrap();
    let seq2 = log
        .append(&stream_id, vec![new_event("E2")], AppendCondition::Any)
        .await
        .unwrap();
    let seq3 = log
        .append(&stream_id, vec![new_event("E3")], AppendCondition::Any)
        .await
        .unwrap();

    assert_eq!(seq1, GlobalSequenceId::new(1));
    assert_eq!(seq2, GlobalSequenceId::new(2));
    assert_eq!(seq3, GlobalSequenceId::new(3));
}

#[tokio::test]
async fn event_log_is_clone_and_shares_state() {
    let log = make_log();
    let log2 = log.clone();
    let stream_id = StreamId::new("orders").unwrap();

    log.append(&stream_id, vec![new_event("E1")], AppendCondition::Any)
        .await
        .unwrap();

    // Clone shares backing state — should see the event appended by original
    let stream = log2
        .read_stream(&stream_id, StreamSequenceId::new(1), None)
        .await
        .unwrap();
    let events: Vec<_> = stream.collect().await;
    assert_eq!(events.len(), 1);
}
