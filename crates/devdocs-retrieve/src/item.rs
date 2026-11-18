//! Result item assembly (docs/implementation/08 §Result envelope).
//!
//! Maps a store [`Hit`] onto the envelope's `ResultItem`, deriving the
//! `docs://` ref from the chunk identity so `search` output feeds `open`
//! input without a second lookup.

use devdocs_model::VersionStatus;
use devdocs_model::docref::ParsedRef;
use devdocs_model::envelope::ResultItem;
use devdocs_model::source::LicenseStatus;
use devdocs_store::Hit;

/// The path segment for a chunk without a section path (content before
/// the first heading). The ref grammar requires a path, so head content
/// serves under this name.
const HEAD_SEGMENT: &str = "index";

/// Whether the license state grants verbatim text: only `approved_snippet`
/// serves the snippet and text fields; every lower state stays queryable
/// with its metadata, but gates the verbatim text off
/// (docs/implementation/09 §License gate enforcement points, point 4). An
/// empty or unknown license string gates off defensively.
#[must_use]
pub fn allows_verbatim(license_status: &str) -> bool {
    LicenseStatus::parse(license_status) == Some(LicenseStatus::ApprovedSnippet)
}

/// Maps a store hit onto the envelope result item.
///
/// `confidence` is the fusion score; `status` comes from the version
/// policy. The ref derives from the library, version, slugified section
/// path, and the slugified symbol as fragment (absent without a symbol).
/// The snippet serves only under [`allows_verbatim`]: a lower approved
/// state keeps the metadata but carries no verbatim text. The served
/// snippet runs through [`devdocs_model::redaction::redact`] — the
/// output-side second gate
/// (docs/implementation/09 §Secrets and redaction).
#[must_use]
pub fn to_result_item(
    hit: &Hit,
    requested: Option<&str>,
    status: VersionStatus,
    confidence: f64,
) -> ResultItem {
    let snippet = if allows_verbatim(&hit.license_status) {
        devdocs_model::redaction::redact(&hit.snippet.clone().unwrap_or_else(|| hit.text.clone()))
    } else {
        String::new()
    };
    ResultItem {
        doc_ref: derive_ref(hit),
        title: hit.section_path.last().unwrap_or(&hit.library).clone(),
        section: hit.section_path.join(" > "),
        snippet,
        examples: Vec::new(),
        source_url: hit.result_source_url().to_owned(),
        source_commit: hit.source_commit.clone().unwrap_or_default(),
        requested_version: requested.map(str::to_owned),
        served_version: hit.version.clone(),
        version_status: status,
        content_hash: hex(&hit.content_hash),
        license_status: hit.license_status.clone(),
        // Attribution joins at the command layer from the declarative
        // config (docs/implementation/07 §License gate).
        attribution: None,
        confidence,
        token_count: u64::from(hit.token_count),
        code_caution: hit.code_caution,
    }
}

/// Derives the `docs://` ref for one chunk: the library, the `v`-prefixed
/// version, the page path (the chunk page URL path relative to the source
/// root, when the chunk carries a usable page URL —
/// `docs://<library>/v<version>/<page>@<path>`), the section path (the
/// placeholder segment when a path segment or the whole path slugs to
/// empty), and the slugified symbol as the fragment when one applies. An
/// empty fragment is left out — a symbol that slugs to empty names
/// nothing, and a chunk without a usable page URL derives the
/// page-less form.
#[must_use]
pub fn derive_ref(hit: &Hit) -> String {
    let segments: Vec<String> = if hit.section_path.is_empty() {
        vec![HEAD_SEGMENT.to_owned()]
    } else {
        hit.section_path.iter().map(|s| segment(s)).collect()
    };
    let mut ref_ = format!("docs://{}/v{}/", hit.library, hit.version);
    if let Some(page) = hit.page_path() {
        ref_.push_str(&page);
        ref_.push('@');
    }
    ref_.push_str(&segments.join("/"));
    if let Some(symbol) = &hit.symbol {
        let slug = slug(symbol);
        if !slug.is_empty() {
            ref_.push('#');
            ref_.push_str(&slug);
        }
    }
    ref_
}

/// Slugifies one ref path or fragment segment: lowercase, runs of
/// non-alphanumeric characters fold to one `-`, ends trimmed.
#[must_use]
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    out
}

