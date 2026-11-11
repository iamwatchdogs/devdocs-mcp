//! The 50-query contract corpus for the identical-results gate
//! (`mcp_contract.rs`): every entry names one query, its parameters, and
//! the outcome both edges must agree on. The entries derive from the
//! fixture bodies `common::contract_env` ingests, so every hit hits and
//! every miss misses deterministically.

/// The tool surface one corpus entry drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `search_docs` on the MCP side, `devdocs search` on the CLI side.
    Search,
    /// `open_doc` on the MCP side, `devdocs open` on the CLI side.
    Open,
    /// `verify_doc` on the MCP side, `devdocs verify` on the CLI side.
    Verify,
}

/// The outcome both edges must agree on: a served envelope (exit 0,
/// stdout) or a miss envelope (exit 2, stderr on the CLI side, a
/// tool-level error on the MCP side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Both edges serve the result envelope.
    Hit,
    /// Both edges serve the error envelope.
    Miss,
}

/// One corpus entry: the query and its parameters, shared verbatim by
/// the MCP tool call and the CLI flags.
#[derive(Debug)]
pub struct CorpusQuery {
    /// The tool both edges run.
    pub kind: Kind,
    /// The query text (`Search`) or the ref (`Open`, `Verify`).
    pub query: &'static str,
    /// The library scope, when the query carries one.
    pub library: Option<&'static str>,
    /// The version pin, when the query carries one.
    pub version: Option<&'static str>,
    /// The MCP `detail` projection; the CLI has no `--detail` flag, so
    /// `detail` entries compare shape only, not bytes.
    pub detail: Option<&'static str>,
    /// The token budget, when the query carries one.
    pub max_tokens: Option<u64>,
    /// The ref, for `Open` and `Verify` entries.
    pub doc_ref: Option<&'static str>,
    /// The outcome both edges must agree on.
    pub outcome: Outcome,
}

/// One `Search` hit with no scope: a plain phrase over the corpus.
const fn plain(query: &'static str) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Search,
        query,
        library: None,
        version: None,
        detail: None,
        max_tokens: None,
        doc_ref: None,
        outcome: Outcome::Hit,
    }
}

/// One scoped `Search` (`library`, optional `version`): a hit.
const fn scoped(
    query: &'static str,
    library: &'static str,
    version: Option<&'static str>,
) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Search,
        query,
        library: Some(library),
        version,
        detail: None,
        max_tokens: None,
        doc_ref: None,
        outcome: Outcome::Hit,
    }
}

/// One `Search` that misses with the envelope both edges must echo.
const fn miss(query: &'static str, library: Option<&'static str>) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Search,
        query,
        library,
        version: None,
        detail: None,
        max_tokens: None,
        doc_ref: None,
        outcome: Outcome::Miss,
    }
}

/// One `Search` under a token budget small enough to truncate the page.
const fn budgeted(query: &'static str, max_tokens: u64) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Search,
        query,
        library: None,
        version: None,
        detail: None,
        max_tokens: Some(max_tokens),
        doc_ref: None,
        outcome: Outcome::Hit,
    }
}

/// One `Open` that serves the section, or misses on a version the index
/// does not carry.
const fn open(doc_ref: &'static str, outcome: Outcome) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Open,
        query: "",
        library: None,
        version: None,
        detail: None,
        max_tokens: None,
        doc_ref: Some(doc_ref),
        outcome,
    }
}

/// One MCP-only `Search` at a projected `detail` level: the CLI has no
/// `--detail` flag, so the gate snapshots the shape instead of bytes.
const fn detailed(query: &'static str, detail: &'static str) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Search,
        query,
        library: None,
        version: None,
        detail: Some(detail),
        max_tokens: None,
        doc_ref: None,
        outcome: Outcome::Hit,
    }
}

