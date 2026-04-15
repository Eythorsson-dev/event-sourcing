use std::sync::Arc;
use std::time::SystemTime;

use event_sourcing::query::event_matches_query;
use event_sourcing::{
    AppendCondition, AppendError, GlobalSequenceId, LogStore, NewEvent, Query, StoreError,
    StoredEvent,
};
use futures::stream::{self, Iter};
use tokio::sync::RwLock;

/// The concrete event stream type returned by InMemoryLogStore query operations.
pub type InMemoryEventStream = Iter<std::vec::IntoIter<Result<StoredEvent, StoreError>>>;

struct InnerState {
    events: Vec<StoredEvent>,
    /// Next global sequence to assign (1-based; starts at 1). Incremented under write lock.
    next_global_seq: u64,
}

impl InnerState {
    fn new() -> Self {
        Self {
            events: Vec::new(),
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
/// allocated under the write lock to prevent gaps and TOCTOU races on condition checks.
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
        events: Vec<NewEvent>,
        condition: Option<AppendCondition>,
    ) -> Result<GlobalSequenceId, AppendError> {
        if events.is_empty() {
            // Nothing to append — return current sequence (no change).
            let state = self.inner.read().await;
            return Ok(state.current_sequence());
        }

        let mut state = self.inner.write().await;

        // Check optimistic concurrency condition BEFORE allocating any sequence IDs.
        // Runs under write lock — no TOCTOU gap.
        if let Some(AppendCondition { query, after }) = condition {
            let conflict = state
                .events
                .iter()
                .find(|e| e.global_sequence > after && event_matches_query(e, &query));

            if let Some(conflicting) = conflict {
                return Err(AppendError::ConcurrencyConflict {
                    conflicting_position: conflicting.global_sequence,
                    checked_after: after,
                });
            }
        }

        // All checks passed — build and store events under the write lock.
        let mut last_global_seq = GlobalSequenceId::ZERO;

        for new_event in events {
            let global_seq = GlobalSequenceId::new(state.next_global_seq);
            state.next_global_seq += 1;

            let stored = StoredEvent {
                global_sequence: global_seq,
                event_type: new_event.event_type,
                payload: new_event.payload,
                tags: new_event.tags,
                timestamp: SystemTime::now(),
            };

            state.events.push(stored);
            last_global_seq = global_seq;
        }

        Ok(last_global_seq)
    }

    async fn query(
        &self,
        query: Query,
        from: GlobalSequenceId,
    ) -> Result<Self::EventStream, StoreError> {
        let state = self.inner.read().await;

        let results: Vec<Result<StoredEvent, StoreError>> = state
            .events
            .iter()
            .filter(|e| e.global_sequence >= from && event_matches_query(e, &query))
            .cloned()
            .map(Ok)
            .collect();

        // Drop the lock before returning — do not hold across await points.
        drop(state);

        Ok(stream::iter(results))
    }

    async fn current_sequence(&self) -> Result<GlobalSequenceId, StoreError> {
        let state = self.inner.read().await;
        Ok(state.current_sequence())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use event_sourcing::{EventType, Tag};
    use futures::StreamExt;

    fn tag(s: &str) -> Tag {
        Tag::new(s).unwrap()
    }

    fn tags(ts: &[&str]) -> HashSet<Tag> {
        ts.iter().map(|s| tag(s)).collect()
    }

    fn test_event(event_type: &str, tag_strs: &[&str]) -> NewEvent {
        NewEvent {
            event_type: EventType::from(event_type),
            payload: serde_json::json!({"test": true}),
            tags: tags(tag_strs),
        }
    }

    #[tokio::test]
    async fn append_and_query_all() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("OrderPlaced", &["order:o1"]),
                    test_event("OrderShipped", &["order:o2"]),
                    test_event("OrderConfirmed", &["order:o1", "premium"]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(1)
        );
        assert_eq!(
            events[1].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(2)
        );
        assert_eq!(
            events[2].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(3)
        );
    }

    #[tokio::test]
    async fn query_filters_by_tag() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("E1", &["order:o1"]),
                    test_event("E2", &["order:o2"]),
                    test_event("E3", &["order:o1", "premium"]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(Query::match_tags([tag("order:o1")]), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].as_ref().unwrap().event_type.as_str(), "E1");
        assert_eq!(events[1].as_ref().unwrap().event_type.as_str(), "E3");
    }