/// The ref segment for one section heading: the slug, or the placeholder
/// when the heading has no ASCII alphanumerics. A non-ASCII or
/// punctuation-only heading slugs to empty, and the ref grammar rejects
/// empty segments.
#[must_use]
fn segment(text: &str) -> String {
    let s = slug(text);
    if s.is_empty() {
        HEAD_SEGMENT.to_owned()
    } else {
        s
    }
}

/// Lowercase hex of a sha256 digest.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [HEX[usize::from(b >> 4)], HEX[usize::from(b & 0x0f)]])
        .collect::<Vec<u8>>()
        .into_iter()
        .map(char::from)
        .collect()
}

/// Whether the hit serves the ref's pinned page: a ref without a page
/// part pins nothing (every chunk qualifies); a page-pinned ref matches
/// only a chunk whose page URL path equals the pinned page, so a section
/// title another page shares cannot win (issue #88).
#[must_use]
fn page_matches(hit: &Hit, pinned: Option<&str>) -> bool {
    match pinned {
        None => true,
        Some(page) => hit.page_path().is_some_and(|hit_page| hit_page == page),
    }
}

/// Finds the chunk that serves a parsed ref: the full section path
/// first, then the fragment (symbol or title match), then the last
/// segment. The placeholder segment matches a chunk without a section
/// path and a chunk whose heading slugs to empty. When several
/// candidates remain, the lowest chunk id serves. `None` when nothing
/// matches.
///
/// A page-pinned ref narrows the full-path match to chunks on the pinned
/// page — the page part only ever narrows, so a page-qualified ref that
/// no chunk on the pinned page satisfies falls through the fragment and
/// last-segment ladder unchanged (docs/implementation/08 §Ref derivation
/// owns the resolution rule).
#[must_use]
pub fn match_ref<'a>(chunks: &'a [Hit], parsed: &ParsedRef) -> Option<&'a Hit> {
    let want: Vec<String> = parsed.segments.iter().map(|s| segment(s)).collect();
    let pinned_page = parsed.page.as_ref().map(|p| p.join("/"));
    let pinned: Option<&str> = pinned_page.as_deref();
    // 1. The full section path matches the ref path exactly, restricted
    //    to the ref's pinned page when it names one. The placeholder path
    //    also matches a chunk without a section path and a chunk whose
    //    heading slugs to empty.
    let full: Vec<&Hit> = chunks
        .iter()
        .filter(|c| {
            page_matches(c, pinned)
                && ((want.len() == 1 && want[0] == HEAD_SEGMENT && c.section_path.is_empty())
                    || (c.section_path.len() == want.len()
                        && c.section_path
                            .iter()
                            .map(|s| segment(s))
                            .eq(want.iter().cloned())))
        })
        .collect();
    if !full.is_empty() && (pinned.is_some() || full.len() == 1) {
        return full.iter().min_by_key(|hit| hit.chunk_id).copied();
    }
    // 2. The fragment names the chunk's symbol when the path is absent
    //    from the section paths.
    if let Some(fragment) = &parsed.fragment {
        let by_symbol: Vec<&Hit> = chunks
            .iter()
            .filter(|c| c.symbol.as_deref().is_some_and(|s| slug(s) == *fragment))
            .collect();
        if by_symbol.len() == 1 {
            return by_symbol.first().copied();
        }
        // 3. The fragment matches the title (the last section segment).
        let by_title: Vec<&Hit> = chunks
            .iter()
            .filter(|c| {
                c.section_path
                    .last()
                    .is_some_and(|t| segment(t) == *fragment)
            })
            .collect();
        if by_title.len() == 1 {
            return by_title.first().copied();
        }
    }
    // 4. The last segment matches the chunk's last section segment; the
    //    placeholder names a chunk without a section path and a chunk
    //    whose heading slugs to empty. When several candidates remain —
    //    two pages can share a section title — the lowest chunk id
    //    serves, so one ref always serves one chunk.
    let last = want.last()?;
    let by_last: Vec<&Hit> = chunks
        .iter()
        .filter(|c| {
            (last == HEAD_SEGMENT && c.section_path.is_empty())
                || c.section_path.last().is_some_and(|t| segment(t) == *last)
        })
        .collect();
    by_last.iter().min_by_key(|hit| hit.chunk_id).copied()
}

