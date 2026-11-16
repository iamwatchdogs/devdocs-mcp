//! The committed fixture corpus: every fixture loads for replay, the
//! inventories match the manifest bytes (docs/implementation/14 §Fixture
//! corpus).

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use devdocs_ingest::fixture::FixtureSet;
use sha2::{Digest, Sha256};

use common::{fixtures_dir, hex, manifest_of};

#[test]
fn every_committed_fixture_manifest_loads_via_fixture_set() {
    for dir in ["next.js/16.2.9", "zod/4.6.7"] {
        let set =
            FixtureSet::load(&fixtures_dir().join(dir)).unwrap_or_else(|e| panic!("{dir}: {e}"));
        let response = manifest_of(dir);
        let first = set
            .serve(&url::Url::parse(&response[0].url).unwrap())
            .expect("the first recorded URL is served");
        assert_eq!(first.body, response[0].body);
    }
}

#[test]
fn manifest_urls_are_https_and_unique_with_success_statuses() {
    for dir in ["next.js/16.2.9", "zod/4.6.7"] {
        let responses = manifest_of(dir);
        assert!(!responses.is_empty(), "{dir}: the manifest is not empty");
        let mut seen = std::collections::HashSet::new();
        for response in &responses {
            assert!(
                response.url.starts_with("https://"),
                "{dir}: {} is not HTTPS",
                response.url
            );
            assert!(
                seen.insert(response.url.as_str()),
                "{dir}: duplicate URL {}",
                response.url
            );
            assert_eq!(response.status, 200, "{dir}: {}", response.url);
        }
    }
}

#[test]
fn inventory_hashes_match_the_manifest_bytes() {
    for dir in ["next.js/16.2.9", "zod/4.6.7"] {
        let base = fixtures_dir().join(dir);
        let responses = manifest_of(dir);
        let bodies: std::collections::HashMap<&str, &str> = responses
            .iter()
            .map(|r| (r.url.as_str(), r.body.as_str()))
            .collect();
        let inventory = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(base.join("inventory.json")).unwrap(),
        )
        .unwrap();
        let rows = inventory["responses"].as_array().unwrap();
        assert_eq!(rows.len(), bodies.len(), "{dir}: one inventory row per URL");
        for row in rows {
            let url = row["url"].as_str().unwrap();
            assert!(
                bodies.contains_key(url),
                "{dir}: {url} missing from the manifest"
            );
            let body = bodies[url];
            let digest = || {
                let mut hasher = Sha256::new();
                hasher.update(body.as_bytes());
                hex(&hasher.finalize())
            };
            match row["trimmed_sha256"].as_str() {
                Some(trimmed) => assert_eq!(trimmed, digest(), "{dir}: {url} trimmed hash"),
                None => assert_eq!(
                    row["original_sha256"].as_str().unwrap(),
                    digest(),
                    "{dir}: {url} original hash"
                ),
            }
        }
    }
}
