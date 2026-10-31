//! Publish behavior: atomic replacement, forced-failure rollback, and the
//! license gate (docs/implementation/07 §Atomic publish).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration as StdDuration;

use devdocs_model::{LicenseStatus, SourceId, SourceKind, SourceRecord};
use devdocs_store::{NewChunk, Store, build_match_query, content_hash};
use rusqlite::Connection;
use tempfile::tempdir;
use time::OffsetDateTime;
use url::Url;

/// A store with schema applied on a temp file, plus a raw connection for
/// assertions.
struct Rig {
    store: Store,
    conn: Connection,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempdir().unwrap();
    let store = Store::open(&dir.path().join("docs.db")).unwrap();
    store.migrate().unwrap();
    let conn = Connection::open(dir.path().join("docs.db")).unwrap();
    Rig {
        store,
        conn,
        _dir: dir,
    }
}

/// A publishable source record at the index approval stage.
fn source(status: LicenseStatus) -> SourceRecord {
    SourceRecord {
        id: SourceId(0),
        library: "next.js".to_owned(),
        version: "16.2.9".to_owned(),
        kind: SourceKind::LlmsTxt,
        origin_url: Url::parse("https://example.test/next/llms.txt").unwrap(),
        commit_sha: Some("abc123".to_owned()),
        license: Some("MIT".to_owned()),
        license_status: status,
        fetched_at: OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap(),
    }
}

/// A one-line chunk body with the given ordinal and body term.
fn chunk(ordinal: u32, symbol: Option<(&str, &str)>, body: &str) -> NewChunk {
    NewChunk {
        section_path: vec!["Routing".to_owned()],
        symbol: symbol.map(|(name, _)| name.to_owned()),
        symbol_kind: symbol.map(|(_, kind)| kind.to_owned()),
        text: format!("## Routing\n\n{body}"),
        token_count: 16,
        content_hash: content_hash(body),
        ordinal,
    }
}

