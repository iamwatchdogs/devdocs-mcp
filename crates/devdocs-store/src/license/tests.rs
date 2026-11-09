#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::{AUTO_EXPIRY_OPERATOR, Store};
use crate::migrate::MigrateOptions;
use crate::store::StoreOptions;
use devdocs_model::LicenseStatus::{
    self, ApprovedCache, ApprovedEmbed, ApprovedFetch, ApprovedIndex, ApprovedSnippet, Denied,
    Expired, PendingReview, Revoked,
};

/// A migrated store plus its kept-alive temp directory.
fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(&dir.path().join("docs.db"), &StoreOptions::default()).unwrap();
    store.migrate().unwrap();
    (store, dir)
}

/// Inserts one source row straight into the store: the transition
/// tests drive states the publish gate would refuse. Rows of the same
/// library need distinct versions to satisfy the UNIQUE key.
fn insert_source(
    store: &Store,
    library: &str,
    version: &str,
    status: LicenseStatus,
    expires: Option<&str>,
) {
    let conn = store.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO sources(library, version, kind, origin_url, license_status, \
         license_expires_at, fetched_at, active) \
         VALUES (?1, ?2, 'llms_txt', 'https://docs.test/llms.txt', ?3, ?4, \
         '2026-01-01T00:00:00Z', 0)",
        rusqlite::params![library, version, status.as_str(), expires],
    )
    .unwrap();
}

/// The stored license state of the library's single row.
fn state_of(store: &Store, library: &str) -> LicenseStatus {
    let conn = store.conn.lock().unwrap();
    let raw: String = conn
        .query_row(
            "SELECT license_status FROM sources WHERE library = ?1",
            [library],
            |row| row.get(0),
        )
        .unwrap();
    LicenseStatus::parse(&raw).unwrap()
}

/// The audit values in `meta`, in key order.
fn audit_lines(store: &Store) -> Vec<String> {
    let conn = store.conn.lock().unwrap();
    let mut statement = conn
        .prepare("SELECT value FROM meta WHERE key LIKE 'audit:%' ORDER BY key")
        .unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// Migration 0004 applies once: `user_version` lands on 4 and a
/// re-run applies nothing (docs/implementation/06 §Migrations).
#[test]
fn migration_0004_applies_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_with(&dir.path().join("docs.db"), &StoreOptions::default()).unwrap();
    assert_eq!(store.user_version().unwrap(), 0);

    let applied = store.migrate().unwrap();
    let names: Vec<_> = applied.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"0004_license_gate"), "names: {names:?}");
    assert_eq!(store.user_version().unwrap(), 4);

    let reapplied = store
        .migrate_with_options(&MigrateOptions::default())
        .unwrap();
    assert!(reapplied.is_empty(), "re-run must be a no-op");
}

/// Asserts one audit line carries the docs/07 fields: the audit prefix,
/// the library, the from/to pair, the operator, and the note exactly
/// when one was given.
fn assert_audit_line(line: &str, from: LicenseStatus, to: LicenseStatus, noted: bool) {
    assert!(line.starts_with("(audit:"), "line: {line}");
    assert!(line.contains(", acme, "), "line: {line}");
    assert!(
        line.contains(&format!(", {}, {}, ", from.as_str(), to.as_str())),
        "line: {line}"
    );
    assert!(line.contains(", op"), "line: {line}");
    assert!(line.ends_with(')'), "line: {line}");
    assert_eq!(
        line.contains(", robots allowlist re-checked"),
        noted,
        "line: {line}"
    );
}

/// Every legal transition moves the row and appends exactly one audit
/// line carrying the docs/07 fields: timestamp, library, from, to,
/// operator.
#[test]
fn every_legal_transition_moves_and_audits() {
    let all = [
        PendingReview,
        ApprovedFetch,
        ApprovedIndex,
        ApprovedEmbed,
        ApprovedCache,
        ApprovedSnippet,
        Denied,
        Expired,
        Revoked,
    ];
    // Derive the legal pairs from the model's own transition table; the
    // table keeps its separate exhaustive test in devdocs-model.
    let mut legal = Vec::new();
    for from in all {
        for to in all {
            if from.can_transition_to(to) {
                legal.push((from, to));
            }
        }
    }
    assert_eq!(legal.len(), 24, "the documented table has 24 legal moves");

    for (from, to) in legal {
        let (store, _dir) = store();
        insert_source(&store, "acme", "1.0", from, None);
        let noted = (from, to) == (Denied, PendingReview);
        let note = if noted {
            Some("robots allowlist re-checked")
        } else {
            None
        };
        let moved = store
            .transition_license("acme", to, "op", note, None)
            .unwrap_or_else(|e| panic!("{from:?} → {to:?}: {e}"));
        assert_eq!(moved, 1, "{from:?} → {to:?}");
        assert_eq!(state_of(&store, "acme"), to, "{from:?} → {to:?}");

        let lines = audit_lines(&store);
        assert_eq!(lines.len(), 1, "{from:?} → {to:?}");
        assert_audit_line(&lines[0], from, to, noted);
    }
}

