//! Property tests for the FTS5 MATCH query builder.
//!
//! Required property (docs/implementation/04 §Properties): the query
//! escaper produces a valid FTS5 MATCH expression for any input string.
//! Validity is checked against the real FTS5 implementation: the built
//! expression must execute as a MATCH against a populated schema-0001
//! database without error.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use proptest::prelude::*;
use rusqlite::Connection;
use tempfile::tempdir;

use devdocs_store::{Store, build_match_query};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any input string yields an expression the FTS5 engine accepts.
    #[test]
    fn any_input_yields_a_valid_match_expression(input in ".*") {
        let expr = build_match_query(&input);
        let expr = expr.as_ref();

        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("docs.db")).unwrap();
        store.migrate().unwrap();
        let conn = Connection::open(dir.path().join("docs.db")).unwrap();
        conn.execute(
            "INSERT INTO sources(library, version, kind, origin_url, license_status, fetched_at, active) \
             VALUES ('p', '1', 'llms_txt', 'https://example.test/p', 'approved_index', \
                     '2026-01-01T00:00:00Z', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO chunks(source_id, content_hash, section_path, text, token_count, ordinal) \
             VALUES (1, ?1, '[\"Docs\"]', 'The router hook configures navigation. use-route stays whole.', 8, 0)",
            rusqlite::params![&[7u8; 32][..]],
        ).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                rusqlite::params![expr],
                |row| row.get(0),
            )
            .unwrap_or_else(|e| panic!("MATCH failed for input {input:?} -> {expr:?}: {e}"));
        prop_assert!(count >= 0);
    }

    /// Operator-only input becomes a valid query that matches nothing.
    #[test]
    fn operator_only_input_matches_nothing(ops in "(AND|OR|NOT|NEAR|-|\\*|\\^|:|\\(|\\)){1,10}") {
        let expr = build_match_query(&ops).into_owned();
        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("docs.db")).unwrap();
        store.migrate().unwrap();
        let conn = Connection::open(dir.path().join("docs.db")).unwrap();
        conn.execute(
            "INSERT INTO sources(library, version, kind, origin_url, license_status, fetched_at, active) \
             VALUES ('p', '1', 'llms_txt', 'https://example.test/p', 'approved_index', \
                     '2026-01-01T00:00:00Z', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO chunks(source_id, content_hash, section_path, text, token_count, ordinal) \
             VALUES (1, ?1, '[\"Docs\"]', 'plain prose body', 3, 0)",
            rusqlite::params![&[7u8; 32][..]],
        ).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH ?1",
                rusqlite::params![expr],
                |row| row.get(0),
            )
            .unwrap_or_else(|e| panic!("operator-only MATCH failed for {ops:?} -> {expr:?}: {e}"));
        prop_assert_eq!(count, 0, "operator-only input must not match prose");
    }
}
