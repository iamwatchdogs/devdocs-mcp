//! The committed fixture corpus: every fixture loads for replay, the
//! inventories match the manifest bytes, and the trim tool roundtrips
//! (docs/implementation/14 §Fixture corpus).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use devdocs_ingest::fixture::{FixtureSet, RecordedResponse};
use sha2::{Digest, Sha256};

/// The committed fixture corpus root.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../eval/fixtures")
}

/// The recorded responses of one committed fixture.
fn manifest_of(dir: &str) -> Vec<RecordedResponse> {
    let path = fixtures_dir().join(dir).join("manifest.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("the committed manifest parses")
}

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

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Runs the trim tool with isolated stdlib semantics (`-I`) on the given
/// inputs and fails on a nonzero exit.
fn run_trim(manifest: &Path, keep: &Path, out_manifest: &Path, out_inventory: &Path) {
    let status = Command::new("python3")
        .arg("-I")
        .arg(fixtures_dir().join("trim.py"))
        .arg(manifest)
        .arg(keep)
        .arg(out_manifest)
        .arg(out_inventory)
        .status()
        .expect("python3 runs the trim tool");
    assert!(status.success(), "trim.py exited with {status}");
}

#[test]
fn the_trim_tool_roundtrips_the_committed_fixtures() {
    let keep = fixtures_dir().join("trim-keep.txt");
    for fixture in ["next.js/16.2.9", "zod/4.6.7"] {
        let committed = fixtures_dir().join(fixture).join("manifest.json");
        let out = tempfile::tempdir().unwrap();
        run_trim(
            &committed,
            &keep,
            &out.path().join("manifest.json"),
            &out.path().join("inventory.json"),
        );
        let retrimmed = std::fs::read(out.path().join("manifest.json")).unwrap();
        assert_eq!(
            retrimmed,
            std::fs::read(&committed).unwrap(),
            "{fixture}: trim(trimmed) is the identity and the tool matches the committed bytes"
        );
    }
}

#[test]
fn the_trim_tool_drops_retargets_and_rescues_an_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n## A\n\n- [kept](https://x.test/kept.md)\n- [dropped](https://x.test/dropped)\n- [twin](https://x.test/twin)\n- [fenced](https://x.test/fenced)\n```\n- [in fence](https://x.test/in-fence)\n```\n"},
            {"url": "https://x.test/kept.md", "status": 200, "headers": {}, "body": "# kept\n"},
            {"url": "https://x.test/twin.md", "status": 200, "headers": {}, "body": "# twin\n"}
        ]"##,
    )
    .unwrap();
    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/kept.md\nhttps://x.test/twin.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    run_trim(&manifest, &keep, &out, &dir.path().join("inventory.json"));

    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(trimmed.len(), 3, "the index and the two kept pages survive");
    let index = &trimmed[0].body;
    assert!(
        index.contains("(https://x.test/kept.md)"),
        "kept link stays: {index}"
    );
    assert!(
        index.contains("(https://x.test/twin.md)"),
        "the dropped page's kept .md twin is retargeted: {index}"
    );
    assert!(
        !index.contains("x.test/dropped"),
        "dropped links go: {index}"
    );
    assert!(
        index.contains("- [in fence](https://x.test/in-fence)"),
        "fenced lines are never link lines: {index}"
    );
    assert!(
        !index.contains("(https://x.test/twin)"),
        "the HTML twin link is gone: {index}"
    );
}