/// Every illegal transition errors with the typed refusal, leaves the
/// stored state unchanged, and writes no audit line (AC2).
#[test]
fn every_illegal_transition_refuses_without_a_trace() {
    let all = [
        PendingReview,
        ApprovedFetch,
        ApprovedIndex,
        ApprovedEmbed,
        ApprovedCache,
        ApprovedSnippet,
        Denied,
        Expired,
        Revoked,
    ];
    for from in all {
        for to in all {
            // The store under test calls the same predicate; the model
            // table keeps its own exhaustive test.
            if from.can_transition_to(to) {
                continue;
            }
            let (store, _dir) = store();
            insert_source(&store, "acme", "1.0", from, None);
            let error = store
                .transition_license("acme", to, "op", Some("note"), None)
                .unwrap_err();
            let crate::error::StoreError::InvalidTransition {
                from: error_from,
                to: error_to,
            } = error
            else {
                panic!("{from:?} → {to:?}: expected InvalidTransition, got {error:?}");
            };
            assert_eq!(error_from, from, "{from:?} → {to:?}");
            assert_eq!(error_to, to);
            assert_eq!(state_of(&store, "acme"), from, "{from:?} → {to:?}");
            assert!(
                audit_lines(&store).is_empty(),
                "{from:?} → {to:?} must write no audit line"
            );
        }
    }
}

/// `denied → pending_review` without a note errors and changes
/// nothing; with a note the note lands in the audit line.
#[test]
fn reopening_a_denied_source_requires_a_note() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", Denied, None);

    let error = store
        .transition_license("acme", PendingReview, "op", None, None)
        .unwrap_err();
    assert!(
        matches!(error, crate::error::StoreError::MissingNote),
        "error: {error:?}"
    );
    assert_eq!(state_of(&store, "acme"), Denied);
    assert_eq!(audit_lines(&store), Vec::<String>::new());

    let error = store
        .transition_license("acme", PendingReview, "op", Some("   "), None)
        .unwrap_err();
    assert!(matches!(error, crate::error::StoreError::MissingNote));

    store
        .transition_license("acme", PendingReview, "op", Some("ToS clarified"), None)
        .unwrap();
    assert_eq!(state_of(&store, "acme"), PendingReview);
    let lines = audit_lines(&store);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(", ToS clarified)"), "line: {}", lines[0]);
}

/// A transition naming an unindexed library errors and writes nothing.
#[test]
fn unknown_library_refuses() {
    let (store, _dir) = store();
    let error = store
        .transition_license("ghost", PendingReview, "op", Some("note"), None)
        .unwrap_err();
    assert!(
        matches!(error, crate::error::StoreError::UnknownLibrary { .. }),
        "error: {error:?}"
    );
    assert_eq!(audit_lines(&store), Vec::<String>::new());
}

/// A transition carries an optional note on any legal move.
#[test]
fn any_transition_may_carry_a_note() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", PendingReview, None);
    store
        .transition_license("acme", Denied, "op", Some("robots disallow"), None)
        .unwrap();
    let lines = audit_lines(&store);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(", robots disallow)"),
        "line: {}",
        lines[0]
    );
}