    #[tokio::test]
    async fn query_filters_by_event_type() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("OrderPlaced", &[]),
                    test_event("OrderShipped", &[]),
                    test_event("OrderPlaced", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(
                Query::match_event_types(["OrderPlaced"]),
                GlobalSequenceId::ZERO,
            )
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|e| e.as_ref().unwrap().event_type.as_str() == "OrderPlaced"));
    }

    #[tokio::test]
    async fn query_all_returns_all_events() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("A", &[]),
                    test_event("B", &[]),
                    test_event("C", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn query_no_match_returns_empty() {
        let store = InMemoryLogStore::new();

        store
            .append(vec![test_event("E1", &["a"])], None)
            .await
            .unwrap();

        let stream = store
            .query(
                Query::match_tags([tag("nonexistent")]),
                GlobalSequenceId::ZERO,
            )
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn query_from_position() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("E1", &[]),
                    test_event("E2", &[]),
                    test_event("E3", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        // from=2 means: include events with global_sequence >= 2
        let stream = store
            .query(Query::all(), GlobalSequenceId::new(2))
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(2)
        );
        assert_eq!(
            events[1].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(3)
        );
    }

    #[tokio::test]
    async fn append_none_condition() {
        let store = InMemoryLogStore::new();

        let result = store.append(vec![test_event("E1", &[])], None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), GlobalSequenceId::new(1));
    }

    #[tokio::test]
    async fn append_condition_no_conflict() {
        let store = InMemoryLogStore::new();

        // Append one event tagged "order:o1"
        store
            .append(vec![test_event("E1", &["order:o1"])], None)
            .await
            .unwrap();

        let current_seq = store.current_sequence().await.unwrap();

        // Append with condition: no "order:o1" events after current_seq.
        // No new events have been appended, so this should succeed.
        let result = store
            .append(
                vec![test_event("E2", &["order:o1"])],
                Some(AppendCondition {
                    query: Query::match_tags([tag("order:o1")]),
                    after: current_seq,
                }),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn append_condition_conflict() {
        let store = InMemoryLogStore::new();

        // Append event tagged "order:o1"
        store
            .append(vec![test_event("E1", &["order:o1"])], None)
            .await
            .unwrap();

        // Attempt second append with condition checking after ZERO — the first event matches.
        let result = store
            .append(
                vec![test_event("E2", &[])],
                Some(AppendCondition {
                    query: Query::match_tags([tag("order:o1")]),
                    after: GlobalSequenceId::ZERO,
                }),
            )
            .await;

        match result {
            Err(AppendError::ConcurrencyConflict {
                conflicting_position,
                checked_after,
            }) => {
                assert_eq!(conflicting_position, GlobalSequenceId::new(1));
                assert_eq!(checked_after, GlobalSequenceId::ZERO);
            }
            other => panic!("expected ConcurrencyConflict, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn append_condition_different_tags_no_conflict() {
        let store = InMemoryLogStore::new();

        // Append event tagged "order:o1"
        store
            .append(vec![test_event("E1", &["order:o1"])], None)
            .await
            .unwrap();

        // Condition checks for "order:o2" — existing event doesn't match, no conflict.
        let result = store
            .append(
                vec![test_event("E2", &[])],
                Some(AppendCondition {
                    query: Query::match_tags([tag("order:o2")]),
                    after: GlobalSequenceId::ZERO,
                }),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn global_sequence_monotonic() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("E1", &[]),
                    test_event("E2", &[]),
                    test_event("E3", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        let seqs: Vec<GlobalSequenceId> = events
            .iter()
            .map(|e| e.as_ref().unwrap().global_sequence)
            .collect();
        for i in 0..seqs.len() - 1 {
            assert!(
                seqs[i] < seqs[i + 1],
                "sequences not strictly increasing at {}",
                i
            );
        }
    }

    #[tokio::test]
    async fn current_sequence_empty() {
        let store = InMemoryLogStore::new();
        let seq = store.current_sequence().await.unwrap();
        assert_eq!(seq, GlobalSequenceId::ZERO);
    }

    #[tokio::test]
    async fn current_sequence_after_append() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("E1", &[]),
                    test_event("E2", &[]),
                    test_event("E3", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        let seq = store.current_sequence().await.unwrap();
        assert_eq!(seq, GlobalSequenceId::new(3));
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let store = InMemoryLogStore::new();
        let clone = store.clone();

        // Append via clone
        clone
            .append(vec![test_event("OrderPlaced", &["order:o1"])], None)
            .await
            .unwrap();

        // Query via original — should see the appended event
        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].as_ref().unwrap().event_type.as_str(),
            "OrderPlaced"
        );
    }

    #[tokio::test]
    async fn append_batch_atomicity() {
        let store = InMemoryLogStore::new();

        store
            .append(
                vec![
                    test_event("E1", &[]),
                    test_event("E2", &[]),
                    test_event("E3", &[]),
                ],
                None,
            )
            .await
            .unwrap();

        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(1)
        );
        assert_eq!(
            events[1].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(2)
        );
        assert_eq!(
            events[2].as_ref().unwrap().global_sequence,
            GlobalSequenceId::new(3)
        );
    }

    #[tokio::test]
    async fn concurrent_condition_conflict() {
        let store = Arc::new(InMemoryLogStore::new());

        // Append seed event tagged "order:o1"
        store
            .append(vec![test_event("Seed", &[])], None)
            .await
            .unwrap();

        let store1 = Arc::clone(&store);
        let store2 = Arc::clone(&store);

        // Two concurrent appends with the same condition checking after ZERO
        let condition1 = AppendCondition {
            query: Query::match_tags([tag("order:o1")]),
            after: GlobalSequenceId::ZERO,
        };
        let condition2 = AppendCondition {
            query: Query::match_tags([tag("order:o1")]),
            after: GlobalSequenceId::ZERO,
        };

        let (r1, r2) = tokio::join!(
            store1.append(vec![test_event("A", &["order:o1"])], Some(condition1)),
            store2.append(vec![test_event("B", &["order:o1"])], Some(condition2)),
        );

        // Exactly one should succeed and one should conflict.
        let successes = [r1.is_ok(), r2.is_ok()].iter().filter(|&&ok| ok).count();
        let conflicts = [r1.is_err(), r2.is_err()]
            .iter()
            .filter(|&&err| err)
            .count();

        assert_eq!(successes, 1, "exactly one append should succeed");
        assert_eq!(conflicts, 1, "exactly one append should conflict");
    }

    #[tokio::test]
    async fn append_empty_events_is_noop() {
        let store = InMemoryLogStore::new();

        let result = store.append(vec![], None).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), GlobalSequenceId::ZERO);

        // No events should be stored
        let stream = store
            .query(Query::all(), GlobalSequenceId::ZERO)
            .await
            .unwrap();
        let events: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn inmemory_evaluates_tag_filter_variants() {
        use event_sourcing::{Criterion, TagFilter};

        let store = InMemoryLogStore::new();

        for e in [
            test_event("E1", &["order:o1"]),
            test_event("E1", &["order:o2"]),
            test_event("E1", &["premium"]),
            test_event("E1", &["shipping:ship1"]),
        ] {
            store.append(vec![e], None).await.unwrap();
        }

        // StartsWith("order:") → 2 events
        let q = Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: Some(TagFilter::StartsWith("order:".into())),
            }],
        };
        let events: Vec<_> = store
            .query(q, GlobalSequenceId::ZERO)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events.len(), 2, "StartsWith(order:) should match 2 events");

        // EndsWith(":o1") → 1 event
        let q = Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: Some(TagFilter::EndsWith(":o1".into())),
            }],
        };
        let events: Vec<_> = store
            .query(q, GlobalSequenceId::ZERO)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events.len(), 1, "EndsWith(:o1) should match 1 event");

        // Or(Equals("premium"), StartsWith("shipping:")) → 2 events
        let q = Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: Some(TagFilter::Or(vec![
                    TagFilter::Equals(tag("premium")),
                    TagFilter::StartsWith("shipping:".into()),
                ])),
            }],
        };
        let events: Vec<_> = store
            .query(q, GlobalSequenceId::ZERO)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(
            events.len(),
            2,
            "Or(premium, shipping:*) should match 2 events"
        );

        // And(StartsWith("order:"), Equals("order:o1")) → 1 event
        let q = Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: Some(TagFilter::And(vec![
                    TagFilter::StartsWith("order:".into()),
                    TagFilter::Equals(tag("order:o1")),
                ])),
            }],
        };
        let events: Vec<_> = store
            .query(q, GlobalSequenceId::ZERO)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(
            events.len(),
            1,
            "And(order:*, order:o1) should match 1 event"
        );

        // tag_filter: None + empty event_types → match all 4
        let q = Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: None,
            }],
        };
        let events: Vec<_> = store
            .query(q, GlobalSequenceId::ZERO)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events.len(), 4, "tag_filter=None should match all events");
    }

    #[tokio::test]
    async fn append_condition_with_starts_with_filter_detects_cross_instance_conflict() {
        use event_sourcing::{AppendError, Criterion, TagFilter};

        let store = InMemoryLogStore::new();

        store
            .append(vec![test_event("E", &["order:o1"])], None)
            .await
            .unwrap();

        let condition = AppendCondition {
            query: Query {
                criteria: vec![Criterion {
                    event_types: HashSet::new(),
                    tag_filter: Some(TagFilter::StartsWith("order:".into())),
                }],
            },
            after: GlobalSequenceId::ZERO,
        };
        let result = store
            .append(vec![test_event("E", &["order:o2"])], Some(condition))
            .await;

        match result {
            Err(AppendError::ConcurrencyConflict { .. }) => {}
            other => panic!("expected ConcurrencyConflict, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn append_condition_with_starts_with_filter_allows_when_no_match() {
        use event_sourcing::{Criterion, TagFilter};

        let store = InMemoryLogStore::new();

        store
            .append(vec![test_event("E", &["premium"])], None)
            .await
            .unwrap();

        let condition = AppendCondition {
            query: Query {
                criteria: vec![Criterion {
                    event_types: HashSet::new(),
                    tag_filter: Some(TagFilter::StartsWith("order:".into())),
                }],
            },
            after: GlobalSequenceId::ZERO,
        };
        let result = store
            .append(vec![test_event("E", &["order:o1"])], Some(condition))
            .await;

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }
}
