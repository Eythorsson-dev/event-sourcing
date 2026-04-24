use crate::types::EventType;

/// Errors that can occur during projection engine fold operations.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("unknown event type '{0}' has no EventSchemaDef")]
    UnknownEventType(EventType),

    #[error("json path '{path}' not found in payload for event type '{event_type}'")]
    FieldNotFound { event_type: EventType, path: String },

    #[error("invalid JSON path expression '{path}'")]
    InvalidPath { path: String },

    #[error("deserialization failed: {0}")]
    DeserializationFailed(#[from] serde_json::Error),

    #[error("type mismatch on field '{field}': expected numeric, got {actual}")]
    TypeMismatch { field: String, actual: String },
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_error_display() {
        let et = EventType::from("CustomerRegistered");

        let e = ProjectionError::UnknownEventType(et.clone());
        assert!(
            e.to_string().contains("CustomerRegistered"),
            "UnknownEventType display: {}",
            e
        );

        let e = ProjectionError::FieldNotFound {
            event_type: et.clone(),
            path: "$.name".to_owned(),
        };
        let s = e.to_string();
        assert!(
            s.contains("$.name"),
            "FieldNotFound display missing path: {}",
            s
        );
        assert!(
            s.contains("CustomerRegistered"),
            "FieldNotFound display missing event_type: {}",
            s
        );

        let deserialization_error: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("not-json").unwrap_err();
        let e = ProjectionError::DeserializationFailed(deserialization_error);
        assert!(
            e.to_string().contains("deserialization failed"),
            "DeserializationFailed display: {}",
            e
        );

        let e = ProjectionError::TypeMismatch {
            field: "count".to_owned(),
            actual: "String".to_owned(),
        };
        let s = e.to_string();
        assert!(
            s.contains("count"),
            "TypeMismatch display missing field: {}",
            s
        );
        assert!(
            s.contains("String"),
            "TypeMismatch display missing actual: {}",
            s
        );
    }
}