/// The transition stamps the expiry inside its own transaction: a
/// `Some` date writes `license_expires_at` on every moved row, and a
/// `None` leaves a recorded stamp untouched
/// (docs/implementation/07 §License gate).
#[test]
fn transition_license_stamps_the_expiry_atomically() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", ApprovedIndex, Some("2026-01-01"));
    insert_source(&store, "acme", "2.0", ApprovedIndex, None);

    let moved = store
        .transition_license("acme", ApprovedEmbed, "op", None, Some("2999-01-01"))
        .unwrap();
    assert_eq!(moved, 2);
    let conn = store.conn.lock().unwrap();
    let stamped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sources \
             WHERE license_status = 'approved_embed' AND license_expires_at = '2999-01-01'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    assert_eq!(stamped, 2, "every moved row carries the stamp");

    // A transition without a date leaves the recorded stamp alone.
    store
        .transition_license("acme", ApprovedCache, "op", None, None)
        .unwrap();
    let conn = store.conn.lock().unwrap();
    let stamped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sources \
             WHERE license_status = 'approved_cache' AND license_expires_at = '2999-01-01'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    assert_eq!(stamped, 2, "the stamp survives the un-dated move");

    // The move kept its audit line shape: one line, no note field.
    let lines = audit_lines(&store);
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|line| line.contains(", op)") && !line.contains(", 2999-01-01")),
        "lines: {lines:?}"
    );
}

/// The sweep flips a past-expiry row to `expired` with the
/// `auto:expiry` audit line, leaves future and null expiries alone,
/// and skips rows already in a terminal refusal (AC3, AC6 store half).
#[test]
fn sweep_flips_only_passed_non_terminal_expiries() {
    let (store, _dir) = store();
    insert_source(&store, "past", "1.0", ApprovedIndex, Some("2026-01-01"));
    insert_source(&store, "future", "1.0", ApprovedIndex, Some("2999-01-01"));
    insert_source(&store, "no-expiry", "1.0", ApprovedIndex, None);
    // Rows already in a terminal refusal: set each up through its own
    // operator transition, with a past expiry recorded underneath.
    let already_terminal = [
        ("already-expired", Expired),
        ("already-denied", Denied),
        ("already-revoked", Revoked),
    ];
    for (library, terminal) in already_terminal {
        insert_source(&store, library, "1.0", ApprovedIndex, Some("2026-01-01"));
        store
            .transition_license(library, terminal, "op", None, None)
            .unwrap();
    }

    let flipped = store.sweep_expired("2026-11-06T12:00:00Z").unwrap();
    assert_eq!(flipped, 1, "only the past-expiry row flips");
    for (library, expected) in [
        ("past", Expired),
        ("future", ApprovedIndex),
        ("no-expiry", ApprovedIndex),
    ]
    .into_iter()
    .chain(already_terminal)
    {
        assert_eq!(state_of(&store, library), expected, "{library}");
    }

    let lines = audit_lines(&store);
    let auto: Vec<_> = lines
        .iter()
        .filter(|l| l.contains(AUTO_EXPIRY_OPERATOR))
        .collect();
    assert_eq!(auto.len(), 1, "lines: {lines:?}");
    assert!(
        auto[0].contains("(audit:") && auto[0].contains(", past, approved_index, expired, "),
        "line: {}",
        auto[0]
    );

    // The sweep is idempotent: the second run flips nothing.
    assert_eq!(store.sweep_expired("2026-11-06T12:00:00Z").unwrap(), 0);
    let auto: BTreeSet<_> = audit_lines(&store)
        .into_iter()
        .filter(|l| l.contains(AUTO_EXPIRY_OPERATOR))
        .collect();
    assert_eq!(auto.len(), 1, "no duplicate auto:expiry line");
}

/// `license_states` reports the distinct states of a library's rows
/// case-insensitively and returns empty for an un-indexed library.
#[test]
fn license_states_reports_distinct_states() {
    let (store, _dir) = store();
    assert_eq!(
        store.license_states("acme").unwrap(),
        Vec::<LicenseStatus>::new()
    );
    insert_source(&store, "acme", "1.0", ApprovedIndex, None);
    insert_source(&store, "acme", "2.0", ApprovedSnippet, None);
    assert_eq!(
        store.license_states("ACME").unwrap(),
        vec![ApprovedIndex, ApprovedSnippet]
    );
}

