use crate::event::StoredEvent;
use crate::types::{EventType, Tag};
use std::collections::HashSet;

/// Expressive tag filter for event matching within a Criterion.
/// Semantics (per phase 03.2 D-06):
/// - Equals(tag): event must carry this exact tag
/// - StartsWith(prefix): event must carry at least one tag whose string starts with prefix (full-string match on tag.as_str())
/// - EndsWith(suffix): event must carry at least one tag whose string ends with suffix
/// - And(filters): event must satisfy ALL filters; And(vec![]) is vacuously true
/// - Or(filters): event must satisfy ANY filter; Or(vec![]) is vacuously false
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TagFilter {
    Equals(Tag),
    StartsWith(String),
    EndsWith(String),
    And(Vec<TagFilter>),
    Or(Vec<TagFilter>),
}

impl TagFilter {
    /// Evaluate this filter against the full tag set of an event.
    pub fn matches(&self, tags: &HashSet<Tag>) -> bool {
        match self {
            TagFilter::Equals(t) => tags.contains(t),
            TagFilter::StartsWith(prefix) => {
                tags.iter().any(|t| t.as_str().starts_with(prefix.as_str()))
            }
            TagFilter::EndsWith(suffix) => {
                tags.iter().any(|t| t.as_str().ends_with(suffix.as_str()))
            }
            TagFilter::And(filters) => filters.iter().all(|f| f.matches(tags)),
            TagFilter::Or(filters) => filters.iter().any(|f| f.matches(tags)),
        }
    }
}

/// A single filter criterion within a query.
/// An event matches a criterion if ALL of the following hold:
/// - event_types is empty OR event.event_type is in event_types
/// - tag_filter is None OR tag_filter.matches(event.tags)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Criterion {
    pub event_types: HashSet<EventType>,
    pub tag_filter: Option<TagFilter>,
}

/// A query is a set of criteria. An event matches the query if it matches ANY criterion (OR semantics).
/// An empty criteria vec means "match nothing" — use Query::all() for match-all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Query {
    pub criteria: Vec<Criterion>,
}

