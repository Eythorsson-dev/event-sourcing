use event_sourcing::{
    AppendCondition, EventLog, EventLogError, EventStreamExt, EventType, GlobalSequenceId,
    NewEvent, Query, StreamError, Tag,
};
use event_sourcing_logstore_inmemory::InMemoryLogStore;
use std::collections::HashSet;

fn tag(s: &str) -> Tag {
    Tag::new(s).unwrap()
}

fn tags(ts: &[&str]) -> HashSet<Tag> {
    ts.iter().map(|s| tag(s)).collect()
}

fn new_event(event_type: &str, tag_strs: &[&str]) -> NewEvent {
    NewEvent {
        event_type: EventType::from(event_type),
        payload: serde_json::json!({}),
        tags: tags(tag_strs),
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
async fn append_none_condition_returns_global_sequence_1() {
    let log = make_log();
    let result = log
        .append(vec![new_event("OrderPlaced", &["order:o1"])], None)
        .await;
    assert_eq!(result.unwrap(), GlobalSequenceId::new(1));
}

#[tokio::test]
async fn append_returns_incrementing_global_sequence_ids() {
    let log = make_log();

    let seq1 = log.append(vec![new_event("E1", &[])], None).await.unwrap();
    let seq2 = log.append(vec![new_event("E2", &[])], None).await.unwrap();
    let seq3 = log.append(vec![new_event("E3", &[])], None).await.unwrap();

    assert_eq!(seq1, GlobalSequenceId::new(1));
    assert_eq!(seq2, GlobalSequenceId::new(2));
    assert_eq!(seq3, GlobalSequenceId::new(3));
}

#[tokio::test]
async fn query_all_returns_all_events_in_global_order() {
    let log = make_log();

    log.append(vec![new_event("E1", &["a"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E2", &["b"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E3", &["c"])], None)
        .await
        .unwrap();

    let stream = log
        .query(Query::all(), GlobalSequenceId::ZERO)
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].global_sequence, GlobalSequenceId::new(1));
    assert_eq!(events[1].global_sequence, GlobalSequenceId::new(2));
    assert_eq!(events[2].global_sequence, GlobalSequenceId::new(3));
}

#[tokio::test]
async fn query_by_tag_returns_only_matching_events() {
    let log = make_log();

    log.append(vec![new_event("E1", &["a"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E2", &["b"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E3", &["a", "b"])], None)
        .await
        .unwrap();

    let stream = log
        .query(Query::match_tags([tag("a")]), GlobalSequenceId::ZERO)
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.tags.contains(&tag("a"))));
}

#[tokio::test]
async fn query_by_event_type_returns_only_matching_events() {
    let log = make_log();

    log.append(vec![new_event("X", &[])], None).await.unwrap();
    log.append(vec![new_event("Y", &[])], None).await.unwrap();
    log.append(vec![new_event("X", &[])], None).await.unwrap();

    let stream = log
        .query(Query::match_event_types(["X"]), GlobalSequenceId::ZERO)
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.event_type.as_str() == "X"));
}

#[tokio::test]
async fn query_from_position_excludes_earlier_events() {
    let log = make_log();

    log.append(vec![new_event("E1", &[])], None).await.unwrap();
    log.append(vec![new_event("E2", &[])], None).await.unwrap();
    log.append(vec![new_event("E3", &[])], None).await.unwrap();

    let stream = log
        .query(Query::all(), GlobalSequenceId::new(2))
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    let events: Vec<_> = events.into_iter().map(|e| e.unwrap()).collect();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].global_sequence, GlobalSequenceId::new(2));
    assert_eq!(events[1].global_sequence, GlobalSequenceId::new(3));
}

#[tokio::test]
async fn query_no_match_returns_empty_stream() {
    let log = make_log();

    log.append(vec![new_event("E1", &["a"])], None)
        .await
        .unwrap();

    let stream = log
        .query(
            Query::match_tags([tag("nonexistent")]),
            GlobalSequenceId::ZERO,
        )
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    assert_eq!(events.len(), 0);
}

