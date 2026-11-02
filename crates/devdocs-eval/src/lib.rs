//! Evaluation harness for devdocs retrieval quality (docs/implementation/14).
//!
//! The harness loads a golden suite (`eval/golden/*.jsonl`), runs the
//! retrieval pipeline (a stub until M4, docs/implementation/13), scores the
//! metrics, writes the run artifacts, and prints the gate table.

pub mod error;
pub mod gates;
pub mod metrics;
pub mod pipeline;
pub mod refs;
pub mod report;
pub mod run;
pub mod subprocess;
pub mod suite;