/// A publish upserts the source, inserts chunks and symbols, and flips the
/// source active; the generation is searchable right away.
#[test]
fn publish_activates_and_inserts_the_generation() {
    let rig = rig();
    rig.store
        .publish_generation(
            &source(LicenseStatus::ApprovedIndex),
            &[
                chunk(
                    0,
                    Some(("use_router", "function")),
                    "The use_router hook configures routing.",
                ),
                chunk(1, None, "Middleware runs before routing decisions."),
            ],
        )
        .unwrap();

    let (active, fetched_at, commit_sha): (i64, String, String) = rig
        .conn
        .query_row(
            "SELECT active, fetched_at, commit_sha FROM sources WHERE library = 'next.js'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(active, 1, "a successful publish flips the source active");
    assert!(fetched_at.ends_with('Z'), "fetched_at is RFC 3339 UTC");
    assert_eq!(commit_sha, "abc123");

    let chunks: i64 = rig
        .conn
        .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(chunks, 2);

    let symbols: (i64, String) = rig
        .conn
        .query_row(
            "SELECT count(*), kind FROM symbols WHERE symbol = 'use_router'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(symbols, (1, "function".to_owned()));

    let expr = build_match_query("use_router").into_owned();
    let hits = rig.store.search_fts(&expr, 10).unwrap();
    assert_eq!(hits.len(), 1, "publish-then-search round trip");
    assert_eq!(hits[0].symbol.as_deref(), Some("use_router"));
}

/// A second publish replaces the chunk set: the old body drops out of the
/// FTS index and the new one drops in.
#[test]
fn republish_replaces_the_chunk_set() {
    let rig = rig();
    let src = source(LicenseStatus::ApprovedIndex);
    rig.store
        .publish_generation(
            &src,
            &[chunk(0, None, "generation one mentions alpha tokens.")],
        )
        .unwrap();

    rig.store
        .publish_generation(
            &src,
            &[
                chunk(
                    0,
                    Some(("rewrite", "function")),
                    "generation two covers beta rewrites.",
                ),
                chunk(1, None, "generation two adds gamma middleware."),
            ],
        )
        .unwrap();

    let chunks: i64 = rig
        .conn
        .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(chunks, 2, "the old set is gone, the new set is in");

    let alpha = build_match_query("alpha").into_owned();
    assert!(
        rig.store.search_fts(&alpha, 10).unwrap().is_empty(),
        "old-generation terms must leave the index"
    );
    let beta = build_match_query("beta").into_owned();
    assert_eq!(rig.store.search_fts(&beta, 10).unwrap().len(), 1);
}

/// A forced failure mid-publish rolls everything back: the old generation
/// stays active and searchable.
#[test]
fn forced_failure_keeps_the_old_generation_searchable() {
    let rig = rig();
    let src = source(LicenseStatus::ApprovedIndex);
    rig.store
        .publish_generation(
            &src,
            &[chunk(0, None, "the stable generation holds ground.")],
        )
        .unwrap();

    // Two chunks share ordinal 0, so the second insert violates
    // UNIQUE(source_id, ordinal) after the old rows are already deleted.
    let broken = vec![
        chunk(0, None, "a new generation that never lands."),
        chunk(0, None, "a duplicate ordinal forces the failure."),
    ];
    let error = rig.store.publish_generation(&src, &broken).unwrap_err();
    assert!(
        matches!(error, devdocs_store::StoreError::Sql(_)),
        "expected the unique-violation error, got {error:?}"
    );

    let chunks: i64 = rig
        .conn
        .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(chunks, 1, "the old generation must survive");

    let (active,): (i64,) = rig
        .conn
        .query_row("SELECT active FROM sources", [], |row| Ok((row.get(0)?,)))
        .unwrap();
    assert_eq!(active, 1, "the source stays active");

    let stable = build_match_query("stable").into_owned();
    let hits = rig.store.search_fts(&stable, 10).unwrap();
    assert_eq!(hits.len(), 1, "the old generation stays searchable");
}

/// The license gate refuses sources below the index stage before anything
/// is written.
#[test]
fn license_gate_refuses_publish_below_the_index_stage() {
    let rig = rig();
    for status in [
        LicenseStatus::PendingReview,
        LicenseStatus::ApprovedFetch,
        LicenseStatus::Denied,
        LicenseStatus::Expired,
        LicenseStatus::Revoked,
    ] {
        let error = rig
            .store
            .publish_generation(&source(status), &[chunk(0, None, "never lands")])
            .unwrap_err();
        match error {
            devdocs_store::StoreError::LicenseNotApproved { status: refused } => {
                assert_eq!(refused, status);
            }
            other => panic!("expected LicenseNotApproved for {status}, got {other:?}"),
        }
    }

    let rows: i64 = rig
        .conn
        .query_row("SELECT count(*) FROM sources", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0, "a refused publish must write nothing");
}

/// Chunk validation rejects symbol entries missing their kind.
#[test]
fn symbol_without_kind_is_refused() {
    let rig = rig();
    let mut broken = chunk(0, None, "body");
    broken.symbol = Some("use_router".to_owned());
    let error = rig
        .store
        .publish_generation(&source(LicenseStatus::ApprovedIndex), &[broken])
        .unwrap_err();
    assert!(
        matches!(error, devdocs_store::StoreError::InvalidChunk(_)),
        "expected InvalidChunk, got {error:?}"
    );
}

/// The fetched-at format survives a round trip through storage.
#[test]
fn fetched_at_is_rfc3339_utc() {
    let rig = rig();
    rig.store
        .publish_generation(
            &source(LicenseStatus::ApprovedIndex),
            &[chunk(0, None, "timestamp probe")],
        )
        .unwrap();
    let stored: String = rig
        .conn
        .query_row("SELECT fetched_at FROM sources", [], |row| row.get(0))
        .unwrap();
    let parsed =
        OffsetDateTime::parse(&stored, &time::format_description::well_known::Rfc3339).unwrap();
    let delta = parsed - OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap();
    assert!(delta.abs() < StdDuration::from_secs(1));
}