#[cfg(test)]
mod tests {
    // Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{derive_ref, hex, match_ref, slug, to_result_item};
    use devdocs_model::VersionStatus;
    use devdocs_model::docref::ParsedRef;
    use devdocs_store::Hit;

    fn hit(library: &str, version: &str, section: &[&str], symbol: Option<&str>) -> Hit {
        let mut hit = devdocs_testkit::sample_hit(1, 42);
        hit.library = library.to_owned();
        hit.version = version.to_owned();
        hit.section_path = section.iter().map(|s| (*s).to_owned()).collect();
        hit.symbol = symbol.map(str::to_owned);
        hit.snippet = Some("the <mark>routing</mark> layer".to_owned());
        hit.text = "text".to_owned();
        hit.content_hash = [0xab; 32];
        hit.source_url = "https://example.test/next".to_owned();
        hit.source_commit = Some("abc123".to_owned());
        hit
    }

    /// The item carries every envelope field, with the hash hex-encoded
    /// and the snippet passed through at the snippet stage.
    #[test]
    fn items_carry_the_envelope_fields() {
        let mut hit = hit(
            "next.js",
            "16.2.9",
            &["Routing", "use_router"],
            Some("use_router"),
        );
        hit.license_status = "approved_snippet".to_owned();
        let item = to_result_item(&hit, Some("16.2.9"), VersionStatus::Verified, 0.94);
        assert_eq!(
            item.doc_ref,
            "docs://next.js/v16.2.9/routing/use-router#use-router"
        );
        assert_eq!(item.title, "use_router");
        assert_eq!(item.section, "Routing > use_router");
        assert_eq!(item.snippet, "the <mark>routing</mark> layer");
        assert_eq!(item.requested_version.as_deref(), Some("16.2.9"));
        assert_eq!(item.served_version, "16.2.9");
        assert_eq!(item.version_status, VersionStatus::Verified);
        assert_eq!(item.content_hash, "ab".repeat(32));
        assert_eq!(item.license_status, "approved_snippet");
        assert_eq!((item.confidence, item.token_count), (0.94, 42));
    }

    /// The render gate (docs/09 enforcement point 4): verbatim text serves
    /// only under `approved_snippet`; every lower state keeps the metadata
    /// but the snippet comes out empty, and an unknown or empty license
    /// string gates off defensively.
    #[test]
    fn verbatim_text_gates_at_the_snippet_stage() {
        let mut hit = hit("next.js", "16.2.9", &["Routing"], Some("use_router"));
        hit.license_status = "approved_snippet".to_owned();
        let item = to_result_item(&hit, None, VersionStatus::Verified, 1.0);
        assert_eq!(item.snippet, "the <mark>routing</mark> layer");
        assert_eq!(item.title, "Routing");
        assert_eq!(item.source_url, "https://example.test/next");

        hit.license_status = "approved_index".to_owned();
        let item = to_result_item(&hit, None, VersionStatus::Verified, 1.0);
        assert_eq!(item.snippet, "");
        assert_eq!(item.title, "Routing");
        assert_eq!(item.section, "Routing");
        assert_eq!(item.source_url, "https://example.test/next");
        assert_eq!(item.token_count, 42);

        hit.license_status = "approved_fetch".to_owned();
        assert_eq!(
            to_result_item(&hit, None, VersionStatus::Verified, 1.0).snippet,
            ""
        );

        hit.license_status = "denied".to_owned();
        assert_eq!(
            to_result_item(&hit, None, VersionStatus::Verified, 1.0).snippet,
            ""
        );

        hit.license_status = String::new();
        assert_eq!(
            to_result_item(&hit, None, VersionStatus::Verified, 1.0).snippet,
            ""
        );

        hit.license_status = "garbage".to_owned();
        assert_eq!(
            to_result_item(&hit, None, VersionStatus::Verified, 1.0).snippet,
            ""
        );
    }

    /// The item cites the chunk's page URL when the chunk carries one, and
    /// falls back to the source origin URL otherwise
    /// (docs/implementation/08 §Result envelope).
    #[test]
    fn source_url_prefers_the_page_url() {
        let mut with_page = hit("next.js", "16.2.9", &["Routing"], None);
        with_page.page_url = Some("https://example.test/next/routing.html".to_owned());
        let item = to_result_item(&with_page, None, VersionStatus::Verified, 1.0);
        assert_eq!(item.source_url, "https://example.test/next/routing.html");

        let without_page = hit("next.js", "16.2.9", &["Routing"], None);
        let item = to_result_item(&without_page, None, VersionStatus::Verified, 1.0);
        assert_eq!(item.source_url, "https://example.test/next");
    }

