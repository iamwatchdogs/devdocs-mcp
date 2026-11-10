#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::super::AUTO_EXPIRY_OPERATOR;
use super::{audit_lines, insert_source, state_of, store};
use devdocs_model::LicenseStatus::{ApprovedIndex, Denied, Expired, Revoked};

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

/// A due row with a corrupt status surfaces the conversion failure from
/// the sweep: the due probe filters on columns, so the transaction body
/// runs and the state parse refuses instead of silently skipping the row.
#[test]
fn a_due_corrupt_status_row_fails_the_sweep() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", ApprovedIndex, Some("2026-01-01"));
    {
        let conn = store.conn.lock().unwrap();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = 1;
             UPDATE sources SET license_status = 'bogus';
             PRAGMA ignore_check_constraints = 0;",
        )
        .unwrap();
    }
    let error = store.sweep_expired("2026-11-06T12:00:00Z").unwrap_err();
    assert!(
        matches!(
            error,
            crate::error::StoreError::Sql(rusqlite::Error::FromSqlConversionFailure(..))
        ),
        "error: {error:?}"
    );
    // The transaction rolled back: the corrupt row keeps its state, and
    // the sweep wrote no audit line.
    assert_eq!(audit_lines(&store), Vec::<String>::new());
}

/// A failed audit append fails the sweep with the SQL error instead of
/// reporting rows as flipped that were never recorded — the withdrawal
/// and audit trail stay consistent (docs/implementation/07 §License gate).
#[test]
fn a_failed_audit_append_fails_the_sweep() {
    let (store, _dir) = store();
    insert_source(&store, "acme", "1.0", ApprovedIndex, Some("2026-01-01"));
    {
        let conn = store.conn.lock().unwrap();
        conn.execute("DROP TABLE meta", []).unwrap();
    }
    let error = store.sweep_expired("2026-11-06T12:00:00Z").unwrap_err();
    assert!(
        matches!(error, crate::error::StoreError::Sql(_)),
        "error: {error:?}"
    );
    // The transaction rolled back: the row did not flip.
    assert_eq!(state_of(&store, "acme"), ApprovedIndex);
}
