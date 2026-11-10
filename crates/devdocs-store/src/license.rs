//! License state transitions, the `meta` audit trail, and the expiry
//! sweep (docs/implementation/07 §License gate).
//!
//! The store is authoritative for the mutable license state. Every
//! transition runs in one transaction — state check, state update, audit
//! line — so an illegal move leaves no trace and a legal one is never
//! half-recorded.

use rusqlite::params;
use time::OffsetDateTime;
use time::macros::format_description;

use devdocs_model::LicenseStatus;

use crate::error::StoreError;
use crate::store::Store;

/// The operator recorded on audit lines the sweep writes itself.
const AUTO_EXPIRY_OPERATOR: &str = "auto:expiry";

impl Store {
    /// Moves every source row of `library` to the license state `to` and
    /// appends one audit line to `meta`. One transaction: load the current
    /// states, check each against the transition table
    /// ([`LicenseStatus::can_transition_to`]), require a non-empty note
    /// exactly for `denied → pending_review`, update the rows, stamp
    /// `expires` on them when one is given, and write the audit line in
    /// the docs/implementation/07 format. A `None` `expires` leaves a
    /// recorded expiry untouched, so the stamp commits atomically with
    /// the move (docs/implementation/07 §License gate).
    ///
    /// An illegal move fails with [`StoreError::InvalidTransition`] and
    /// leaves the stored state unchanged with no audit line. Returns the
    /// number of rows moved. The library matches case-insensitively, and
    /// the audit line carries the canonical stored spelling of the
    /// library.
    ///
    /// # Errors
    /// [`StoreError::UnknownLibrary`] when no source row carries the
    /// library; [`StoreError::InvalidTransition`] when any current state
    /// cannot move to `to`; [`StoreError::MissingNote`] when the re-opening
    /// transition carries no note; [`StoreError::Sql`] on query failures.
    pub fn transition_license(
        &self,
        library: &str,
        to: LicenseStatus,
        operator: &str,
        note: Option<&str>,
        expires: Option<&str>,
    ) -> Result<usize, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        let states = read_license_states(&tx, library)?;
        if states.is_empty() {
            return Err(StoreError::UnknownLibrary {
                library: library.to_owned(),
            });
        }
        for from in &states {
            if !from.can_transition_to(to) {
                return Err(StoreError::InvalidTransition { from: *from, to });
            }
        }
        let note = required_note(&states, to, note)?;

        let moved = tx
            .execute(
                "UPDATE sources SET license_status = ?1, \
                 license_expires_at = COALESCE(?2, license_expires_at) \
                 WHERE library = ?3 COLLATE NOCASE",
                params![to.as_str(), expires, library],
            )
            .map_err(StoreError::Sql)?;

        // The audit line carries the canonical stored spelling of the
        // library, not the caller's casing.
        let canonical: String = tx
            .query_row(
                "SELECT library FROM sources WHERE library = ?1 COLLATE NOCASE LIMIT 1",
                params![library],
                |row| row.get(0),
            )
            .map_err(StoreError::Sql)?;

        let from = joined_states(&states);
        append_audit(&tx, &canonical, &from, to.as_str(), operator, note)?;
        tx.commit().map_err(StoreError::Sql)?;
        Ok(moved)
    }

    /// Flips every source whose recorded expiry has passed to `expired`
    /// (docs/implementation/07 §License gate: the recorded expiry passes).
    /// One transaction: rows with a non-null `license_expires_at` at or
    /// before `now` (an RFC 3339 instant) that do not already sit in a
    /// terminal refusal move to `expired`, one audit line each with the
    /// `auto:expiry` operator. Rows already `expired`, `denied`, or
    /// `revoked` stay put, and a row without an expiry never flips.
    /// Returns the number of rows flipped.
    ///
    /// # Errors
    /// [`StoreError::Sql`] on query failures.
    pub fn sweep_expired(&self, now: &str) -> Result<usize, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;

        // Fast path: the sweep is advisory, so a plain read probes for due
        // rows first and the write lock (BEGIN IMMEDIATE) is taken only
        // when work exists — an idle sweep cannot stall readers behind a
        // busy writer.
        let due_probe: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sources \
                 WHERE license_expires_at IS NOT NULL AND license_expires_at <= ?1 \
                 AND license_status NOT IN ('expired', 'denied', 'revoked'))",
                params![now],
                |row| row.get(0),
            )
            .map_err(StoreError::Sql)?;
        if !due_probe {
            return Ok(0);
        }

        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

        let mut statement = tx.prepare(
            "SELECT source_id, library, license_status FROM sources \
             WHERE license_expires_at IS NOT NULL AND license_expires_at <= ?1 \
             AND license_status NOT IN ('expired', 'denied', 'revoked')",
        )?;
        let due: Vec<(i64, String, String)> = statement
            .query_map(params![now], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(StoreError::Sql)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Sql)?;
        drop(statement);

        for (source_id, library, from) in &due {
            let Some(from) = LicenseStatus::parse(from) else {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    format!("unknown license status {from:?}").into(),
                )
                .into());
            };
            tx.execute(
                "UPDATE sources SET license_status = 'expired' WHERE source_id = ?1",
                params![source_id],
            )
            .map_err(StoreError::Sql)?;
            append_audit(
                &tx,
                library,
                from.as_str(),
                LicenseStatus::Expired.as_str(),
                AUTO_EXPIRY_OPERATOR,
                None,
            )?;
        }
        tx.commit().map_err(StoreError::Sql)?;
        Ok(due.len())
    }
}