    /// Ref derivation slugifies the path and the symbol, qualifies the
    /// page when the chunk carries a usable page URL, and stays
    /// page-less otherwise. Every derived ref parses back.
    #[test]
    fn refs_derive_from_the_chunk_identity() {
        let with_symbol = hit("next.js", "16.2.9", &["Routing"], Some("use_router"));
        assert_eq!(
            derive_ref(&with_symbol),
            "docs://next.js/v16.2.9/routing#use-router"
        );

        let no_symbol = hit("zod", "4.6.7", &["Parse"], None);
        assert_eq!(derive_ref(&no_symbol), "docs://zod/v4.6.7/parse");

        let mut on_page = hit("next.js", "16.2.9", &["Routing"], Some("use_router"));
        on_page.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        assert_eq!(
            derive_ref(&on_page),
            "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md\
             @routing#use-router"
        );
        ParsedRef::parse(&derive_ref(&on_page))
            .unwrap_or_else(|| panic!("the derived ref parses back: {}", derive_ref(&on_page)));

        // A page URL that cannot be a page part leaves the ref page-less.
        let mut unusable = hit("zod", "4.6.7", &["Parse"], None);
        unusable.page_url = Some("https://zod.dev".to_owned());
        assert_eq!(derive_ref(&unusable), "docs://zod/v4.6.7/parse");
    }

    /// A page-pinned ref serves the pinned page's chunk, not a same-title
    /// section on another page — the #88 hole: the proxy page's Matcher
    /// ref no longer resolves into the llms-full export copy that carries
    /// the same breadcrumb, and the page-less ref keeps the old
    /// lowest-chunk-id behavior.
    #[test]
    fn a_page_qualified_ref_stays_on_the_pinned_page() {
        let mut export = hit("next.js", "16.2.9", &["Matcher"], Some("config"));
        export.chunk_id = 1;
        export.page_url = Some("https://nextjs.org/docs/llms-full.txt".to_owned());
        let mut proxy = hit("next.js", "16.2.9", &["Matcher"], Some("config"));
        proxy.chunk_id = 2;
        proxy.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        let chunks = vec![export, proxy];

        let page_qualified = ParsedRef::parse(
            "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md\
             @matcher#config",
        )
        .unwrap();
        assert_eq!(
            match_ref(&chunks, &page_qualified).map(|h| h.chunk_id),
            Some(2),
            "the proxy page's chunk serves, not the export copy"
        );

        // Without the page part, the old ladder applies: the full-path
        // match collides, and the lowest chunk id (the export copy) wins.
        let bare = ParsedRef::parse("docs://next.js/v16.2.9/matcher#config").unwrap();
        assert_eq!(match_ref(&chunks, &bare).map(|h| h.chunk_id), Some(1));
    }

    /// A page-pinned ref whose pinned page serves no full-path match
    /// falls through the old fragment and last-segment ladder — the page
    /// part narrows, it never widens.
    #[test]
    fn a_page_pinned_ref_without_a_page_match_falls_through() {
        let mut on_page = hit("next.js", "16.2.9", &["Routing"], None);
        on_page.chunk_id = 1;
        on_page.page_url = Some("https://nextjs.org/docs/app/proxy.md".to_owned());
        let mut elsewhere = hit("next.js", "16.2.9", &["Storage"], None);
        elsewhere.chunk_id = 2;
        elsewhere.page_url = Some("https://nextjs.org/docs/storage.md".to_owned());
        let chunks = vec![on_page, elsewhere];

        // The pinned page names only the Routing chunk, so the Storage
        // section on it matches nothing and the last-segment ladder serves
        // the lowest chunk id of the Storage title matches.
        let drifted = ParsedRef::parse("docs://next.js/v16.2.9/docs/app/proxy.md@storage").unwrap();
        assert_eq!(match_ref(&chunks, &drifted).map(|h| h.chunk_id), Some(2));
    }

