// state/mod.rs
// Data layer — Redis session state and LanceDB vector store.
// Both are initialised once at startup and shared across all requests via Arc<>.

pub mod lancedb;
pub mod redis;
