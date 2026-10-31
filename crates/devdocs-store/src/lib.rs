//! SQLite storage, schema, and migrations for devdocs.
//!
//! The store is the only crate that touches SQLite
//! (docs/implementation/05 §Concurrency model). It wraps a
//! `Mutex<Connection>`; async callers reach it through
//! `tokio::task::spawn_blocking` and release the guard before every await.

mod error;
mod migrate;

pub use error::StoreError;
pub use migrate::{AppliedMigration, migrate};
