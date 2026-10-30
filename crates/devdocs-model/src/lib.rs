//! Core types, configuration, and errors for devdocs.

pub mod config;
pub mod envelope;
pub mod error;

pub use config::{Config, Overrides, Paths, SourceConfig};
pub use envelope::{Example, ResultEnvelope, ResultItem, empty_envelope};
pub use error::{Error, ErrorEnvelope, MissReason, UpstreamCode};
