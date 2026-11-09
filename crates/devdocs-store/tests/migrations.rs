//! Migration runner behavior on a temp database file.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use devdocs_store::migrate;
use rusqlite::Connection;

/// Names of every schema object the shipped migrations must create.
const SCHEMA_OBJECTS: &[(&str, &str)] = &[
    ("table", "meta"),
    ("table", "sources"),
    ("table", "chunks"),
    ("table", "symbols"),
    ("table", "session_trajectory"),
    ("table", "chunks_fts"),
    ("view", "chunks_content"),
    ("trigger", "chunks_ai"),
    ("trigger", "chunks_ad"),
    ("index", "idx_sources_library"),
    ("index", "idx_sources_active"),
    ("index", "idx_chunks_symbol"),
    ("index", "idx_chunks_hash"),
    ("index", "idx_symbols_chunk"),
    ("index", "idx_symbols_symbol_nocase"),
];

/// Checks whether a column exists on a table after the migrations ran.
fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
        rusqlite::params![table, column],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap()
}

/// Inserts a pending-license source and reads its expiry back, which must be
/// unset for rows written before the gate shipped.
fn pending_license_expiry_is_unset(conn: &Connection) -> Option<String> {
    conn.execute(
        "INSERT INTO sources(library, version, kind, origin_url, license_status, fetched_at, \
         active) VALUES ('acme', '1.0', 'llms_txt', 'https://docs.test/llms.txt', \
         'pending_review', '2026-01-01T00:00:00Z', 0)",
        [],
    )
    .unwrap();
    conn.query_row(
        "SELECT license_expires_at FROM sources WHERE library = 'acme'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

/// Applies every shipped migration to a fresh temp file and checks every
/// object, including the license-gate expiry column.
#[test]
fn fresh_database_reaches_the_latest_version_with_all_objects() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = Connection::open(dir.path().join("docs.db")).unwrap();

    assert_eq!(
        migrate(&mut conn)
            .unwrap()
            .into_iter()
            .map(|m| (m.version, m.name))
            .collect::<Vec<_>>(),
        vec![
            (1, "0001_init".to_owned()),
            (2, "0002_symbols_symbol_nocase".to_owned()),
            (3, "0003_page_url".to_owned()),
            (4, "0004_license_gate".to_owned()),
        ]
    );

    for (kind, name) in SCHEMA_OBJECTS {
        let found: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                rusqlite::params![kind, name],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "{kind} {name} must exist after migration");
    }

    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);

    // The page-url column exists on chunks.
    assert!(
        column_exists(&conn, "chunks", "page_url"),
        "chunks.page_url must exist after migration"
    );

    // The expiry column exists, is nullable, and existing rows carry no
    // expiry.
    assert_eq!(pending_license_expiry_is_unset(&conn), None);
}

/// A second run applies nothing, leaves `user_version` alone, and keeps the
/// schema bookkeeping row from the first run.
#[test]
fn re_migrating_an_up_to_date_database_applies_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = Connection::open(dir.path().join("docs.db")).unwrap();
    migrate(&mut conn).unwrap();
    let created_at: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_created_at'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let second = migrate(&mut conn).unwrap();
    assert_eq!(
        second,
        Vec::<devdocs_store::AppliedMigration>::new(),
        "an up-to-date database must get no migrations"
    );

    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    let created_after: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_created_at'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created_after, created_at, "schema 0001 must not re-run");
}
