use crate::event::StoredEvent;
use crate::types::Tag;
use std::collections::HashSet;

/// A single filter criterion within a query.
/// An event matches a criterion if ALL of the following hold:
/// - event_types is empty OR event.event_type is in event_types
/// - tags is empty OR event.tags is a superset of tags (all required tags present)
#[derive(Debug, Clone)]
pub struct Criterion {
    pub event_types: HashSet<String>,
    pub tags: HashSet<Tag>,
}

/// A query is a set of criteria. An event matches the query if it matches ANY criterion (OR semantics).
/// An empty criteria vec means "match nothing" — use Query::all() for match-all.
#[derive(Debug, Clone)]
pub struct Query {
    pub criteria: Vec<Criterion>,
}

impl Query {
    /// Match all events.
    pub fn all() -> Self {
        Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tags: HashSet::new(),
            }],
        }
    }

    /// Match events carrying ALL of the specified tags.
    pub fn match_tags(tags: impl IntoIterator<Item = Tag>) -> Self {
        Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tags: tags.into_iter().collect(),
            }],
        }
    }

    /// Match events of any of the specified event types.
    pub fn match_event_types(event_types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Query {
            criteria: vec![Criterion {
                event_types: event_types.into_iter().map(Into::into).collect(),
                tags: HashSet::new(),
            }],
        }
    }

    /// Add an OR criterion to the query, returning the modified query.
    pub fn or(mut self, criterion: Criterion) -> Self {
        self.criteria.push(criterion);
        self
    }
}

/// Check if a stored event matches a single criterion.
pub fn event_matches_criterion(event: &StoredEvent, criterion: &Criterion) -> bool {
    let type_match =
        criterion.event_types.is_empty() || criterion.event_types.contains(&event.event_type);
    let tag_match =
        criterion.tags.is_empty() || criterion.tags.iter().all(|t| event.tags.contains(t));
    type_match && tag_match
}

/// Check if a stored event matches a query (any criterion matches).
pub fn event_matches_query(event: &StoredEvent, query: &Query) -> bool {
    if query.criteria.is_empty() {
        return false;
    }
    query
        .criteria
        .iter()
        .any(|c| event_matches_criterion(event, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GlobalSequenceId;
    use std::time::SystemTime;

    fn make_event(event_type: &str, tag_strs: &[&str]) -> StoredEvent {
        StoredEvent {
            global_sequence: GlobalSequenceId::new(1),
            event_type: event_type.to_string(),
            payload: serde_json::json!({}),
            tags: tag_strs.iter().map(|s| Tag::new(*s).unwrap()).collect(),
            timestamp: SystemTime::now(),
        }
    }

    #[test]
    fn query_all_matches_any_event() {
        let event = make_event("OrderPlaced", &["order:o1"]);
        assert!(event_matches_query(&event, &Query::all()));
    }

    #[test]
    fn query_empty_criteria_matches_nothing() {
        let event = make_event("OrderPlaced", &["order:o1"]);
        let q = Query { criteria: vec![] };
        assert!(!event_matches_query(&event, &q));
    }

    #[test]
    fn query_match_tags_filters_correctly() {
        let event_ab = make_event("E", &["a", "b"]);
        let event_b = make_event("E", &["b"]);

        let q_a = Query::match_tags([Tag::new("a").unwrap()]);
        assert!(event_matches_query(&event_ab, &q_a));
        assert!(!event_matches_query(&event_b, &q_a));

        let q_c = Query::match_tags([Tag::new("c").unwrap()]);
        assert!(!event_matches_query(&event_ab, &q_c));
    }

    #[test]
    fn query_match_event_types_filters_correctly() {
        let event_x = make_event("X", &[]);
        let event_y = make_event("Y", &[]);

        let q = Query::match_event_types(["X"]);
        assert!(event_matches_query(&event_x, &q));
        assert!(!event_matches_query(&event_y, &q));
    }

    #[test]
    fn query_criterion_and_semantics() {
        // Criterion requires BOTH tag "a" AND event type "X"
        let criterion = Criterion {
            event_types: ["X".to_string()].into_iter().collect(),
            tags: [Tag::new("a").unwrap()].into_iter().collect(),
        };
        let q = Query {
            criteria: vec![criterion],
        };

        let event_match = make_event("X", &["a"]);
        let event_wrong_type = make_event("Y", &["a"]);
        let event_wrong_tag = make_event("X", &["b"]);

        assert!(event_matches_query(&event_match, &q));
        assert!(!event_matches_query(&event_wrong_type, &q));
        assert!(!event_matches_query(&event_wrong_tag, &q));
    }

    #[test]
    fn query_or_semantics() {
        let q = Query::match_tags([Tag::new("a").unwrap()]).or(Criterion {
            event_types: ["X".to_string()].into_iter().collect(),
            tags: HashSet::new(),
        });

        let event_tag_a = make_event("Y", &["a"]);
        let event_type_x = make_event("X", &["b"]);
        let event_neither = make_event("Z", &["c"]);

        assert!(event_matches_query(&event_tag_a, &q));
        assert!(event_matches_query(&event_type_x, &q));
        assert!(!event_matches_query(&event_neither, &q));
    }
}
