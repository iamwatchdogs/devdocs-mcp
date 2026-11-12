//! The MCP-layer result projections the `detail` parameter selects
//! (docs/implementation/08 §MCP adapter).
//!
//! The projection structs carry the envelope-level fields unchanged and
//! trim the per-item payload. The `full` level serializes the CLI
//! envelope directly, so the text content stays byte-identical to the
//! CLI output for the same query against the same index.

use devdocs_model::VersionStatus;
use devdocs_model::envelope::{ResultEnvelope, ResultItem};
use schemars::JsonSchema;
use serde::Serialize;

/// How much of each result item the `search_docs` tool serves. `full` is
/// the default: the exact CLI envelope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    /// The identity of each item: the ref, the title, the served
    /// version, and the version status.
    Name,
    /// Each item without its code examples.
    Summary,
    /// The exact CLI envelope. The default.
    #[default]
    Full,
}

impl Detail {
    /// Serializes the envelope at this detail level to the compact JSON
    /// string the tool result carries as its text content. Field order
    /// follows the struct declaration, so the `full` level is
    /// byte-identical to the CLI output for the same query against the
    /// same index.
    ///
    /// # Errors
    /// Field-only structs cannot fail to serialize, but `serde_json`
    /// forces this shape on us.
    pub fn render(&self, envelope: &ResultEnvelope) -> Result<String, serde_json::Error> {
        match self {
            Self::Full => serde_json::to_string(envelope),
            Self::Summary => serde_json::to_string(&SummaryEnvelope::of(envelope)),
            Self::Name => serde_json::to_string(&NameEnvelope::of(envelope)),
        }
    }
}

/// The `summary` projection of one result item: the CLI item without its
/// `examples` field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SummaryItem {
    /// The document ref, for example `docs://next.js/v16.2.9/proxy#rewrite`.
    #[serde(rename = "ref")]
    pub doc_ref: String,
    /// The section title.
    pub title: String,
    /// The section path, for example `Routing > Proxy`.
    pub section: String,
    /// The matched text.
    pub snippet: String,
    /// The upstream page the section came from.
    pub source_url: String,
    /// The upstream commit the content was captured at.
    pub source_commit: String,
    /// The version the query carries, when any.
    pub requested_version: Option<String>,
    /// The version that actually served the query.
    pub served_version: String,
    /// The version status of the served result.
    pub version_status: VersionStatus,
    /// The hex sha256 of the section content.
    pub content_hash: String,
    /// The license status of the source.
    pub license_status: String,
    /// The attribution line the source's license requires, when one is
    /// recorded (docs/implementation/07 §License gate). Absent from the
    /// JSON when no attribution applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// The fusion confidence score.
    pub confidence: f64,
    /// The token count of the snippet.
    pub token_count: u64,
}

impl SummaryItem {
    /// Projects one CLI result item onto the `summary` shape.
    #[must_use]
    pub fn of(item: &ResultItem) -> Self {
        Self {
            doc_ref: item.doc_ref.clone(),
            title: item.title.clone(),
            section: item.section.clone(),
            snippet: item.snippet.clone(),
            source_url: item.source_url.clone(),
            source_commit: item.source_commit.clone(),
            requested_version: item.requested_version.clone(),
            served_version: item.served_version.clone(),
            version_status: item.version_status,
            content_hash: item.content_hash.clone(),
            license_status: item.license_status.clone(),
            attribution: item.attribution.clone(),
            confidence: item.confidence,
            token_count: item.token_count,
        }
    }
}

/// The `summary` projection of the result envelope: the envelope-level
/// fields stay, each item drops its `examples` field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SummaryEnvelope {
    /// The projected results, in fusion order.
    pub results: Vec<SummaryItem>,
    /// Whether the budget cut the result list short.
    pub truncated: bool,
    /// The cursor for the next page, when results were truncated.
    pub next_cursor: Option<String>,
    /// The RFC 3339 time the envelope was served at.
    pub served_at: String,
}

impl SummaryEnvelope {
    /// Projects the CLI envelope onto the `summary` shape.
    #[must_use]
    pub fn of(envelope: &ResultEnvelope) -> Self {
        Self {
            results: envelope.results.iter().map(SummaryItem::of).collect(),
            truncated: envelope.truncated,
            next_cursor: envelope.next_cursor.clone(),
            served_at: envelope.served_at.clone(),
        }
    }
}

/// The `name` projection of one result item: only the identity fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct NameItem {
    /// The document ref, for example `docs://next.js/v16.2.9/proxy#rewrite`.
    #[serde(rename = "ref")]
    pub doc_ref: String,
    /// The section title.
    pub title: String,
    /// The version that actually served the query.
    pub served_version: String,
    /// The version status of the served result.
    pub version_status: VersionStatus,
}

impl NameItem {
    /// Projects one CLI result item onto the `name` shape.
    #[must_use]
    pub fn of(item: &ResultItem) -> Self {
        Self {
            doc_ref: item.doc_ref.clone(),
            title: item.title.clone(),
            served_version: item.served_version.clone(),
            version_status: item.version_status,
        }
    }
}

/// The `name` projection of the result envelope: the envelope-level
/// fields stay, each item keeps only its identity fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct NameEnvelope {
    /// The projected results, in fusion order.
    pub results: Vec<NameItem>,
    /// Whether the budget cut the result list short.
    pub truncated: bool,
    /// The cursor for the next page, when results were truncated.
    pub next_cursor: Option<String>,
    /// The RFC 3339 time the envelope was served at.
    pub served_at: String,
}

