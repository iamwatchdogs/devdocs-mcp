// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::metrics::{RetrievedRef, hit_at_5, ref_match, ref_matches};

use super::{hit, on_page, ref_list};

#[test]
fn references_render_with_and_without_a_symbol() {
    let with = hit(1, "next.js", "16.2.9", "app/proxy", Some("rewrite"));
    assert_eq!(with.reference(), "docs://next.js/v16.2.9/app/proxy#rewrite");
    let without = hit(1, "next.js", "16.2.9", "app/proxy", None);
    assert_eq!(without.reference(), "docs://next.js/v16.2.9/app/proxy");
    let paged = on_page(
        hit(1, "next.js", "16.2.9", "app/proxy", Some("rewrite")),
        "docs/app/api-reference/proxy.md",
    );
    assert_eq!(
        paged.reference(),
        "docs://next.js/v16.2.9/docs/app/api-reference/proxy.md@app/proxy#rewrite"
    );
}

/// A pinned page must equal the hit's page exactly: the wrong-page
/// hole (issue #88) stays closed — a Matcher section served from the
/// llms-full export copy does not grade a proxy-page pin.
#[test]
fn a_pinned_page_must_match_exactly() {
    let export = on_page(
        hit(1, "next.js", "16.2.9", "Matcher", Some("config")),
        "docs/llms-full.txt",
    );
    let proxy = on_page(
        hit(1, "next.js", "16.2.9", "Matcher", Some("config")),
        "docs/app/api-reference/file-conventions/proxy.md",
    );
    let pin = "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md@Matcher";

    assert!(ref_matches(pin, &proxy));
    assert!(
        !ref_matches(pin, &export),
        "a same-title section on another page is not a hit"
    );
    // The page-less pin keeps the old semantics for both hits.
    assert!(ref_matches(
        "docs://next.js/v16.2.9/Matcher#config",
        &export
    ));
    assert!(ref_matches("docs://next.js/v16.2.9/Matcher#config", &proxy));
}

#[test]
fn broken_refs_and_longer_expectations_never_match() {
    let h = hit(1, "next.js", "16.2.9", "proxy", None);
    // A ref that does not parse matches nothing.
    assert!(!ref_matches("not-a-ref", &h));
    // The expected path is longer than the hit path: no trailing run
    // of the hit segments can equal it.
    assert!(!ref_matches("docs://next.js/v16.2.9/app/proxy/deep", &h));
}

#[test]
fn hit_at_5_counts_only_the_top_five() {
    let hits: Vec<RetrievedRef> = (1..=8)
        .map(|i| hit(i, "next.js", "16.2.9", &format!("page{i}"), None))
        .collect();
    let at_three = ref_list(&["docs://next.js/v16.2.9/page3"]);
    let at_six = ref_list(&["docs://next.js/v16.2.9/page6"]);
    let absent = ref_list(&["docs://next.js/v16.2.9/other"]);

    assert!(hit_at_5(&at_three, &hits));
    assert!(!hit_at_5(&at_six, &hits), "rank 6 is outside the top five");
    assert!(!hit_at_5(&absent, &hits));
    assert!(ref_match(&at_six, &hits), "the full list reaches rank 6");
}

#[test]
fn expected_paths_match_trailing_hit_segments() {
    let h = hit(1, "next.js", "16.2.9", "app/proxy", Some("rewrite"));
    assert!(ref_matches("docs://next.js/v16.2.9/proxy#rewrite", &h));
    assert!(ref_matches("docs://next.js/v16.2.9/app/proxy#rewrite", &h));
    assert!(!ref_matches("docs://next.js/v16.2.9/proxy#other", &h));
    assert!(
        ref_matches("docs://next.js/v16.2.9/app/proxy", &h),
        "a ref without a fragment ignores the hit symbol"
    );
    assert!(ref_matches(
        "docs://next.js/v16.2.9/app/proxy",
        &hit(1, "next.js", "16.2.9", "app/proxy", None)
    ));
}

#[test]
fn library_is_case_insensitive_and_version_exact() {
    let h = hit(1, "Next.js", "16.2.9", "proxy", None);
    assert!(ref_matches("docs://next.js/v16.2.9/proxy", &h));
    assert!(!ref_matches("docs://next.js/v16.3.0/proxy", &h));
}