/// One `Verify` entry: the gate compares fields, not bytes — the MCP
/// tool reports `{"valid", "content_hash"}` while the CLI serves the
/// result envelope.
const fn verify(doc_ref: &'static str) -> CorpusQuery {
    CorpusQuery {
        kind: Kind::Verify,
        query: "",
        library: None,
        version: None,
        detail: None,
        max_tokens: None,
        doc_ref: Some(doc_ref),
        outcome: Outcome::Hit,
    }
}

/// The corpus: exactly 50 entries.
///
/// Composition: 19 plain searches (two on the symbol rung), 5
/// library-scoped searches, 4 version-pinned searches that hit, 2
/// version-pinned opens that miss (`PKG_NOT_INDEXED`), 3
/// license-refused searches (`NOT_AVAILABLE`), 3 gibberish searches
/// (`NO_RESULT`), 3 budgeted searches (truncated first page), 6 opens,
/// 2 detail projections (MCP-only shape snapshots), and 3 verifies.
pub const CORPUS: [CorpusQuery; 50] = [
    // 19 plain searches: phrases over the acme corpus, two on the symbol
    // rung.
    plain("quick start builds"),
    plain("first widget project"),
    plain("first dashboard project"),
    plain("install toolchain"),
    plain("api key"),
    plain("access token"),
    plain("boxed frame"),
    plain("paints a panel"),
    plain("stacks widgets"),
    plain("arranges widgets"),
    plain("installer export"),
    plain("installer import"),
    plain("upgrade guide"),
    plain("migration guide"),
    plain("error channel"),
    plain("failure channel"),
    plain("core module"),
    plain("makeWidget"),
    plain("renderBox"),
    // 5 library-scoped searches.
    scoped("quick start", "acme", None),
    scoped("error channel", "acme", None),
    scoped("core module", "acme", None),
    scoped("beta starter", "beta", None),
    scoped("sandbox fixes", "beta", None),
    // 4 version-pinned searches that hit.
    scoped("upgrade guide", "acme", Some("1.0")),
    scoped("migration guide", "acme", Some("2.0")),
    scoped("makeWidget", "acme", Some("1.0")),
    scoped("paintPanel", "acme", Some("2.0")),
    // 3 license-refused searches: both edges answer `NOT_AVAILABLE`.
    miss("blocked marker", Some("blocked")),
    miss("expiring marker", Some("expiring")),
    miss("denied marker", Some("denied")),
    // 3 gibberish searches: both edges answer `NO_RESULT`.
    miss("zzz-nonexistent-thing", None),
    miss("qqqq wwww", None),
    miss("xyzzy plugh zorkmid", None),
    // 3 budgeted searches: a tiny budget truncates the first page, and
    // the truncation state plus the resume cursor compare byte for byte.
    budgeted("quick start builds", 15),
    budgeted("install toolchain", 15),
    budgeted("makeWidget", 15),
    // 2 version-pinned opens that miss: the index does not carry the
    // pinned version, and both edges answer `PKG_NOT_INDEXED`.
    open("docs://acme/v3.0/getting-started", Outcome::Miss),
    open("docs://acme/v9.9/reference", Outcome::Miss),
    // 6 opens that serve.
    open("docs://acme/v1.0/getting-started", Outcome::Hit),
    open("docs://acme/v2.0/widgets", Outcome::Hit),
    open("docs://acme/v1.0/install/upgrade", Outcome::Hit),
    open("docs://acme/v2.0/reference/errors", Outcome::Hit),
    open("docs://beta/v1.0/starter", Outcome::Hit),
    open(
        "docs://acme/v1.0/symbols/makewidget#makewidget",
        Outcome::Hit,
    ),
    // 2 detail projections: MCP-only shape snapshots.
    detailed("quick start builds", "name"),
    detailed("error channel", "summary"),
    // 3 verifies: field-level comparison against the CLI envelope.
    verify("docs://acme/v1.0/getting-started"),
    verify("docs://acme/v1.0/install/upgrade"),
    verify("docs://beta/v1.0/starter"),
];