    /// A page-pinned ref whose pinned page holds several full-path
    /// matches stays on the pinned page — the lowest chunk id among the
    /// pinned-page candidates serves, not a lower chunk id on another
    /// page that the unfiltered fallback scans would pick up.
    #[test]
    fn a_page_pinned_ref_with_several_page_matches_stays_on_the_pinned_page() {
        let mut export = hit("next.js", "16.2.9", &["Matcher"], None);
        export.chunk_id = 1;
        export.page_url = Some("https://nextjs.org/docs/llms-full.txt".to_owned());
        let mut proxy_earlier = hit("next.js", "16.2.9", &["Matcher"], None);
        proxy_earlier.chunk_id = 2;
        proxy_earlier.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        let mut proxy_later = hit("next.js", "16.2.9", &["Matcher"], None);
        proxy_later.chunk_id = 3;
        proxy_later.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        let chunks = vec![export, proxy_earlier, proxy_later];

        let pinned = ParsedRef::parse(
            "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md\
             @matcher",
        )
        .unwrap();
        assert_eq!(
            match_ref(&chunks, &pinned).map(|h| h.chunk_id),
            Some(2),
            "the pinned page's lowest chunk id serves, not the export copy"
        );
    }

    /// The placeholder variant: two headless chunks on the pinned page
    /// collide on `index`, and the ref stays on the pinned page instead
    /// of serving a lower chunk id on another page.
    #[test]
    fn a_page_pinned_placeholder_ref_stays_on_the_pinned_page() {
        let mut export = hit("next.js", "16.2.9", &[], None);
        export.chunk_id = 1;
        export.page_url = Some("https://nextjs.org/docs/llms-full.txt".to_owned());
        let mut proxy_earlier = hit("next.js", "16.2.9", &[], None);
        proxy_earlier.chunk_id = 2;
        proxy_earlier.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        let mut proxy_later = hit("next.js", "16.2.9", &[], None);
        proxy_later.chunk_id = 3;
        proxy_later.page_url =
            Some("https://nextjs.org/docs/app/api-reference/file-conventions/proxy.md".to_owned());
        let chunks = vec![export, proxy_earlier, proxy_later];

        let pinned = ParsedRef::parse(
            "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md\
             @index",
        )
        .unwrap();
        assert_eq!(
            match_ref(&chunks, &pinned).map(|h| h.chunk_id),
            Some(2),
            "the pinned page's lowest chunk id serves, not the export copy"
        );
    }

    /// The slug rule folds punctuation runs and trims the ends.
    #[test]
    fn slugs_fold_punctuation() {
        assert_eq!(slug("Proxy Rewrite"), "proxy-rewrite");
        assert_eq!(slug("--use_cache (v2)--"), "use-cache-v2");
        assert_eq!(slug("###"), "");
        assert_eq!(slug("Rust 1.90"), "rust-1-90");
    }

    /// Ref matching finds the chunk by full path, then fragment, then
    /// last segment.
    #[test]
    fn refs_match_by_path_then_fragment() {
        let mut first = hit(
            "next.js",
            "16.2.9",
            &["Routing", "use_router"],
            Some("use_router"),
        );
        first.chunk_id = 1;
        let mut second = hit("next.js", "16.2.9", &["Storage"], None);
        second.chunk_id = 2;
        let chunks = vec![first, second];
        let parsed = devdocs_model::docref::ParsedRef::parse(
            "docs://next.js/v16.2.9/routing/use-router#use-router",
        )
        .unwrap();
        assert_eq!(match_ref(&chunks, &parsed).map(|h| h.chunk_id), Some(1));

        let by_last =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/storage").unwrap();
        assert_eq!(match_ref(&chunks, &by_last).map(|h| h.chunk_id), Some(2));

        let nothing =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/missing").unwrap();
        assert_eq!(match_ref(&chunks, &nothing), None);
    }

    /// Fragment matching falls back to the symbol, then the title, and the
    /// last ref segment matches the last section segment.
    #[test]
    fn refs_match_by_fragment_then_last_segment() {
        let mut with_symbol = hit("next.js", "16.2.9", &["Routing"], Some("use_router"));
        with_symbol.chunk_id = 1;
        let mut titled = hit("next.js", "16.2.9", &["Storage"], None);
        titled.chunk_id = 2;
        let chunks = vec![with_symbol, titled];

        // Step 2: the fragment names the symbol; the path matches no
        // section path.
        let by_symbol =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/missing#use-router")
                .unwrap();
        assert_eq!(match_ref(&chunks, &by_symbol).map(|h| h.chunk_id), Some(1));

        // Step 3: the fragment matches the last section segment (the
        // title) of a chunk without a symbol.
        let by_title =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/other#storage")
                .unwrap();
        assert_eq!(match_ref(&chunks, &by_title).map(|h| h.chunk_id), Some(2));

        // Step 4: only the last segment of a multi-segment path matches.
        let by_last =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/other/storage")
                .unwrap();
        assert_eq!(match_ref(&chunks, &by_last).map(|h| h.chunk_id), Some(2));
    }