/// Same-instant audit lines collide on the key: the `-<n>` suffix
/// keeps both rows (driven through `append_audit_at` with a fixed
/// instant; the wall clock moves between real transitions).
#[test]
fn same_instant_audit_lines_get_a_suffix() {
    let (store, _dir) = store();
    insert_source(&store, "one", "1.0", ApprovedIndex, None);
    insert_source(&store, "two", "1.0", ApprovedIndex, None);
    {
        let mut conn = store.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        super::append_audit_at(
            &tx,
            "2026-11-06T12:00:00.1Z",
            "one",
            "approved_index",
            "expired",
            "auto:expiry",
            None,
        )
        .unwrap();
        super::append_audit_at(
            &tx,
            "2026-11-06T12:00:00.1Z",
            "two",
            "approved_index",
            "expired",
            "auto:expiry",
            None,
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let conn = store.conn.lock().unwrap();
    let mut statement = conn
        .prepare("SELECT key FROM meta WHERE key LIKE 'audit:%' ORDER BY key")
        .unwrap();
    let keys: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    assert_eq!(keys.len(), 2, "keys: {keys:?}");
    assert!(
        keys[0].starts_with("audit:") && keys[1].starts_with(&format!("{}-", keys[0])),
        "the second key must suffix the first: {keys:?}"
    );
}

/// The expiry boundary is 00:00 UTC of the recorded date: the
/// date-only stored value compares string-wise against the full
/// RFC 3339 instant, so a row flips on the recorded date itself —
/// expiry = today flips, expiry = tomorrow does not, expiry =
/// yesterday flips (docs/implementation/07 §Expiry sweep).
#[test]
fn expiry_boundary_flips_at_0000_utc_of_the_recorded_date() {
    // One clock read: a UTC-midnight crossing between two reads
    // would shift the dates against `now` and flake the boundary.
    let now_utc = OffsetDateTime::now_utc();
    let today = now_utc.date();
    let yesterday = today.previous_day().unwrap();
    let tomorrow = today.next_day().unwrap();
    let now = now_utc.format(&Rfc3339).unwrap();

    let (store, _dir) = store();
    insert_source(
        &store,
        "today",
        "1.0",
        ApprovedIndex,
        Some(&today.to_string()),
    );
    insert_source(
        &store,
        "yesterday",
        "1.0",
        ApprovedIndex,
        Some(&yesterday.to_string()),
    );
    insert_source(
        &store,
        "tomorrow",
        "1.0",
        ApprovedIndex,
        Some(&tomorrow.to_string()),
    );

    let flipped = store.sweep_expired(&now).unwrap();
    assert_eq!(flipped, 2, "today and yesterday flip; tomorrow does not");
    assert_eq!(state_of(&store, "today"), Expired);
    assert_eq!(state_of(&store, "yesterday"), Expired);
    assert_eq!(state_of(&store, "tomorrow"), ApprovedIndex);
}

/// The audit key always carries the fixed 9-digit subsecond field, so
/// the key shape never depends on the nanosecond value — a zero
/// nanosecond instant must not drop the fraction
/// (docs/implementation/06 §Migrations).
#[test]
fn audit_keys_carry_a_fixed_subsecond_field() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", ApprovedIndex, None);
    store
        .transition_license("acme", ApprovedEmbed, "op", None, None)
        .unwrap();
    let conn = store.conn.lock().unwrap();
    let key: String = conn
        .query_row("SELECT key FROM meta WHERE key LIKE 'audit:%'", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(conn);
    let ts = key.strip_prefix("audit:").expect("audit key").to_owned();
    // `YYYY-MM-DDTHH:MM:SS.999999999Z` is exactly 30 characters, with
    // the fraction at positions 20..=28.
    assert_eq!(ts.len(), 30, "key: {key}");
    assert_eq!(ts.as_bytes()[19], b'.', "key: {key}");
    assert!(
        ts.as_bytes()[20..29].iter().all(u8::is_ascii_digit),
        "key: {key}"
    );
    assert!(ts.ends_with('Z'), "key: {key}");
}

/// The license path matches the library case-insensitively: a
/// transition naming `ACME` moves the store row `acme`, and the audit
/// line carries the canonical stored spelling
/// (docs/implementation/07 §License gate).
#[test]
fn transition_license_matches_the_library_case_insensitively() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", ApprovedIndex, None);

    let moved = store
        .transition_license("ACME", ApprovedEmbed, "op", None, None)
        .unwrap();
    assert_eq!(moved, 1);
    assert_eq!(state_of(&store, "acme"), ApprovedEmbed);

    let lines = audit_lines(&store);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains(", acme, approved_index, approved_embed, "),
        "canonical spelling in the line: {}",
        lines[0]
    );
    assert!(
        !lines[0].contains(", ACME, "),
        "the caller casing must not appear: {}",
        lines[0]
    );
}
