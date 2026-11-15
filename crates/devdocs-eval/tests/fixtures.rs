//! The committed fixture corpus: every fixture loads for replay, the
//! inventories match the manifest bytes, and the trim tool roundtrips
//! (docs/implementation/14 §Fixture corpus).

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
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
    let (success, stderr) = trim_output(manifest, keep, out_manifest, out_inventory);
    assert!(success, "trim.py failed: {stderr}");
}

/// Runs the trim tool and returns whether it succeeded, with its stderr.
fn trim_output(
    manifest: &Path,
    keep: &Path,
    out_manifest: &Path,
    out_inventory: &Path,
) -> (bool, String) {
    let output = Command::new("python3")
        .arg("-I")
        .arg(fixtures_dir().join("trim.py"))
        .arg(manifest)
        .arg(keep)
        .arg(out_manifest)
        .arg(out_inventory)
        .output()
        .expect("python3 runs the trim tool");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
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

#[test]
fn the_trim_tool_retargets_only_the_destination_of_a_parenthetical_title() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Guide (https://x.test/guide)](https://x.test/guide)\n"},
            {"url": "https://x.test/guide.md", "status": 200, "headers": {}, "body": "# guide\n"}
        ]"##,
    )
    .unwrap();
    let keep = dir.path().join("keep.txt");
    std::fs::write(&keep, "https://x.test/llms.txt\nhttps://x.test/guide.md\n").unwrap();
    let out = dir.path().join("trimmed.json");
    run_trim(&manifest, &keep, &out, &dir.path().join("inventory.json"));

    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let index = &trimmed[0].body;
    assert!(
        index.contains("- [Guide (https://x.test/guide)](https://x.test/guide.md)"),
        "the title keeps its parentheses, only the destination is retargeted: {index}"
    );
    assert!(
        !index.contains("Guide(https"),
        "the title is never corrupted into the destination: {index}"
    );
}

#[test]
fn the_trim_tool_refuses_to_rescue_an_empty_index_that_ends_inside_a_fence() {
    let dir = tempfile::tempdir().unwrap();
    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/llms-full.txt\n",
    )
    .unwrap();

    // The rewrite drops the index's only link line, and the body ends
    // inside an unclosed fence: appending the export link there would
    // stay fenced, so the fallback must fail loudly instead.
    let fenced = dir.path().join("fenced.json");
    std::fs::write(
        &fenced,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [dropped](https://x.test/dropped)\n\n```\nunterminated\n"},
            {"url": "https://x.test/llms-full.txt", "status": 200, "headers": {}, "body": "# X full\n"}
        ]"##,
    )
    .unwrap();
    let (_, stderr) = trim_output(
        &fenced,
        &keep,
        &dir.path().join("out.json"),
        &dir.path().join("out-inventory.json"),
    );
    assert!(
        stderr.contains("ends inside an unclosed code fence"),
        "the empty-index fallback refuses a body inside an unclosed fence: {stderr}"
    );

    // Negative control: the same input with the fence closed passes and
    // gains the export link.
    let closed = dir.path().join("closed.json");
    std::fs::write(
        &closed,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [dropped](https://x.test/dropped)\n\n```\nunterminated\n```\n"},
            {"url": "https://x.test/llms-full.txt", "status": 200, "headers": {}, "body": "# X full\n"}
        ]"##,
    )
    .unwrap();
    let out = dir.path().join("closed-trimmed.json");
    run_trim(
        &closed,
        &keep,
        &out,
        &dir.path().join("closed-inventory.json"),
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let index = &trimmed[0].body;
    assert!(
        index.contains("- [Full documentation](https://x.test/llms-full.txt)"),
        "the closed index gains the export link: {index}"
    );
}

