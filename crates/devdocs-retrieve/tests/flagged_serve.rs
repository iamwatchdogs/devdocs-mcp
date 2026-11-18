//! The flag-aware serving surface: the quarantined-only miss, the
//! review-flag confidence penalty, the `code_caution` envelope field, and
//! the output-side secret redaction (docs/implementation/07 §Injection
//! filter, docs/implementation/08 §Result envelope, docs/implementation/09
//! §Secrets and redaction). Chunks seed directly through the testkit,
//! bypassing ingest, so the flags and the planted secrets arrive verbatim.

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use devdocs_model::MissReason;
use devdocs_model::VersionStatus;
use devdocs_retrieve::Plan;
use devdocs_retrieve::Query;
use devdocs_retrieve::RetrieveError;
use devdocs_retrieve::serve::{open, search, symbol};
use devdocs_store::Store;
use tempfile::TempDir;

/// The searchable body the seeded chunks carry.
const BODY: &str = "The routing layer maps requests to handlers.";

/// Seeds an active `approved_snippet` source of `acme` with one chunk per
/// `(flags, text)` row, each under its own section so the refs stay
/// distinct. Returns the store and the directory the search runs from.
fn flagged_store(rows: &[(devdocs_testkit::ChunkFlags<'_>, &str)]) -> (TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("docs.db");
    let store = devdocs_testkit::open_migrated_and(&db, |conn| {
        let source = devdocs_testkit::seed_source(conn, "acme", true)?;
        for (index, (flags, text)) in rows.iter().enumerate() {
            devdocs_testkit::seed_flagged_chunk(
                conn,
                source,
                i64::try_from(index).unwrap_or(0),
                &format!("[\"Section {index}\"]"),
                None,
                &format!("# Section {index}\n\n{text}"),
                *flags,
            )?;
        }
        Ok(())
    })
    .unwrap();
    devdocs_testkit::approve_source_to(
        &db,
        "acme",
        devdocs_model::LicenseStatus::ApprovedSnippet,
        "test:bootstrap",
    )
    .unwrap();
    (dir, store)
}

/// The quarantined-only miss: a query whose only matches sit in
/// quarantined chunks misses with `quarantined_only`, not the generic
/// `low_confidence` (docs/implementation/07 §Injection filter).
#[test]
fn a_quarantined_only_match_misses_quarantined_only() {
    let (dir, store) = flagged_store(&[(
        devdocs_testkit::ChunkFlags {
            quarantined: true,
            rule_names: Some("injection"),
            ..devdocs_testkit::ChunkFlags::default()
        },
        BODY,
    )]);
    let query = Query::new("routing", Some("acme"), None);
    let error = search(&store, &query, dir.path(), &Plan::defaults()).unwrap_err();
    assert!(
        matches!(error, RetrieveError::Miss(MissReason::QuarantinedOnly)),
        "expected the quarantined-only miss, got {error}"
    );
    // The envelope the CLI edge prints carries the miss reason.
    let model = devdocs_retrieve::wiring::map_retrieve_error(error);
    let envelope = model.to_envelope();
    assert_eq!(
        envelope.miss_reason,
        Some(MissReason::QuarantinedOnly),
        "envelope: {envelope:?}"
    );
    assert_eq!(model.exit_code(), 2);
    assert_eq!(
        envelope.hint.as_deref(),
        Some("run: devdocs db quarantine list")
    );
}

/// The quarantined-only miss through the symbol rung: a quarantined chunk
/// whose `symbols` row matches the query — its body matches no FTS term —
/// misses with `quarantined_only`, not the generic `low_confidence`
/// (docs/implementation/07 §Injection filter: the rungs matched only
/// quarantined chunks, and the symbol rung is one of them).
#[test]
fn a_quarantined_symbol_only_match_misses_quarantined_only() {
    let (dir, store) = flagged_store(&[(
        devdocs_testkit::ChunkFlags {
            quarantined: true,
            rule_names: Some("injection"),
            ..devdocs_testkit::ChunkFlags::default()
        },
        "It hides secrets in plain sight.",
    )]);
    // The symbol rung drives off the `symbols` table, so the row lands
    // here; the body carries no token the FTS rung could match.
    let conn = rusqlite::Connection::open(dir.path().join("docs.db")).unwrap();
    conn.execute(
        "INSERT INTO symbols(symbol, chunk_id, kind) \
         SELECT 'makeWidgetPro', chunk_id, 'function' FROM chunks LIMIT 1",
        [],
    )
    .unwrap();
    drop(conn);

    let query = Query::new("makeWidget()", Some("acme"), None);
    let error = search(&store, &query, dir.path(), &Plan::defaults()).unwrap_err();
    assert!(
        matches!(error, RetrieveError::Miss(MissReason::QuarantinedOnly)),
        "expected the quarantined-only miss, got {error}"
    );
}

/// The `symbol` tool path names the quarantined-only miss too: a pinned
/// lookup whose only symbol matches sit on quarantined chunks misses with
/// `quarantined_only`, not `symbol_absent` (docs/implementation/07
/// §Injection filter).
#[test]
fn a_quarantined_symbol_match_misses_quarantined_only_on_the_symbol_path() {
    let (dir, store) = flagged_store(&[(
        devdocs_testkit::ChunkFlags {
            quarantined: true,
            rule_names: Some("injection"),
            ..devdocs_testkit::ChunkFlags::default()
        },
        "It hides secrets in plain sight.",
    )]);
    let conn = rusqlite::Connection::open(dir.path().join("docs.db")).unwrap();
    conn.execute(
        "INSERT INTO symbols(symbol, chunk_id, kind) \
         SELECT 'makeWidgetPro', chunk_id, 'function' FROM chunks LIMIT 1",
        [],
    )
    .unwrap();
    drop(conn);

    let error = symbol(&store, "acme", None, "makeWidget()", &Plan::defaults()).unwrap_err();
    assert!(
        matches!(error, RetrieveError::Miss(MissReason::QuarantinedOnly)),
        "expected the quarantined-only miss, got {error}"
    );
}

/// A quarantined match plus a clean match still serves the clean one, and
/// stays the `low_confidence` miss shape — the quarantined reason only
/// names the case where nothing else matched.
#[test]
fn a_clean_match_serves_despite_a_quarantined_sibling() {
    let (dir, store) = flagged_store(&[
        (
            devdocs_testkit::ChunkFlags {
                quarantined: true,
                ..devdocs_testkit::ChunkFlags::default()
            },
            BODY,
        ),
        (
            devdocs_testkit::ChunkFlags::default(),
            "Routing plans the request path.",
        ),
    ]);
    let query = Query::new("routing", Some("acme"), None);
    let envelope = search(&store, &query, dir.path(), &Plan::defaults()).unwrap();
    assert_eq!(envelope.results.len(), 1);
}

/// The review-flag confidence penalty: an identical chunk that carries the
/// review flag serves with exactly half the confidence the same corpus
/// serves unflagged (docs/implementation/07 §Injection filter: the fixed
/// × 0.5 factor). Two stores with the same seed shape keep the fused
/// score comparable.
#[test]
fn a_review_flagged_hit_serves_at_half_the_clean_confidence() {
    let seeds = |flagged: bool| {
        flagged_store(&[(
            devdocs_testkit::ChunkFlags {
                review_flagged: flagged,
                rule_names: flagged.then_some("injection"),
                ..devdocs_testkit::ChunkFlags::default()
            },
            BODY,
        )])
    };
    let (flagged_dir, flagged) = seeds(true);
    let (clean_dir, clean) = seeds(false);
    let query = Query::new("routing", Some("acme"), None);
    let flagged = search(&flagged, &query, flagged_dir.path(), &Plan::defaults()).unwrap();
    let clean = search(&clean, &query, clean_dir.path(), &Plan::defaults()).unwrap();
    assert_eq!(flagged.results.len(), 1);
    assert_eq!(clean.results.len(), 1);
    let flagged_score = flagged.results[0].confidence;
    let clean_score = clean.results[0].confidence;
    assert!(
        flagged_score > 0.0 && clean_score > 0.0,
        "both hits must carry a fused score, got {flagged_score} vs {clean_score}"
    );
    assert_eq!(
        flagged_score,
        clean_score * 0.5,
        "the flagged confidence must be exactly half the clean one"
    );
    // The score moves, the serve does not: the flagged hit still serves.
    assert_eq!(flagged.results[0].doc_ref, clean.results[0].doc_ref);
}

/// The review-flag penalty on the symbol path: `symbol` runs the same
/// fusion-and-pack path as search, so a review-flagged chunk a symbol
/// lookup resolves serves at the × 0.5 confidence of its clean twin
/// (docs/implementation/07 §Injection filter). Two stores with the same
/// seed shape keep the fused score comparable.
#[test]
fn a_review_flagged_symbol_hit_serves_at_half_the_clean_confidence() {
    let seeds = |flagged: bool| {
        let (dir, store) = flagged_store(&[(
            devdocs_testkit::ChunkFlags {
                review_flagged: flagged,
                rule_names: flagged.then_some("injection"),
                ..devdocs_testkit::ChunkFlags::default()
            },
            BODY,
        )]);
        let conn = rusqlite::Connection::open(dir.path().join("docs.db")).unwrap();
        conn.execute(
            "INSERT INTO symbols(symbol, chunk_id, kind) \
             SELECT 'makeWidget', chunk_id, 'function' FROM chunks LIMIT 1",
            [],
        )
        .unwrap();
        (dir, store)
    };
    let (_flagged_dir, flagged) = seeds(true);
    let (_clean_dir, clean) = seeds(false);
    let flagged = symbol(&flagged, "acme", None, "makeWidget", &Plan::defaults()).unwrap();
    let clean = symbol(&clean, "acme", None, "makeWidget", &Plan::defaults()).unwrap();
    assert_eq!(flagged.results.len(), 1, "flagged: {flagged:?}");
    assert_eq!(clean.results.len(), 1, "clean: {clean:?}");
    let flagged_score = flagged.results[0].confidence;
    let clean_score = clean.results[0].confidence;
    assert!(
        flagged_score > 0.0 && clean_score > 0.0,
        "both hits must carry a fused score, got {flagged_score} vs {clean_score}"
    );
    assert_eq!(
        flagged_score,
        clean_score * 0.5,
        "the flagged confidence must be exactly half the clean one"
    );
}

/// A caution chunk passes `code_caution` into the envelope item, and the
/// JSON carries the key only when it is true — the byte-identity gate
/// keeps the clean output unchanged (docs/implementation/08 §Result
/// envelope).
#[test]
fn code_caution_reaches_the_envelope_and_clean_output_omits_it() {
    let (dir, store) = flagged_store(&[
        (
            devdocs_testkit::ChunkFlags {
                code_caution: true,
                ..devdocs_testkit::ChunkFlags::default()
            },
            BODY,
        ),
        (
            devdocs_testkit::ChunkFlags::default(),
            "Routing plans the request path.",
        ),
    ]);
    let query = Query::new("routing", Some("acme"), None);
    let envelope = search(&store, &query, dir.path(), &Plan::defaults()).unwrap();
    assert_eq!(envelope.results.len(), 2);
    let statuses: Vec<bool> = envelope
        .results
        .iter()
        .map(|item| item.code_caution)
        .collect();
    assert!(
        statuses.contains(&true) && statuses.contains(&false),
        "one caution item and one clean item expected: {statuses:?}"
    );
    let caution = envelope
        .results
        .iter()
        .find(|item| item.code_caution)
        .unwrap();
    let json = serde_json::to_string(&caution).unwrap();
    assert!(json.contains(r#""code_caution":true"#), "json: {json}");
    let clean = envelope
        .results
        .iter()
        .find(|item| !item.code_caution)
        .unwrap();
    let json = serde_json::to_string(clean).unwrap();
    assert!(!json.contains("code_caution"), "json: {json}");
}

/// The output-side redaction gate: a chunk seeded directly into the store
/// (bypassing ingest, which strips at fetch time) serves its planted
/// `ghp_` token as the `[REDACTED:github_token]` marker, on the search
/// snippet path and on the `open` page path
/// (docs/implementation/09 §Secrets and redaction).
#[test]
fn a_planted_token_serves_redacted() {
    let token = format!("ghp_{}", "0123456789abcdefghijklmnopqrstuvwxyz");
    let text = format!("Run `export GITHUB_TOKEN={token}` before the request.");
    let (dir, store) = flagged_store(&[(devdocs_testkit::ChunkFlags::default(), &text)]);
    let query = Query::new("GITHUB_TOKEN", Some("acme"), None);
    let envelope = search(&store, &query, dir.path(), &Plan::defaults()).unwrap();
    assert_eq!(envelope.results.len(), 1, "envelope: {envelope:?}");
    let snippet = &envelope.results[0].snippet;
    assert!(
        snippet.contains("[REDACTED:github_token]"),
        "snippet: {snippet}"
    );
    assert!(!snippet.contains("ghp_"), "the raw token must not serve");

    // The open path redacts the served page too.
    let item = open(&store, "docs://acme/v1.0.0/section-0", &Plan::defaults()).unwrap();
    let page = &item.results[0].snippet;
    assert!(page.contains("[REDACTED:github_token]"), "page: {page}");
    assert!(!page.contains("ghp_"), "the raw token must not serve");
}

/// The `open` path keeps the version status verified and the caution flag
/// on a caution chunk — the ref-serving items carry the flag like the
/// search items do.
#[test]
fn the_open_path_carries_the_caution_flag() {
    let (_dir, store) = flagged_store(&[(
        devdocs_testkit::ChunkFlags {
            code_caution: true,
            ..devdocs_testkit::ChunkFlags::default()
        },
        BODY,
    )]);
    let item = open(&store, "docs://acme/v1.0.0/section-0", &Plan::defaults()).unwrap();
    assert_eq!(item.results.len(), 1);
    assert!(item.results[0].code_caution);
    assert_eq!(item.results[0].version_status, VersionStatus::Verified);
}