impl Store {
    /// The distinct license states the library's rows currently carry,
    /// case-insensitive on the library, ordered by the storage spelling.
    /// An un-indexed library returns an empty vector.
    ///
    /// # Errors
    /// [`StoreError::Sql`] on query failures, or a conversion failure when
    /// a row carries an unknown status string.
    pub fn license_states(&self, library: &str) -> Result<Vec<LicenseStatus>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        read_license_states(&conn, library)
    }
}

/// The distinct license states the library's rows carry, case-insensitive
/// on the library, ordered by the storage spelling. An un-indexed library
/// returns an empty vector. Shared by the read path and the
/// transaction-typed writers — a `&Transaction` derefs to `&Connection`,
/// so both call shapes use one error-handling body
/// (docs/implementation/07 §License gate).
pub(crate) fn read_license_states(
    conn: &rusqlite::Connection,
    library: &str,
) -> Result<Vec<LicenseStatus>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT DISTINCT license_status FROM sources \
         WHERE library = ?1 COLLATE NOCASE ORDER BY license_status",
    )?;
    let rows = statement
        .query_map(params![library], |row| row.get::<_, String>(0))
        .map_err(StoreError::Sql)?;
    let mut states = Vec::new();
    for row in rows {
        let raw = row.map_err(StoreError::Sql)?;
        let Some(state) = LicenseStatus::parse(&raw) else {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("unknown license status {raw:?}").into(),
            )
            .into());
        };
        states.push(state);
    }
    Ok(states)
}

/// Validates the note requirement: `denied → pending_review` needs a
/// non-empty note for the audit trail; every other transition needs none
/// but may carry one. Returns the note to record.
fn required_note<'a>(
    states: &'a [LicenseStatus],
    to: LicenseStatus,
    note: Option<&'a str>,
) -> Result<Option<&'a str>, StoreError> {
    let reopens = states.contains(&LicenseStatus::Denied) && to == LicenseStatus::PendingReview;
    let trimmed = note.map(str::trim).filter(|note| !note.is_empty());
    if reopens && trimmed.is_none() {
        return Err(StoreError::MissingNote);
    }
    Ok(trimmed)
}

/// Renders the `from` side of an audit line: one state, or the distinct
/// set joined with `|` when the library's rows disagree.
fn joined_states(states: &[LicenseStatus]) -> String {
    states
        .iter()
        .map(|state| state.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

/// Appends one audit line to `meta` at the current instant. The
/// timestamp formats with a fixed 9-digit subsecond field, so the audit
/// key shape never depends on the nanosecond value
/// (docs/implementation/06 §Migrations).
fn append_audit(
    tx: &rusqlite::Transaction<'_>,
    library: &str,
    from: &str,
    to: &str,
    operator: &str,
    note: Option<&str>,
) -> Result<(), StoreError> {
    let ts = OffsetDateTime::now_utc()
        .format(&format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z"
        ))
        .map_err(StoreError::Timestamp)?;
    append_audit_at(tx, &ts, library, from, to, operator, note)
}

/// Appends one audit line to `meta`: the key is `audit:<rfc3339 nanosecond
/// timestamp>` — a same-instant collision falls back to a `-<n>` suffix —
/// and the value is the docs/implementation/07 format
/// `(audit:<ts>, library, from, to, operator[, note])`.
fn append_audit_at(
    tx: &rusqlite::Transaction<'_>,
    ts: &str,
    library: &str,
    from: &str,
    to: &str,
    operator: &str,
    note: Option<&str>,
) -> Result<(), StoreError> {
    let base_key = format!("audit:{ts}");
    let mut key = base_key.clone();
    let mut suffix = 0;
    loop {
        let taken: i64 = tx
            .query_row(
                "SELECT count(*) FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .map_err(StoreError::Sql)?;
        if taken == 0 {
            break;
        }
        suffix += 1;
        key = format!("{base_key}-{suffix}");
    }

    let mut value = format!("(audit:{ts}, {library}, {from}, {to}, {operator}");
    if let Some(note) = note {
        value.push_str(", ");
        value.push_str(note);
    }
    value.push(')');

    tx.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(StoreError::Sql)?;
    Ok(())
}

#[cfg(test)]
mod tests;
