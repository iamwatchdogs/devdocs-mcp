//! The unified result envelope (docs/implementation/08 §CLI contract).
//!
//! The CLI prints this envelope on stdout under `--json`; the MCP server
//! reuses the shape for structured output. The per-result fields follow the
//! docs/08 example exactly; placeholder commands print the empty envelope.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// One search/open/symbol result in the docs/08 result envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultItem {
    /// The document ref, for example `docs://next.js/v16.2.9/proxy#rewrite`.
    #[serde(rename = "ref")]
    pub doc_ref: String,
    /// The section title.
    pub title: String,
    /// The section path, for example `Routing > Proxy`.
    pub section: String,
    /// The matched text.
    pub snippet: String,
    /// Code examples in the section.
    pub examples: Vec<Example>,
    /// The upstream page the section came from.
    pub source_url: String,
    /// The upstream commit the content was captured at.
    pub source_commit: String,
    /// The version the query carries, when any.
    pub requested_version: Option<String>,
    /// The version that actually served the query.
    pub served_version: String,
    /// The version status: `verified`, `alias`, `fallback`, or `absent`.
    pub version_status: String,
    /// The hex sha256 of the section content.
    pub content_hash: String,
    /// The license status of the source.
    pub license_status: String,
    /// The fusion confidence score.
    pub confidence: f64,
    /// The token count of the snippet.
    pub token_count: u64,
}

/// A code example in a result item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    /// The example language, for example `tsx`.
    pub language: String,
    /// The example code.
    pub code: String,
}

/// The result envelope printed on stdout (docs/08 top-level fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultEnvelope {
    /// The results, in fusion order.
    pub results: Vec<ResultItem>,
    /// Whether the budget cut the result list short.
    pub truncated: bool,
    /// The cursor for the next page, when results were truncated.
    pub next_cursor: Option<String>,
    /// The RFC 3339 time the envelope was served at.
    pub served_at: String,
}

/// Build the empty result envelope that placeholder commands print.
#[must_use]
pub fn empty_envelope() -> ResultEnvelope {
    ResultEnvelope {
        results: Vec::new(),
        truncated: false,
        next_cursor: None,
        served_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
    }
}
