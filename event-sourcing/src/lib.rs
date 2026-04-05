pub mod error;
pub mod event;
pub mod store;
pub mod types;

// Re-export primary types at crate root
pub use error::{AppendError, StoreError};
pub use event::{NewEvent, StoredEvent};
pub use store::{AppendCondition, LogStore};
pub use types::{EmptyStreamIdError, GlobalSequenceId, StreamId, StreamSequenceId};
