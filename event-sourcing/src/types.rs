use std::collections::HashSet;
use std::fmt;

/// Error returned when an empty tag is provided.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("tag must not be empty")]
pub struct InvalidTag;

/// An opaque tag that identifies an event classification or membership.
/// Non-empty. The library does not interpret tag contents or enforce any key:value convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Tag(String);

impl Tag {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidTag> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidTag);
        }
        Ok(Tag(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Global sequence ID across all streams. Monotonically increasing. Starts at 1, ZERO = no events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlobalSequenceId(u64);

impl GlobalSequenceId {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for GlobalSequenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An event submitted for appending. No sequence IDs or timestamp — those are assigned by the store.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub tags: HashSet<Tag>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_new_valid() {
        let tag = Tag::new("course:c1").unwrap();
        assert_eq!(tag.as_str(), "course:c1");
    }

    #[test]
    fn tag_new_empty_returns_error() {
        let result = Tag::new("");
        assert_eq!(result, Err(InvalidTag));
    }

    #[test]
    fn tag_equality() {
        let a = Tag::new("premium").unwrap();
        let b = Tag::new("premium").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn tag_hash_set() {
        let mut set = HashSet::new();
        let t = Tag::new("order:o1").unwrap();
        set.insert(t.clone());
        assert!(set.contains(&t));
        assert_eq!(set.len(), 1);
        // Inserting the same tag again doesn't increase size
        set.insert(Tag::new("order:o1").unwrap());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn global_sequence_zero_is_zero() {
        assert_eq!(GlobalSequenceId::ZERO.get(), 0);
    }

    #[test]
    fn global_sequence_ord() {
        let a = GlobalSequenceId::new(1);
        let b = GlobalSequenceId::new(2);
        assert!(a < b);
    }

    #[test]
    fn new_event_can_be_constructed() {
        let event = NewEvent {
            event_type: "OrderPlaced".to_string(),
            payload: serde_json::json!({"order_id": "123"}),
            tags: [Tag::new("order:o1").unwrap()].into_iter().collect(),
        };
        assert_eq!(event.event_type, "OrderPlaced");
        assert_eq!(event.tags.len(), 1);
    }
}