#[tokio::test]
async fn append_condition_conflict() {
    let log = make_log();

    // Append an event with tag "order:o1"
    log.append(vec![new_event("OrderPlaced", &["order:o1"])], None)
        .await
        .unwrap();

    // Condition: fail if any event matching tag "order:o1" appeared after ZERO
    let condition = AppendCondition {
        query: Query::match_tags([tag("order:o1")]),
        after: GlobalSequenceId::ZERO,
    };

    let result = log
        .append(
            vec![new_event("OrderUpdated", &["order:o1"])],
            Some(condition),
        )
        .await;

    assert!(
        matches!(result, Err(EventLogError::ConcurrencyConflict { .. })),
        "expected ConcurrencyConflict, got {:?}",
        result
    );
}

#[tokio::test]
async fn append_condition_no_conflict() {
    let log = make_log();

    // Append an event
    log.append(vec![new_event("OrderPlaced", &["order:o1"])], None)
        .await
        .unwrap();

    // Get current position — no events after this position
    let current_seq = log.current_sequence().await.unwrap();

    // Condition after current_seq: no events can have appeared after current position yet
    let condition = AppendCondition {
        query: Query::match_tags([tag("order:o1")]),
        after: current_seq,
    };

    let result = log
        .append(
            vec![new_event("OrderShipped", &["order:o1"])],
            Some(condition),
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got {:?}", result);
}

#[tokio::test]
async fn concurrent_condition_conflict() {
    use std::sync::Arc;

    let store = InMemoryLogStore::new();
    let log = Arc::new(EventLog::new(store));

    let log1 = Arc::clone(&log);
    let log2 = Arc::clone(&log);

    let make_condition = || AppendCondition {
        query: Query::match_tags([tag("order:o1")]),
        after: GlobalSequenceId::ZERO,
    };

    let (result1, result2): (
        Result<GlobalSequenceId, EventLogError>,
        Result<GlobalSequenceId, EventLogError>,
    ) = tokio::join!(
        async move {
            log1.append(
                vec![new_event("OrderPlaced", &["order:o1"])],
                Some(make_condition()),
            )
            .await
        },
        async move {
            log2.append(
                vec![new_event("OrderCancelled", &["order:o1"])],
                Some(make_condition()),
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
async fn event_log_is_clone_and_shares_state() {
    let log = make_log();
    let log2 = log.clone();

    log.append(vec![new_event("E1", &["shared"])], None)
        .await
        .unwrap();

    let stream = log2
        .query(Query::match_tags([tag("shared")]), GlobalSequenceId::ZERO)
        .await
        .unwrap();
    let events: Vec<_> = {
        use futures::StreamExt;
        stream.collect().await
    };
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn single_returns_exactly_one_event() {
    let log = make_log();

    log.append(vec![new_event("OrderPlaced", &["order:o1"])], None)
        .await
        .unwrap();

    let stream = log
        .query(Query::match_tags([tag("order:o1")]), GlobalSequenceId::ZERO)
        .await
        .unwrap();

    let result = stream.single().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().event_type.as_str(), "OrderPlaced");
}

#[tokio::test]
async fn single_errors_on_empty() {
    let log = make_log();

    let stream = log
        .query(
            Query::match_tags([tag("nonexistent")]),
            GlobalSequenceId::ZERO,
        )
        .await
        .unwrap();

    let result = stream.single().await;
    assert!(matches!(result, Err(StreamError::Empty)));
}

#[tokio::test]
async fn single_errors_on_multiple() {
    let log = make_log();

    log.append(vec![new_event("E1", &["shared"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E2", &["shared"])], None)
        .await
        .unwrap();

    let stream = log
        .query(Query::match_tags([tag("shared")]), GlobalSequenceId::ZERO)
        .await
        .unwrap();

    let result = stream.single().await;
    assert!(matches!(result, Err(StreamError::Multiple)));
}

#[tokio::test]
async fn first_returns_first_event() {
    let log = make_log();

    log.append(vec![new_event("E1", &["shared"])], None)
        .await
        .unwrap();
    log.append(vec![new_event("E2", &["shared"])], None)
        .await
        .unwrap();

    let stream = log
        .query(Query::match_tags([tag("shared")]), GlobalSequenceId::ZERO)
        .await
        .unwrap();

    let result = stream.first().await;
    assert!(result.is_ok());
    let opt = result.unwrap();
    assert!(opt.is_some());
    assert_eq!(opt.unwrap().event_type.as_str(), "E1");
}

#[tokio::test]
async fn first_returns_none_on_empty() {
    let log = make_log();

    let stream = log
        .query(
            Query::match_tags([tag("nonexistent")]),
            GlobalSequenceId::ZERO,
        )
        .await
        .unwrap();

    let result = stream.first().await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}