impl NameEnvelope {
    /// Projects the CLI envelope onto the `name` shape.
    #[must_use]
    pub fn of(envelope: &ResultEnvelope) -> Self {
        Self {
            results: envelope.results.iter().map(NameItem::of).collect(),
            truncated: envelope.truncated,
            next_cursor: envelope.next_cursor.clone(),
            served_at: envelope.served_at.clone(),
        }
    }
}

/// The `verify_doc` tool result: the citation check outcome with the
/// content hash the section served under (docs/implementation/08 §MCP
/// adapter). Field order is declaration order: `valid` first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct VerifyReport {
    /// Whether the ref resolved to a section whose hash matches the
    /// index. Always `true` on a success result: a mismatch is the
    /// tool-level error envelope.
    pub valid: bool,
    /// The hex sha256 of the section content at serve time.
    pub content_hash: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Detail, NameEnvelope, SummaryEnvelope, VerifyReport};
    use devdocs_model::VersionStatus;
    use devdocs_model::envelope::{Example, ResultEnvelope, ResultItem};

    /// One sample item with every field filled, the shape the projection
    /// snapshots start from.
    fn sample_item(ref_id: &str) -> ResultItem {
        ResultItem {
            doc_ref: ref_id.to_owned(),
            title: String::from("Proxy"),
            section: String::from("Routing > Proxy"),
            snippet: String::from("Rewrites the request before the handler runs."),
            examples: vec![Example {
                language: String::from("ts"),
                code: String::from("export const proxy = {}"),
            }],
            source_url: String::from("https://docs.test/next.js/proxy"),
            source_commit: String::from("abc123"),
            requested_version: Some(String::from("16.2.9")),
            served_version: String::from("16.2.9"),
            version_status: VersionStatus::Verified,
            content_hash: String::from("deadbeef"),
            license_status: String::from("approved_index"),
            attribution: Some(String::from("© Vercel")),
            confidence: 0.75,
            token_count: 42,
        }
    }

    /// One sample envelope with two items and a fixed serve time, so the
    /// snapshots are deterministic.
    fn sample_envelope() -> ResultEnvelope {
        ResultEnvelope {
            results: vec![
                sample_item("docs://next.js/v16.2.9/proxy"),
                sample_item("docs://next.js/v16.2.9/routing"),
            ],
            truncated: true,
            next_cursor: Some(String::from("12")),
            served_at: String::from("2026-01-01T00:00:00Z"),
        }
    }

    /// The `full` level serializes the CLI envelope directly: the JSON
    /// carries every field in declaration order, including `examples`.
    #[test]
    fn full_renders_the_exact_cli_envelope() {
        let envelope = sample_envelope();
        let json = Detail::Full.render(&envelope).unwrap();
        assert!(json.contains("\"examples\""), "json: {json}");
        assert!(json.starts_with("{\"results\":"));
        insta::assert_json_snapshot!(
            "full_render",
            serde_json::from_str::<serde_json::Value>(&json).unwrap()
        );
    }

    /// The `summary` level drops `examples` from each item and keeps the
    /// envelope-level fields and every other item field.
    #[test]
    fn summary_drops_examples_only() {
        let envelope = sample_envelope();
        let json = Detail::Summary.render(&envelope).unwrap();
        assert!(!json.contains("\"examples\""), "json: {json}");
        assert!(json.contains("\"attribution\""), "json: {json}");
        insta::assert_json_snapshot!(
            "summary_render",
            serde_json::from_str::<serde_json::Value>(&json).unwrap()
        );
    }

    /// The `name` level keeps only the identity fields per item; the
    /// envelope-level fields stay.
    #[test]
    fn name_keeps_identity_fields_only() {
        let envelope = sample_envelope();
        let json = Detail::Name.render(&envelope).unwrap();
        assert!(json.contains("\"ref\""), "json: {json}");
        assert!(!json.contains("\"snippet\""), "json: {json}");
        insta::assert_json_snapshot!(
            "name_render",
            serde_json::from_str::<serde_json::Value>(&json).unwrap()
        );
    }

    /// The projection constructors carry the envelope-level fields
    /// through unchanged.
    #[test]
    fn projections_carry_envelope_fields() {
        let envelope = sample_envelope();
        let summary = SummaryEnvelope::of(&envelope);
        assert!(summary.truncated);
        assert_eq!(summary.next_cursor.as_deref(), Some("12"));
        assert_eq!(summary.served_at, "2026-01-01T00:00:00Z");
        assert_eq!(summary.results.len(), 2);
        let name = NameEnvelope::of(&envelope);
        assert_eq!(name.results.len(), 2);
        assert_eq!(name.results[0].doc_ref, "docs://next.js/v16.2.9/proxy");
        assert_eq!(name.results[0].title, "Proxy");
        assert_eq!(name.results[0].served_version, "16.2.9");
    }

    /// The verify report serializes with `valid` first, `content_hash`
    /// second (docs/implementation/08 §MCP adapter).
    #[test]
    fn verify_report_serializes_in_declaration_order() {
        let json = serde_json::to_string(&VerifyReport {
            valid: true,
            content_hash: String::from("deadbeef"),
        })
        .unwrap();
        assert_eq!(json, "{\"valid\":true,\"content_hash\":\"deadbeef\"}");
    }
}
