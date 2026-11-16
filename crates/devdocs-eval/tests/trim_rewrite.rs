//! The trim rewrite mechanics: link drops and retargets, parenthetical
//! titles, the committed-fixture roundtrip, and the empty-index export
//! fallback at the fence boundary (docs/implementation/14 §Fixture
//! corpus, §Trim tool).

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use devdocs_ingest::fixture::RecordedResponse;

use common::{fixtures_dir, run_trim, trim_output};

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
}

/// Negative control for the unclosed-fence refusal: the same input with
/// the fence closed passes, and the rescued empty index gains the export
/// link.
#[test]
fn the_trim_tool_appends_the_export_link_when_the_empty_index_fence_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/llms.txt\nhttps://x.test/llms-full.txt\n",
    )
    .unwrap();
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