/// The redirect dual-entry recording: the subpath index is recorded under
/// both the requested and the final URL, and the keep list holds only the
/// final-URL entry. The root's link line to the requested URL drops, so
/// the kept index sits outside the reachable closure — the replay never
/// loads it, and its page must come from the rescue (a merged line in the
/// reachable root) instead of silently from the unreachable index.
#[test]
fn the_trim_tool_rescues_pages_of_an_unreachable_subpath_index() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Intro](https://x.test/intro.md)\n- [Guide](https://x.test/guide/llms.txt)\n"},
            {"url": "https://x.test/guide/llms.txt", "status": 200, "headers": {}, "body": "# Guide\n\n- [Page](https://x.test/guide/v2/page.md)\n"},
            {"url": "https://x.test/guide/v2/llms.txt", "status": 200, "headers": {}, "body": "# Guide\n\n- [Page](https://x.test/guide/v2/page.md)\n"},
            {"url": "https://x.test/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/guide/v2/page.md", "status": 200, "headers": {}, "body": "# page\n"}
        ]"##,
    )
    .unwrap();

    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\n\
         https://x.test/guide/v2/llms.txt\nhttps://x.test/guide/v2/page.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        stderr.contains("no reachable index listed"),
        "the unreachable index's page is rescued loudly: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(
        trimmed.len(),
        4,
        "the root, the intro page, the final-URL index, and the page survive"
    );
    assert!(
        trimmed[0]
            .body
            .contains("(https://x.test/guide/v2/page.md)"),
        "the rescued page line lands in the reachable root: {}",
        trimmed[0].body
    );

    // The rescue lines sit in a reachable host, so re-trimming the
    // rescued manifest with the same keep list is the identity.
    let retrimmed = dir.path().join("retrimmed.json");
    run_trim(
        &out,
        &keep,
        &retrimmed,
        &dir.path().join("re-inventory.json"),
    );
    assert_eq!(
        std::fs::read(&retrimmed).unwrap(),
        std::fs::read(&out).unwrap(),
        "trim(trimmed) is the identity"
    );

    // Negative control: the root keeps the link to the requested URL, so
    // the subpath index is reachable and every kept page is linked
    // without a rescue — replay parity.
    let keep_linked = dir.path().join("keep-linked.txt");
    std::fs::write(
        &keep_linked,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\n\
         https://x.test/guide/llms.txt\nhttps://x.test/guide/v2/llms.txt\n\
         https://x.test/guide/v2/page.md\n",
    )
    .unwrap();
    let out_linked = dir.path().join("trimmed-linked.json");
    let (_, stderr) = trim_output(
        &manifest,
        &keep_linked,
        &out_linked,
        &dir.path().join("inventory-linked.json"),
    );
    assert!(
        !stderr.contains("no reachable index listed"),
        "the reachable index needs no rescue: {stderr}"
    );
    let linked: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out_linked).unwrap()).unwrap();
    assert!(
        linked[0].body.contains("(https://x.test/guide/llms.txt)"),
        "the root keeps the subpath index link: {}",
        linked[0].body
    );
    let requested = linked
        .iter()
        .find(|r| r.url == "https://x.test/guide/llms.txt")
        .expect("the requested subpath index survives");
    assert!(
        requested.body.contains("(https://x.test/guide/v2/page.md)"),
        "the reachable index links the page: {}",
        requested.body
    );
}