impl Query {
    /// Match all events.
    pub fn all() -> Self {
        Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: None,
            }],
        }
    }

    /// Match events carrying ALL of the specified tags.
    /// If `tags` is empty, returns `Query::all()` — vacuously true semantics from `And([])`.
    /// Pass at least one tag to create a selective filter.
    pub fn match_tags(tags: impl IntoIterator<Item = Tag>) -> Self {
        let filters: Vec<TagFilter> = tags.into_iter().map(TagFilter::Equals).collect();
        // And([]) is vacuously true — treat as match-all rather than silently surprising callers.
        if filters.is_empty() {
            return Query::all();
        }
        Query {
            criteria: vec![Criterion {
                event_types: HashSet::new(),
                tag_filter: Some(TagFilter::And(filters)),
            }],
        }
    }

    /// Match events of any of the specified event types.
    pub fn match_event_types(event_types: impl IntoIterator<Item = impl Into<EventType>>) -> Self {
        Query {
            criteria: vec![Criterion {
                event_types: event_types.into_iter().map(Into::into).collect(),
                tag_filter: None,
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
    let tag_match = match &criterion.tag_filter {
        None => true,
        Some(f) => f.matches(&event.tags),
    };
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
            event_type: EventType::from(event_type),
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
            event_types: [EventType::from("X")].into_iter().collect(),
            tag_filter: Some(TagFilter::Equals(Tag::new("a").unwrap())),
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
            event_types: [EventType::from("X")].into_iter().collect(),
            tag_filter: None,
        });

        let event_tag_a = make_event("Y", &["a"]);
        let event_type_x = make_event("X", &["b"]);
        let event_neither = make_event("Z", &["c"]);

        assert!(event_matches_query(&event_tag_a, &q));
        assert!(event_matches_query(&event_type_x, &q));
        assert!(!event_matches_query(&event_neither, &q));
    }

    #[test]
    fn tag_filter_equals_matches_and_misses() {
        let a = Tag::new("a").unwrap();
        let b = Tag::new("b").unwrap();
        let tags_a: HashSet<Tag> = [a.clone()].into_iter().collect();
        let tags_b: HashSet<Tag> = [b.clone()].into_iter().collect();
        assert!(TagFilter::Equals(a.clone()).matches(&tags_a));
        assert!(!TagFilter::Equals(a).matches(&tags_b));
        let empty: HashSet<Tag> = HashSet::new();
        assert!(!TagFilter::Equals(Tag::new("a").unwrap()).matches(&empty));
    }

    #[test]
    fn tag_filter_starts_with_matches_and_misses() {
        let tags: HashSet<Tag> = [Tag::new("order:o1").unwrap()].into_iter().collect();
        assert!(TagFilter::StartsWith("order:".into()).matches(&tags));
        // Separator rule (D-11): "ordering:" does NOT match "order:o1"
        let tags2: HashSet<Tag> = [Tag::new("ordering:foo").unwrap()].into_iter().collect();
        assert!(!TagFilter::StartsWith("order:".into()).matches(&tags2));
    }

    #[test]
    fn tag_filter_ends_with_matches_and_misses() {
        let tags: HashSet<Tag> = [Tag::new("order:o1").unwrap()].into_iter().collect();
        assert!(TagFilter::EndsWith(":o1".into()).matches(&tags));
        assert!(!TagFilter::EndsWith(":o2".into()).matches(&tags));
    }

    #[test]
    fn tag_filter_and_requires_all_including_empty_vacuous_true() {
        let a = Tag::new("a").unwrap();
        let b = Tag::new("b").unwrap();
        let tags_ab: HashSet<Tag> = [a.clone(), b.clone()].into_iter().collect();
        let tags_a: HashSet<Tag> = [a.clone()].into_iter().collect();
        let f = TagFilter::And(vec![
            TagFilter::Equals(a.clone()),
            TagFilter::Equals(b.clone()),
        ]);
        assert!(f.matches(&tags_ab));
        assert!(!f.matches(&tags_a));
        assert!(TagFilter::And(vec![]).matches(&tags_a));
        assert!(TagFilter::And(vec![]).matches(&HashSet::new()));
    }

    #[test]
    fn tag_filter_or_requires_any_including_empty_vacuous_false() {
        let a = Tag::new("a").unwrap();
        let b = Tag::new("b").unwrap();
        let tags_a: HashSet<Tag> = [a.clone()].into_iter().collect();
        let f = TagFilter::Or(vec![
            TagFilter::Equals(a.clone()),
            TagFilter::Equals(b.clone()),
        ]);
        assert!(f.matches(&tags_a));
        assert!(!TagFilter::Or(vec![]).matches(&tags_a));
    }

    #[test]
    fn tag_filter_nested_and_or() {
        let tags: HashSet<Tag> = [Tag::new("order:o1").unwrap()].into_iter().collect();
        let f = TagFilter::And(vec![
            TagFilter::StartsWith("order:".into()),
            TagFilter::Equals(Tag::new("order:o1").unwrap()),
        ]);
        assert!(f.matches(&tags));
        let tags2: HashSet<Tag> = [Tag::new("order:o2").unwrap()].into_iter().collect();
        assert!(!f.matches(&tags2));
    }

    #[test]
    fn tag_filter_serde_equals_roundtrip() {
        let f = TagFilter::Equals(Tag::new("order:o1").unwrap());
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"Equals":"order:o1"}"#);
        let decoded: TagFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, f);
    }

    #[test]
    fn tag_filter_serde_starts_with_roundtrip() {
        let f = TagFilter::StartsWith("order:".into());
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"StartsWith":"order:"}"#);
        let decoded: TagFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, f);
    }

    #[test]
    fn tag_filter_serde_ends_with_roundtrip() {
        let f = TagFilter::EndsWith(":o1".into());
        let decoded: TagFilter = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(decoded, f);
    }

    #[test]
    fn tag_filter_serde_nested_roundtrip() {
        let f = TagFilter::And(vec![
            TagFilter::Equals(Tag::new("order:o1").unwrap()),
            TagFilter::Or(vec![
                TagFilter::StartsWith("premium:".into()),
                TagFilter::EndsWith(":vip".into()),
            ]),
        ]);
        let json = serde_json::to_string(&f).unwrap();
        let decoded: TagFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, f);
    }

    #[test]
    fn query_serde_roundtrip() {
        let q = Query::match_tags([Tag::new("order:o1").unwrap()]);
        let json = serde_json::to_string(&q).unwrap();
        let decoded: Query = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, q);
    }

    #[test]
    fn query_match_tags_produces_and_of_equals() {
        // D-08: match_tags must produce TagFilter::And(Equals, ...)
        let q = Query::match_tags([Tag::new("a").unwrap(), Tag::new("b").unwrap()]);
        assert_eq!(q.criteria.len(), 1);
        match &q.criteria[0].tag_filter {
            Some(TagFilter::And(filters)) => {
                assert_eq!(filters.len(), 2);
                for f in filters {
                    match f {
                        TagFilter::Equals(_) => {}
                        other => panic!("expected Equals, got {:?}", other),
                    }
                }
            }
            other => panic!("expected Some(And(..)), got {:?}", other),
        }
    }
}
