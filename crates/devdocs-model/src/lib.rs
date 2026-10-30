//! Core types, configuration, and errors for devdocs.

pub mod config;
pub mod error;

pub use config::{Config, Overrides, Paths, SourceConfig};
pub use error::{Error, ErrorEnvelope, MissReason, UpstreamCode};