/// The subpath index is discovered only through a kept page's
/// `Link: rel="describedby"` header (relative target resolved against the
/// page URL) — the root never links it. With the describer kept, the
/// header queues the index and its page is linked without a rescue; with
/// the describer dropped, the index is unreachable and the rescue must
/// carry the page loudly; with the describer kept but the index dropped,
/// trim fails loudly — the header is never rewritten, so the replay
/// would follow it and miss. A keep list without the root index cannot
/// seed the replay at all and fails.
#[test]
fn the_trim_tool_follows_describedby_headers_for_reachability() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Intro](https://x.test/intro.md)\n- [Api](https://x.test/api.html)\n"},
            {"url": "https://x.test/api.html", "status": 200, "headers": {"link": "</guide/llms.txt>; rel=\"describedby\", </style.css>; rel=\"stylesheet\""}, "body": "# api\n"},
            {"url": "https://x.test/guide/llms.txt", "status": 200, "headers": {}, "body": "# Guide\n\n- [Page](https://x.test/guide/page.md)\n"},
            {"url": "https://x.test/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/guide/page.md", "status": 200, "headers": {}, "body": "# page\n"}
        ]"##,
    )
    .unwrap();

    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\nhttps://x.test/api.html\n\
         https://x.test/guide/llms.txt\nhttps://x.test/guide/page.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        !stderr.contains("no reachable index listed"),
        "the describedby header queues the index, so no rescue fires: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let index = trimmed
        .iter()
        .find(|r| r.url == "https://x.test/guide/llms.txt")
        .expect("the describedby-discovered index survives");
    assert!(
        index.body.contains("(https://x.test/guide/page.md)"),
        "the reachable index keeps its page link: {}",
        index.body
    );

    // Dropped describer: the index is unreachable, so its page must be
    // rescued into the reachable root instead of passing silently.
    let keep_dropped = dir.path().join("keep-dropped.txt");
    std::fs::write(
        &keep_dropped,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\n\
         https://x.test/guide/llms.txt\nhttps://x.test/guide/page.md\n",
    )
    .unwrap();
    let out_dropped = dir.path().join("trimmed-dropped.json");
    let (_, stderr) = trim_output(
        &manifest,
        &keep_dropped,
        &out_dropped,
        &dir.path().join("inventory-dropped.json"),
    );
    assert!(
        stderr.contains("no reachable index listed"),
        "the dropped describer leaves the index unreachable; the page is rescued: {stderr}"
    );
    let dropped: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out_dropped).unwrap()).unwrap();
    assert!(
        dropped[0].body.contains("(https://x.test/guide/page.md)"),
        "the rescued page line lands in the reachable root: {}",
        dropped[0].body
    );

    // A kept describer whose describedby header names a dropped index
    // must fail loudly: the header is never rewritten, so the replay
    // would follow it and miss the kept page's covering index.
    let keep_no_index = dir.path().join("keep-no-index.txt");
    std::fs::write(
        &keep_no_index,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\nhttps://x.test/api.html\n",
    )
    .unwrap();
    let (_, stderr) = trim_output(
        &manifest,
        &keep_no_index,
        &dir.path().join("out-no-index.json"),
        &dir.path().join("inventory-no-index.json"),
    );
    assert!(
        stderr.contains("describedby") && stderr.contains("https://x.test/guide/llms.txt"),
        "a kept page naming a dropped index fails loudly: {stderr}"
    );

    // A keep list without the root index cannot seed the replay: trim
    // fails loudly instead of writing a corpus nothing can load.
    let keep_no_root = dir.path().join("keep-no-root.txt");
    std::fs::write(&keep_no_root, "https://x.test/intro.md\n").unwrap();
    let (_, stderr) = trim_output(
        &manifest,
        &keep_no_root,
        &dir.path().join("out.json"),
        &dir.path().join("out-inventory.json"),
    );
    assert!(
        stderr.contains("no scope to seed"),
        "a dropped root index fails loudly: {stderr}"
    );
}

/// A kept page's `Link: rel="describedby"` header naming a non-index
/// target (a `.md` page) must neither fail the missing-index check nor
/// enter the reachable closure: non-index targets are filtered out
/// (`is_index_url`, mirrored from the ingest pipeline), and the replay
/// never fetches them.
#[test]
fn the_trim_tool_excludes_non_index_describedby_targets() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Api](https://x.test/api.html)\n"},
            {"url": "https://x.test/api.html", "status": 200, "headers": {"link": "</extra.md>; rel=\"describedby\""}, "body": "# api\n"}
        ]"##,
    )
    .unwrap();

    // extra.md is recorded but dropped: naming it must not read as a
    // dropped index.
    let keep = dir.path().join("keep.txt");
    std::fs::write(&keep, "https://x.test/llms.txt\nhttps://x.test/api.html\n").unwrap();
    let out = dir.path().join("trimmed.json");
    let (success, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        success,
        "a describedby header naming a .md target must not fail the trim: {stderr}"
    );
    assert!(
        !stderr.contains("describedby"),
        "the non-index target must not trip the missing-index check: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert!(
        trimmed[0].body.contains("(https://x.test/api.html)"),
        "the root still links the kept describer: {}",
        trimmed[0].body
    );
}

