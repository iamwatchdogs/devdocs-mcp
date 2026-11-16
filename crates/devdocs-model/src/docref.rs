//! The `docs://` ref grammar (docs/implementation/14 §Golden-set format,
//! docs/implementation/08 §Ref derivation): parsing shared by the eval
//! harness and the retrieval open path.

/// A parsed `docs://` ref:
/// `docs://<library>/v<version>/<page>@<path>[#<fragment>]`, the page
/// part optional — `docs://<library>/v<version>/<path>[#<fragment>]`
/// parses without one. The page qualifies which served page the ref
/// names; the path is the section breadcrumb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRef {
    /// The library authority.
    pub library: String,
    /// The pinned version in the ref, without the leading `v`.
    pub version: String,
    /// The page path segments between the version and the `@` delimiter,
    /// when the ref qualifies a page. The page is the chunk's page URL
    /// path relative to the source root ([`page_path`]).
    pub page: Option<Vec<String>>,
    /// The non-empty breadcrumb segments after the `@` (or the whole path
    /// on a page-less ref), matched against chunk section paths at open
    /// time.
    pub segments: Vec<String>,
    /// The fragment after `#`, when present.
    pub fragment: Option<String>,
}

impl ParsedRef {
    /// Parses the ref grammar; `None` for any broken shape.
    ///
    /// The path is one or more `/`-separated segments without `..`, `?`,
    /// or whitespace; a fragment must be non-empty without whitespace.
    /// When the path after the version carries an `@`, everything before
    /// the first `@` is the page path under the same segment rules (plus
    /// no `@` inside it — the first `@` is the delimiter), and everything
    /// after it is the breadcrumb; both sides must be non-empty.
    #[must_use]
    pub fn parse(r: &str) -> Option<ParsedRef> {
        let rest = r.strip_prefix("docs://")?;
        let (library, tail) = rest.split_once('/')?;
        if library.is_empty() {
            return None;
        }
        let (path, fragment) = match tail.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (tail, None),
        };
        let (version_segment, segment_text) = path.split_once('/')?;
        // "v", not 'v': Lizard's Rust lexer trips on 'v' (docs/implementation/03 §Git hooks).
        let version = version_segment.strip_prefix("v")?;
        if version.is_empty() {
            return None;
        }
        let (page, segment_text) = match segment_text.split_once('@') {
            Some((page, crumb)) => {
                let page = split_path(page)?;
                (Some(page), crumb)
            }
            None => (None, segment_text),
        };
        let segments = split_path(segment_text)?;
        if fragment.is_some_and(|f| f.is_empty() || f.contains(char::is_whitespace)) {
            return None;
        }
        Some(ParsedRef {
            library: library.to_owned(),
            version: version.to_owned(),
            page,
            segments,
            fragment: fragment.map(str::to_owned),
        })
    }
}

