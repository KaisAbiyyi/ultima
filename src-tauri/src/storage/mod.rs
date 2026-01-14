//! Storage module for Ultima AI
//!
//! Provides SQLite-based local storage.

mod manager;

pub use manager::get_storage;
pub use manager::init_storage;
pub use manager::Storage;
pub use manager::StorageHandle;