/// A page's `Link:` header naming two describedby indexes must behave
/// like the Rust parser: one slot, last value wins. The earlier target
/// never queues and never lends its links — a page listed only there is
/// rescued into the reachable root, exactly as if the header had named
/// the later index alone.
#[test]
fn the_trim_tool_queues_only_the_last_describedby_header_target() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Intro](https://x.test/intro.md)\n- [Api](https://x.test/api.html)\n"},
            {"url": "https://x.test/api.html", "status": 200, "headers": {"link": "</a/llms.txt>; rel=\"describedby\", </b/llms.txt>; rel=\"describedby\""}, "body": "# api\n"},
            {"url": "https://x.test/a/llms.txt", "status": 200, "headers": {}, "body": "# A\n\n- [Only](https://x.test/a/only.md)\n"},
            {"url": "https://x.test/b/llms.txt", "status": 200, "headers": {}, "body": "# B\n\n- [Page](https://x.test/b/page.md)\n"},
            {"url": "https://x.test/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/a/only.md", "status": 200, "headers": {}, "body": "# only\n"},
            {"url": "https://x.test/b/page.md", "status": 200, "headers": {}, "body": "# page\n"}
        ]"##,
    )
    .unwrap();

    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\nhttps://x.test/api.html\n\
         https://x.test/a/llms.txt\nhttps://x.test/b/llms.txt\n\
         https://x.test/a/only.md\nhttps://x.test/b/page.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        stderr.contains("no reachable index listed"),
        "the page listed only under the earlier target is rescued: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(
        trimmed.len(),
        7,
        "every kept response survives the two-target trim"
    );
    assert!(
        trimmed[0].body.contains("(https://x.test/a/only.md)"),
        "the rescued page line lands in the reachable root: {}",
        trimmed[0].body
    );

    // Replay parity: the rescue sits in the reachable root the replay
    // loads, so re-trimming the trimmed manifest is the identity.
    let retrimmed = dir.path().join("retrimmed.json");
    run_trim(
        &out,
        &keep,
        &retrimmed,
        &dir.path().join("re-inventory.json"),
    );
    assert_eq!(
        std::fs::read(&retrimmed).unwrap(),
        std::fs::read(&out).unwrap(),
        "trim(trimmed) is the identity"
    );
}

/// Within one link value, the last `rel` parameter wins — the Rust
/// parser overwrites its rel accumulator per parameter. A value whose
/// final rel is not `describedby` never counts, so the earlier value's
/// target stays the one that queues: the trim accepts a header the
/// replay can load.
#[test]
fn the_trim_tool_ignores_a_value_whose_last_rel_is_not_describedby() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/llms.txt", "status": 200, "headers": {}, "body": "# X\n\n- [Intro](https://x.test/intro.md)\n- [Api](https://x.test/api.html)\n"},
            {"url": "https://x.test/api.html", "status": 200, "headers": {"link": "</a/llms.txt>; rel=\"describedby\", </b/llms.txt>; rel=\"describedby\"; rel=\"stylesheet\""}, "body": "# api\n"},
            {"url": "https://x.test/a/llms.txt", "status": 200, "headers": {}, "body": "# A\n\n- [Only](https://x.test/a/only.md)\n"},
            {"url": "https://x.test/b/llms.txt", "status": 200, "headers": {}, "body": "# B\n"},
            {"url": "https://x.test/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/a/only.md", "status": 200, "headers": {}, "body": "# only\n"}
        ]"##,
    )
    .unwrap();

    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/intro.md\nhttps://x.test/api.html\n\
         https://x.test/a/llms.txt\nhttps://x.test/a/only.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (success, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        success,
        "the kept earlier target needs no dropped index, so the trim passes: {stderr}"
    );
    assert!(
        !stderr.contains("no reachable index listed"),
        "the queued earlier target is reachable and links its page: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert!(
        trimmed.iter().any(|r| r.url == "https://x.test/a/llms.txt"),
        "the earlier value's target queues: {:?}",
        trimmed.iter().map(|r| r.url.as_str()).collect::<Vec<_>>()
    );
    assert!(
        !trimmed.iter().any(|r| r.url == "https://x.test/b/llms.txt"),
        "the non-describedby value's target never queues"
    );
}
