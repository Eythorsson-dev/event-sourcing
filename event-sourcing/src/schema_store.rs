//! EventSchemaStore trait and implementations.
//! Append-only store for EventSchemaDef records.

use async_trait::async_trait;

use crate::schema::EventSchemaDef;

/// Error type for EventSchemaStore operations.
#[derive(Debug, thiserror::Error)]
pub enum EventSchemaError {
    #[error("storage failure: {0}")]
    StorageFailure(String),
}

/// Append-only store for event schema definitions.
/// Corresponds to D-09 and D-10 from Phase 4 context.
#[async_trait]
pub trait EventSchemaStore: Send + Sync {
    async fn record_if_new(&self, schema: &EventSchemaDef) -> Result<(), EventSchemaError>;
    async fn fetch_all(&self) -> Result<Vec<EventSchemaDef>, EventSchemaError>;
    async fn fetch_one(&self, event_type: &str) -> Result<Option<EventSchemaDef>, EventSchemaError>;
}

/// No-op implementation used as the default type parameter on EventLog.
/// All operations are no-ops — schema tracking is disabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpEventSchemaStore;

#[async_trait]
impl EventSchemaStore for NoOpEventSchemaStore {
    async fn record_if_new(&self, _schema: &EventSchemaDef) -> Result<(), EventSchemaError> {
        Ok(())
    }

    async fn fetch_all(&self) -> Result<Vec<EventSchemaDef>, EventSchemaError> {
        Ok(vec![])
    }

    async fn fetch_one(
        &self,
        _event_type: &str,
    ) -> Result<Option<EventSchemaDef>, EventSchemaError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EventSchemaDef, FieldDef, FieldType};
    use crate::types::EventType;

    fn make_schema(event_type: &str) -> EventSchemaDef {
        EventSchemaDef {
            event_type: EventType::from(event_type),
            fields: vec![FieldDef {
                name: "id".to_owned(),
                field_type: FieldType::String,
                optional: false,
            }],
        }
    }

    #[tokio::test]
    async fn noop_store_record_returns_ok() {
        let store = NoOpEventSchemaStore;
        let schema = make_schema("OrderPlaced");
        let result = store.record_if_new(&schema).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn noop_store_fetch_all_returns_empty() {
        let store = NoOpEventSchemaStore;
        let result = store.fetch_all().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn noop_store_fetch_one_returns_none() {
        let store = NoOpEventSchemaStore;
        let result = store.fetch_one("X").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn noop_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoOpEventSchemaStore>();
    }
}