    /// A chunk without a section path derives the placeholder ref, and
    /// that ref matches the chunk back.
    #[test]
    fn headerless_chunks_derive_and_match_the_placeholder_ref() {
        let headless = hit("next.js", "16.2.9", &[], None);
        assert_eq!(derive_ref(&headless), "docs://next.js/v16.2.9/index");

        let parsed =
            devdocs_model::docref::ParsedRef::parse("docs://next.js/v16.2.9/index").unwrap();
        assert_eq!(match_ref(&[headless], &parsed).map(|h| h.chunk_id), Some(1));
    }

    /// Two chunks that share a section path (or two headless chunks that
    /// share the placeholder path) collide on one ref; the lowest chunk
    /// id serves, so one ref always serves one chunk.
    #[test]
    fn colliding_refs_serve_the_lowest_chunk_id() {
        let mut titled_later = hit("acme", "1.0.0", &["Overview"], None);
        titled_later.chunk_id = 7;
        let mut titled_earlier = hit("acme", "1.0.0", &["Overview"], None);
        titled_earlier.chunk_id = 3;
        let parsed =
            devdocs_model::docref::ParsedRef::parse("docs://acme/v1.0.0/overview").unwrap();
        assert_eq!(
            match_ref(&[titled_later, titled_earlier], &parsed).map(|h| h.chunk_id),
            Some(3)
        );

        let mut headless_later = hit("acme", "1.0.0", &[], None);
        headless_later.chunk_id = 9;
        let mut headless_earlier = hit("acme", "1.0.0", &[], None);
        headless_earlier.chunk_id = 4;
        let placeholder =
            devdocs_model::docref::ParsedRef::parse("docs://acme/v1.0.0/index").unwrap();
        assert_eq!(
            match_ref(&[headless_later, headless_earlier], &placeholder).map(|h| h.chunk_id),
            Some(4)
        );
    }

    /// A heading with no ASCII alphanumerics slugs to empty; the ref
    /// carries the placeholder segment and still serves the chunk, and a
    /// symbol that slugs to empty leaves no empty fragment.
    #[test]
    fn empty_slug_headings_derive_and_match_the_placeholder_ref() {
        let heading = hit("acme", "1.0.0", &["日本語"], None);
        assert_eq!(derive_ref(&heading), "docs://acme/v1.0.0/index");
        let parsed = devdocs_model::docref::ParsedRef::parse("docs://acme/v1.0.0/index").unwrap();
        assert_eq!(match_ref(&[heading], &parsed).map(|h| h.chunk_id), Some(1));

        let nested = hit("acme", "1.0.0", &["Intro", "###"], None);
        assert_eq!(derive_ref(&nested), "docs://acme/v1.0.0/intro/index");

        let symbol = hit("acme", "1.0.0", &["Guide"], Some("###"));
        assert_eq!(derive_ref(&symbol), "docs://acme/v1.0.0/guide");
    }

    /// A literal "Index" heading, a heading that slugs to empty, and a
    /// headless chunk all collide on the placeholder ref; the lowest
    /// chunk id serves.
    #[test]
    fn placeholder_collisions_serve_the_lowest_chunk_id() {
        let mut named = hit("acme", "1.0.0", &["Index"], None);
        named.chunk_id = 8;
        let mut empties = hit("acme", "1.0.0", &["###"], None);
        empties.chunk_id = 6;
        let mut headless = hit("acme", "1.0.0", &[], None);
        headless.chunk_id = 9;
        let parsed = devdocs_model::docref::ParsedRef::parse("docs://acme/v1.0.0/index").unwrap();
        assert_eq!(
            match_ref(&[named, empties, headless], &parsed).map(|h| h.chunk_id),
            Some(6)
        );
    }

    /// Hex encoding is lowercase and two digits per byte.
    #[test]
    fn hex_encodes_lowercase() {
        assert_eq!(hex(&[0x0f, 0xab]), "0fab");
    }
}