/// The canonical spelling of a parsed ref: the inverse of [`ParsedRef::parse`]
/// for any ref whose parts are grammar-valid.
impl std::fmt::Display for ParsedRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "docs://{}/v{}/", self.library, self.version)?;
        if let Some(page) = &self.page {
            write!(f, "{}@", page.join("/"))?;
        }
        write!(f, "{}", self.segments.join("/"))?;
        if let Some(fragment) = &self.fragment {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

/// Splits one `/`-joined path into its segments under the grammar rules:
/// at least one non-empty segment, no `..`, no `?`, no whitespace, no `@`.
#[must_use]
fn split_path(text: &str) -> Option<Vec<String>> {
    let segments: Vec<String> = text.split('/').map(str::to_owned).collect();
    if segments.is_empty()
        || segments.iter().any(|s| {
            s.is_empty()
                || s == ".."
                || s.contains('?')
                || s.contains('@')
                || s.contains(char::is_whitespace)
        })
    {
        return None;
    }
    Some(segments)
}

/// The page URL path relative to the source root: the path component of a
/// page URL after the authority, the leading and trailing `/` trimmed and
/// the query and fragment dropped. `None` when the URL carries no path or
/// the path cannot be the page part of the ref grammar (an `@` inside it
/// would end the page early; whitespace, `..`, or `?` would not parse
/// back).
#[must_use]
pub fn page_path(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    let path = after_scheme[after_scheme.find('/')?..]
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches('/');
    if path.is_empty() {
        return None;
    }
    split_path(path).map(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
    // Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{ParsedRef, page_path};

    /// Well-formed refs parse into their parts.
    #[test]
    fn well_formed_refs_parse() {
        let parsed = ParsedRef::parse("docs://next.js/v16.2.9/app/proxy#rewrite").unwrap();
        assert_eq!(parsed.library, "next.js");
        assert_eq!(parsed.version, "16.2.9");
        assert_eq!(parsed.page, None);
        assert_eq!(parsed.segments, vec!["app", "proxy"]);
        assert_eq!(parsed.fragment.as_deref(), Some("rewrite"));

        let bare = ParsedRef::parse("docs://zod/v4.6.7/parse").unwrap();
        assert_eq!(bare.segments, vec!["parse"]);
        assert_eq!(bare.page, None);
        assert_eq!(bare.fragment, None);
    }

    /// A page-qualified ref splits the page path off at the `@`.
    #[test]
    fn page_qualified_refs_parse() {
        let parsed = ParsedRef::parse(
            "docs://next.js/v16.2.9/docs/app/api-reference/file-conventions/proxy.md\
             @app/proxy#rewrite",
        )
        .unwrap();
        assert_eq!(parsed.library, "next.js");
        assert_eq!(parsed.version, "16.2.9");
        assert_eq!(
            parsed.page,
            Some(vec![
                "docs".to_owned(),
                "app".to_owned(),
                "api-reference".to_owned(),
                "file-conventions".to_owned(),
                "proxy.md".to_owned()
            ])
        );
        assert_eq!(parsed.segments, vec!["app", "proxy"]);
        assert_eq!(parsed.fragment.as_deref(), Some("rewrite"));

        let no_fragment =
            ParsedRef::parse("docs://zod/v4.6.7/docs/llms-full.txt@Defining schemas").is_none();
        assert!(no_fragment, "whitespace still breaks the breadcrumb");
    }

    /// Broken shapes do not parse, including broken page parts.
    #[test]
    fn broken_shapes_do_not_parse() {
        let cases = [
            "https://next.js/v16.2.9/proxy",
            "next.js/v16.2.9/proxy",
            "docs://next.js/proxy",
            "docs://next.js/v16.2.9",
            "docs://next.js/v16.2.9/",
            "docs://next.js/16.2.9/proxy",
            "docs:///v1.0.0/path",
            "docs://next.js/v/path",
            "docs://next.js/v16.2.9/../etc",
            "docs://next.js/v16.2.9/a//b",
            "docs://next.js/v16.2.9/a b",
            "docs://next.js/v16.2.9/a?b",
            "docs://next.js/v16.2.9/proxy#",
            "docs://next.js/v16.2.9/proxy#a b",
            // Page parts: empty page, empty breadcrumb, broken page
            // segments.
            "docs://next.js/v16.2.9/@proxy",
            "docs://next.js/v16.2.9/docs/proxy.md@",
            "docs://next.js/v16.2.9/docs//proxy.md@proxy",
            "docs://next.js/v16.2.9/../proxy.md@proxy",
            "docs://next.js/v16.2.9/docs proxy.md@proxy",
            "docs://next.js/v16.2.9/docs/proxy?x.md@proxy",
            "docs://next.js/v16.2.9/docs/proxy.md@../proxy",
        ];
        for r in cases {
            assert!(ParsedRef::parse(r).is_none(), "{r} must not parse");
        }
    }

    /// The canonical spelling reassembles the parsed parts, and every
    /// grammar-valid ref round-trips through it.
    #[test]
    fn display_serializes_the_parsed_parts() {
        let page_qualified =
            ParsedRef::parse("docs://next.js/v16.2.9/docs/app/proxy.md@app/proxy#rewrite").unwrap();
        assert_eq!(
            page_qualified.to_string(),
            "docs://next.js/v16.2.9/docs/app/proxy.md@app/proxy#rewrite"
        );

        let bare = ParsedRef::parse("docs://zod/v4.6.7/parse").unwrap();
        assert_eq!(bare.to_string(), "docs://zod/v4.6.7/parse");

        let fragment_only = ParsedRef::parse("docs://next.js/v16.2.9/routing#use-router").unwrap();
        assert_eq!(
            fragment_only.to_string(),
            "docs://next.js/v16.2.9/routing#use-router"
        );
    }

    /// Page-qualified refs round-trip: parse(serialize(p)) == p.
    #[test]
    fn page_qualified_refs_round_trip() {
        for r in [
            "docs://next.js/v16.2.9/docs/app/proxy.md@app/proxy#rewrite",
            "docs://next.js/v16.2.9/docs/llms-full.txt@Exports/Matcher#config",
            "docs://zod/v4.6.7/docs/defining-schemas.md@Defining/Codecs",
            "docs://next.js/v16.2.9/docs/app/proxy.md@index",
        ] {
            let parsed = ParsedRef::parse(r).unwrap();
            assert_eq!(parsed.to_string(), r, "{r} round-trips");
        }
    }

    /// The page path of a page URL is the path after the authority, the
    /// ends trimmed, query and fragment dropped; unusable paths yield
    /// nothing.
    #[test]
    fn page_paths_strip_the_authority() {
        assert_eq!(
            page_path("https://nextjs.org/docs/app/api-reference/proxy.md"),
            Some("docs/app/api-reference/proxy.md".to_owned())
        );
        assert_eq!(
            page_path("https://nextjs.org/docs/llms-full.txt?x=1#frag"),
            Some("docs/llms-full.txt".to_owned())
        );
        assert_eq!(
            page_path("https://nextjs.org/"),
            None,
            "a bare authority has no page path"
        );
        assert_eq!(page_path("not a url"), None);
        assert_eq!(page_path("https://nextjs.org"), None);
        assert_eq!(
            page_path("https://nextjs.org/docs/a@b.md"),
            None,
            "an `@` would end the page part early"
        );
        assert_eq!(
            page_path("https://nextjs.org/docs/a b.md"),
            None,
            "a path that cannot parse back is no page part"
        );
    }
}
