//! The shared command wiring (docs/implementation/08 §MCP adapter): the
//! store-open path with its data-directory creation, and the attribution
//! join the CLI edge and the MCP edge both run.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use devdocs_model::config::{Config, SourceConfig};
use devdocs_model::envelope::{ResultEnvelope, ResultItem, VersionStatus};
use devdocs_retrieve::wiring;

/// A minimal served item: every field filled, only the ref, the
/// attribution, and the token count vary between the assertions.
fn item(doc_ref: &str, attribution: Option<&str>) -> ResultItem {
    ResultItem {
        doc_ref: String::from(doc_ref),
        title: String::from("Routing"),
        section: String::from("Guide > Routing"),
        snippet: String::from("The routing layer maps requests."),
        examples: Vec::new(),
        source_url: String::from("https://acme.test/guide"),
        source_commit: String::new(),
        requested_version: None,
        served_version: String::from("1.0.0"),
        version_status: VersionStatus::Verified,
        content_hash: "0".repeat(64),
        license_status: String::from("approved_index"),
        attribution: attribution.map(String::from),
        confidence: 1.0,
        token_count: 8,
    }
}

fn config_with_acme_attribution() -> Config {
    let mut config = devdocs_testkit::base_config();
    config.sources.insert(
        String::from("acme"),
        SourceConfig {
            attribution: Some(String::from("Acme Docs (MIT)")),
            ..Default::default()
        },
    );
    config
}

/// An open into a data root that does not exist yet creates the missing
/// directory levels with private mode, so a fresh machine's first `add`
/// or `list` does not fail on a missing directory
/// (docs/implementation/09 §Storage).
#[test]
fn open_store_creates_a_missing_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = devdocs_testkit::base_config();
    config.index.path = dir.path().join("nested").join("deeper").join("docs.db");
    wiring::open_store(&config, false).unwrap();
    let parent = config.index.path.parent().unwrap();
    assert!(parent.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(parent).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "the data root must stay private");
    }
}

/// The attribution join keeps an attribution an earlier step recorded,
/// fills the declarative line for items with none, and leaves items
/// whose ref parses to no registry entry unattributed
/// (docs/implementation/07 §License gate).
#[test]
fn attribute_results_keeps_fills_and_skips_attributions() {
    let config = config_with_acme_attribution();
    let mut envelope = ResultEnvelope {
        results: vec![
            item("docs://acme/v1.0.0/guide#routing", Some("kept")),
            item("docs://acme/v1.0.0/api#client", None),
            item("not a doc ref", None),
        ],
        truncated: false,
        next_cursor: None,
        served_at: String::from("2026-11-12T00:00:00Z"),
    };
    wiring::attribute_results(&config, &mut envelope);
    assert_eq!(envelope.results[0].attribution.as_deref(), Some("kept"));
    assert_eq!(
        envelope.results[1].attribution.as_deref(),
        Some("Acme Docs (MIT)")
    );
    assert_eq!(envelope.results[2].attribution, None);
}
