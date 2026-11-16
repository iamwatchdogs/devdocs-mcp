//! Reachability for the trim tool: the reachable closure from the kept
//! indexes, the loud failure when a kept page has no covering index, the
//! rescue of pages behind unreachable or dropped indexes, and the
//! `Link: rel="describedby"` discovery (docs/implementation/14 §Trim
//! tool).

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use devdocs_ingest::fixture::RecordedResponse;

use common::{run_trim, trim_output};

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
}

/// Negative control for the unreachable-subpath rescue: the root keeps
/// the link to the requested URL, so the subpath index is reachable and
/// every kept page is linked without a rescue — replay parity.
#[test]
fn a_reachable_subpath_index_needs_no_rescue() {
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
         https://x.test/guide/llms.txt\nhttps://x.test/guide/v2/llms.txt\n\
         https://x.test/guide/v2/page.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        !stderr.contains("no reachable index listed"),
        "the reachable index needs no rescue: {stderr}"
    );
    let linked: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
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

/// The rescue host must cover the page: replay ingests a page only under
/// the scope whose directory is the longest path prefix of the page
/// (`most_specific_scope`), so a root index at `/docs/` can never host a
/// page under `/api/`.
#[test]
fn the_trim_tool_fails_when_no_reachable_index_covers_a_kept_page() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/docs/llms.txt", "status": 200, "headers": {}, "body": "# Docs\n\n- [Api](https://x.test/api/llms.txt)\n- [Sub Api](https://x.test/docs/api/llms.txt)\n- [Intro](https://x.test/docs/intro.md)\n"},
            {"url": "https://x.test/api/llms.txt", "status": 200, "headers": {}, "body": "# Api\n\n- [Page](https://x.test/api/intro.md)\n"},
            {"url": "https://x.test/docs/api/llms.txt", "status": 200, "headers": {}, "body": "# Api\n"},
            {"url": "https://x.test/docs/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/api/intro.md", "status": 200, "headers": {}, "body": "# api intro\n"},
            {"url": "https://x.test/docs/api/intro.md", "status": 200, "headers": {}, "body": "# docs api intro\n"}
        ]"##,
    )
    .unwrap();

    // The only listing of the /api page comes from the dropped /api
    // index, and the /docs root does not cover /api — the replay would
    // silently drop the page, so the trim fails loudly.
    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/docs/llms.txt\nhttps://x.test/docs/intro.md\nhttps://x.test/api/intro.md\n",
    )
    .unwrap();
    let (_, stderr) = trim_output(
        &manifest,
        &keep,
        &dir.path().join("out.json"),
        &dir.path().join("inventory.json"),
    );
    assert!(
        stderr.contains("no reachable index covers https://x.test/api/intro.md"),
        "a page no reachable index covers fails loudly: {stderr}"
    );
}

/// Positive control for the coverage check: a kept deeper index covers
/// the page, so the rescue line lands there — not in the shallower root
/// — in the record the rewrite pass already holds.
#[test]
fn the_trim_tool_rescues_pages_only_under_a_covering_reachable_index() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("manifest.json");
    std::fs::write(
        &manifest,
        r##"[
            {"url": "https://x.test/docs/llms.txt", "status": 200, "headers": {}, "body": "# Docs\n\n- [Api](https://x.test/api/llms.txt)\n- [Sub Api](https://x.test/docs/api/llms.txt)\n- [Intro](https://x.test/docs/intro.md)\n"},
            {"url": "https://x.test/api/llms.txt", "status": 200, "headers": {}, "body": "# Api\n\n- [Page](https://x.test/api/intro.md)\n"},
            {"url": "https://x.test/docs/api/llms.txt", "status": 200, "headers": {}, "body": "# Api\n"},
            {"url": "https://x.test/docs/intro.md", "status": 200, "headers": {}, "body": "# intro\n"},
            {"url": "https://x.test/api/intro.md", "status": 200, "headers": {}, "body": "# api intro\n"},
            {"url": "https://x.test/docs/api/intro.md", "status": 200, "headers": {}, "body": "# docs api intro\n"}
        ]"##,
    )
    .unwrap();

    let keep = dir.path().join("keep.txt");
    std::fs::write(
        &keep,
        "https://x.test/docs/llms.txt\nhttps://x.test/docs/api/llms.txt\n\
         https://x.test/docs/intro.md\nhttps://x.test/docs/api/intro.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        stderr.contains("no reachable index listed"),
        "the uncovered page is rescued loudly: {stderr}"
    );
    let trimmed: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(
        trimmed.len(),
        4,
        "every kept response survives the covered rescue"
    );
    let deeper = trimmed
        .iter()
        .find(|r| r.url == "https://x.test/docs/api/llms.txt")
        .expect("the covering deeper index survives");
    assert!(
        deeper.body.contains("(https://x.test/docs/api/intro.md)"),
        "the rescue line lands in the deepest covering index: {}",
        deeper.body
    );
    let root = &trimmed[0];
    assert_eq!(root.url, "https://x.test/docs/llms.txt");
    assert!(
        !root.body.contains("docs/api/intro.md"),
        "the shallower root never hosts the page: {}",
        root.body
    );
}

/// The subpath index is discovered only through a kept page's
/// `Link: rel="describedby"` header (relative target resolved against the
/// page URL) — the root never links it. With the describer kept, the
/// header queues the index and its page is linked without a rescue.
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
}

/// The describer page is dropped: the describedby-discovered index is
/// unreachable, so its page must be rescued into the reachable root
/// instead of passing silently.
#[test]
fn the_trim_tool_rescues_pages_when_the_describedby_page_is_dropped() {
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
        "https://x.test/llms.txt\nhttps://x.test/intro.md\n\
         https://x.test/guide/llms.txt\nhttps://x.test/guide/page.md\n",
    )
    .unwrap();
    let out = dir.path().join("trimmed.json");
    let (_, stderr) = trim_output(&manifest, &keep, &out, &dir.path().join("inventory.json"));
    assert!(
        stderr.contains("no reachable index listed"),
        "the dropped describer leaves the index unreachable; the page is rescued: {stderr}"
    );
    let dropped: Vec<RecordedResponse> =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert!(
        dropped[0].body.contains("(https://x.test/guide/page.md)"),
        "the rescued page line lands in the reachable root: {}",
        dropped[0].body
    );
}

/// A kept describer whose describedby header names a dropped index must
/// fail loudly: the header is never rewritten, so the replay would follow
/// it and miss the kept page's covering index.
#[test]
fn the_trim_tool_fails_when_a_describedby_page_names_a_dropped_index() {
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
        "https://x.test/llms.txt\nhttps://x.test/intro.md\nhttps://x.test/api.html\n",
    )
    .unwrap();
    let (_, stderr) = trim_output(
        &manifest,
        &keep,
        &dir.path().join("out.json"),
        &dir.path().join("out-inventory.json"),
    );
    assert!(
        stderr.contains("describedby") && stderr.contains("https://x.test/guide/llms.txt"),
        "a kept page naming a dropped index fails loudly: {stderr}"
    );
}

/// A keep list without the root index cannot seed the replay: trim fails
/// loudly instead of writing a corpus nothing can load.
#[test]
fn the_trim_tool_fails_without_the_root_index_to_seed() {
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
    std::fs::write(&keep, "https://x.test/intro.md\n").unwrap();
    let (_, stderr) = trim_output(
        &manifest,
        &keep,
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
