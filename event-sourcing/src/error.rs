/// Error from an append operation (D-14).
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error("concurrency conflict: stream has advanced past expected version")]
    ConcurrencyConflict,

    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Error from a read operation (D-15).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage failure: {0}")]
    StorageFailure(#[source] Box<dyn std::error::Error + Send + Sync>),
}
