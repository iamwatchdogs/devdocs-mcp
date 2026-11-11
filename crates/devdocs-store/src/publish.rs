//! Atomic generation publish.
//!
//! Contract: docs/implementation/07 §Atomic publish. On any error the
//! transaction rolls back and the previous generation stays queryable.

use rusqlite::OptionalExtension;
use rusqlite::TransactionBehavior;
use rusqlite::params;
use time::format_description::well_known::Rfc3339;

use crate::error::StoreError;
use crate::store::Store;

/// One chunk queued for publish.
#[derive(Debug, Clone, PartialEq)]
pub struct NewChunk {
    /// Breadcrumb of section titles.
    pub section_path: Vec<String>,
    /// Every symbol declared in the chunk, stored in the `symbols` table
    /// for the exact-symbol lookup ladder (docs/implementation/06
    /// §Metadata mapping). Duplicates collapse on the `(symbol, chunk_id)`
    /// primary key.
    pub symbols: Vec<Symbol>,
    /// The exact symbol this chunk documents, stored in `chunks.symbol`:
    /// the declared symbol whose name the section header names, else the
    /// first declared one.
    pub symbol: Option<Symbol>,
    /// Body with the contextual header prepended.
    pub text: String,
    /// Token count of `text`.
    pub token_count: u32,
    /// SHA-256 of the canonical body without the contextual header
    /// (docs/implementation/06 §Hashing policy); the caller strips the
    /// header before hashing.
    pub content_hash: [u8; 32],
    /// Position of the chunk inside its source, zero-based.
    pub ordinal: u32,
    /// The linked-page URL the chunk's body came from; `None` for chunks
    /// without one (docs/implementation/07 §llms.txt adapter).
    pub page_url: Option<String>,
}

/// A documented symbol and its kind, stored in `chunks.symbol` and
/// `symbols.kind` together.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// The exact symbol, e.g. `use_router`.
    pub name: String,
    /// The `symbols.kind` value, e.g. `function`.
    pub kind: String,
}

impl Store {
    /// Replaces one source's chunk set atomically.
    ///
    /// The source row is upserted by its unique key, the old chunk set is
    /// removed, the new chunks and symbols are inserted with the FTS
    /// triggers firing, and the source flips to `active` — all inside one
    /// transaction. The license gate refuses sources below the index
    /// approval stage before any write happens.
    ///
    /// A terminal license state (`denied`, `expired`, `revoked`) additionally
    /// withdraws a previously indexed source: the stored status and the
    /// `active` flip land before the gate error returns, so the source leaves
    /// search results at this enforcement point and not only at retrieve-time
    /// filtering (docs/implementation/09 §License gate enforcement points).
    ///
    /// Inside the publish transaction the STORED license state of the library
    /// is re-read, so a state that turned terminal while the caller's record
    /// was in flight (an operator deny racing a re-sync) cannot be
    /// overwritten or reactivated: the rows deactivate inside the committed
    /// transaction and the same typed refusal returns. A stored `pending_review`
    /// refuses too, with no writes at all: the operator's reopen decision
    /// stands, and so does a stored below-index state on the published row
    /// itself — a stale record must not lift an operator's lower-tier grant.
    /// Other stored non-terminal states keep the record's state, as before.
    pub fn publish_generation(
        &self,
        src: &devdocs_model::SourceRecord,
        chunks: &[NewChunk],
    ) -> Result<(), StoreError> {
        if !src.license_status.allows_index() {
            if matches!(
                src.license_status,
                devdocs_model::LicenseStatus::Denied
                    | devdocs_model::LicenseStatus::Expired
                    | devdocs_model::LicenseStatus::Revoked
            ) {
                self.withdraw_source(src)?;
            }
            return Err(StoreError::LicenseNotApproved {
                status: src.license_status,
            });
        }

        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // TOCTOU recheck: the caller-supplied record is stale when any of
        // the library's rows carries a terminal stored state after the
        // fetch gate read it. The withdrawal lands inside this committed
        // transaction, so the refusal cannot roll it back.
        let stored = crate::license::read_license_states(&tx, &src.library)?;
        if let Some(terminal) = stored.iter().copied().find(|state| state.is_terminal()) {
            tx.execute(
                "UPDATE sources SET active = 0 WHERE library = ?1 COLLATE NOCASE",
                params![src.library],
            )
            .map_err(StoreError::Sql)?;
            tx.commit().map_err(StoreError::Sql)?;
            return Err(StoreError::LicenseNotApproved { status: terminal });
        }

        // A stored `pending_review` is the operator's reopen decision: the
        // refusal writes nothing (dropping `tx` rolls back), so the stale
        // record can neither overwrite the state nor reactivate the rows.
        if stored.contains(&devdocs_model::LicenseStatus::PendingReview) {
            return Err(StoreError::LicenseNotApproved {
                status: devdocs_model::LicenseStatus::PendingReview,
            });
        }

        // A stale record must not lift an operator's below-index grant on
        // the upsert target row itself.
        if let Some(status) = stored_below_index_state(&tx, src)? {
            return Err(StoreError::LicenseNotApproved { status });
        }

        let result = publish_inside(&tx, src, chunks);
        match result {
            Ok(()) => tx.commit().map_err(StoreError::Sql),
            Err(error) => {
                // Dropping `tx` rolls back; the old generation stays active
                // and searchable.
                Err(error)
            }
        }
    }

    /// Records the terminal license state and deactivates every source
    /// published from `origin_url`, whatever its library or version. The
    /// add path calls this when the gate refuses a previously indexed
    /// source: the refusal fires before any publish, so the publish-time
    /// withdrawal cannot run. A source that was never published matches no
    /// row. One transaction: every matched row flips to `status` and
    /// `active = 0`, and each row whose status actually changes appends one
    /// `auto:withdraw` audit line (docs/implementation/07 §License gate).
    /// Returns the number of rows withdrawn — every match, changed or not.
    ///
    /// # Errors
    /// [`StoreError::LockPoisoned`] or [`StoreError::Sql`] on failures.
    pub fn withdraw_source_by_origin_url(
        &self,
        origin_url: &str,
        status: devdocs_model::LicenseStatus,
    ) -> Result<usize, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = select_withdrawal_rows(&tx, "WHERE origin_url = ?1", params![origin_url])?;
        let withdrawn = withdraw_rows(&tx, &rows, status)?;
        tx.commit().map_err(StoreError::Sql)?;
        Ok(withdrawn)
    }

    /// Records the terminal license state and deactivates the matching
    /// source row. A source that was never published matches no row. One
    /// transaction: the row flips to the terminal state and `active = 0`,
    /// and it appends one `auto:withdraw` audit line only when its status
    /// actually changes (docs/implementation/07 §License gate).
    ///
    /// # Errors
    /// [`StoreError::LockPoisoned`] or [`StoreError::Sql`] on failures.
    fn withdraw_source(&self, src: &devdocs_model::SourceRecord) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = select_withdrawal_rows(
            &tx,
            "WHERE library = ?1 COLLATE NOCASE AND version = ?2 AND kind = ?3 \
             AND origin_url = ?4",
            params![
                src.library,
                src.version,
                src.kind.as_str(),
                src.origin_url.as_str(),
            ],
        )?;
        withdraw_rows(&tx, &rows, src.license_status)?;
        tx.commit().map_err(StoreError::Sql)?;
        Ok(())
    }
}

/// Loads the rows a withdrawal matches: `(source_id, library,
/// license_status)` per row, selected with the given `WHERE` clause.
fn select_withdrawal_rows(
    tx: &rusqlite::Transaction<'_>,
    where_clause: &str,
    where_params: impl rusqlite::Params,
) -> Result<Vec<(i64, String, String)>, StoreError> {
    let mut statement = tx
        .prepare(&format!(
            "SELECT source_id, library, license_status FROM sources {where_clause}"
        ))
        .map_err(StoreError::Sql)?;
    let rows = statement
        .query_map(where_params, |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(StoreError::Sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::Sql)?;
    Ok(rows)
}

/// Flips the loaded rows to `status` and `active = 0`, appending one
/// `auto:withdraw` audit line per row whose stored status actually
/// changes — a row already in `status` deactivates silently
/// (docs/implementation/07 §License gate). Returns the number of rows
/// withdrawn: every loaded row, changed or not.
fn withdraw_rows(
    tx: &rusqlite::Transaction<'_>,
    rows: &[(i64, String, String)],
    status: devdocs_model::LicenseStatus,
) -> Result<usize, StoreError> {
    for (source_id, library, from) in rows {
        tx.execute(
            "UPDATE sources SET license_status = ?1, active = 0 WHERE source_id = ?2",
            params![status.as_str(), source_id],
        )
        .map_err(StoreError::Sql)?;
        if from.as_str() != status.as_str() {
            crate::license::append_audit(
                tx,
                library,
                from,
                status.as_str(),
                crate::license::AUTO_WITHDRAW_OPERATOR,
                None,
            )?;
        }
    }
    Ok(rows.len())
}

/// The row-scoped TOCTOU check: reads the upsert target row's stored
/// license state — keyed exactly like the publish upsert — and returns it
/// when it sits below the index stage. A record that was in flight while
/// the operator walked the row below the index stage must not overwrite
/// that lower-tier grant on publish.
fn stored_below_index_state(
    tx: &rusqlite::Transaction<'_>,
    src: &devdocs_model::SourceRecord,
) -> Result<Option<devdocs_model::LicenseStatus>, StoreError> {
    let raw: Option<String> = tx
        .query_row(
            "SELECT license_status FROM sources \
             WHERE library = ?1 COLLATE NOCASE AND version = ?2 AND kind = ?3 \
             AND origin_url = ?4",
            params![
                src.library,
                src.version,
                src.kind.as_str(),
                src.origin_url.as_str(),
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::Sql)?;
    Ok(raw
        .and_then(|raw| devdocs_model::LicenseStatus::parse(&raw))
        .filter(|state| state.rank() < devdocs_model::LicenseStatus::ApprovedIndex.rank()))
}

/// Runs the whole publish sequence inside `tx`. A new version row inherits
/// the library's latest stored `license_expires_at`, a fresh library takes
/// the configured expiry, and an operator-stamped date never clobbers
/// (docs/implementation/07 §Atomic publish).
fn publish_inside(
    tx: &rusqlite::Transaction<'_>,
    src: &devdocs_model::SourceRecord,
    chunks: &[NewChunk],
) -> Result<(), StoreError> {
    let source_id: i64 = tx
        .query_row(
            "INSERT INTO sources(library, version, kind, origin_url, commit_sha, license, \
             license_status, fetched_at, license_expires_at, active) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, \
             COALESCE((SELECT MAX(license_expires_at) FROM sources \
             WHERE library = ?1 COLLATE NOCASE), ?9), 0) \
             ON CONFLICT(library, version, kind, origin_url) DO UPDATE SET \
             commit_sha = excluded.commit_sha, license = excluded.license, \
             license_status = excluded.license_status, fetched_at = excluded.fetched_at, \
             license_expires_at = COALESCE(license_expires_at, excluded.license_expires_at), \
             active = 0 \
             RETURNING source_id",
            params![
                src.library,
                src.version,
                src.kind.as_str(),
                src.origin_url.as_str(),
                src.commit_sha,
                src.license,
                src.license_status.as_str(),
                rfc3339_utc(src.fetched_at)?,
                src.license_expires_at,
            ],
            |row| row.get(0),
        )
        .map_err(StoreError::Sql)?;

    // Deactivate the old generation: the chunk delete fires the FTS delete
    // trigger, and symbols plus session_trajectory cascade.
    tx.execute(
        "DELETE FROM chunks WHERE source_id = ?1",
        params![source_id],
    )
    .map_err(StoreError::Sql)?;

    insert_chunks(tx, source_id, chunks)?;

    tx.execute(
        "UPDATE sources SET active = 1 WHERE source_id = ?1",
        params![source_id],
    )
    .map_err(StoreError::Sql)?;
    Ok(())
}

/// Inserts the new chunk set and their symbol rows.
fn insert_chunks(
    tx: &rusqlite::Transaction<'_>,
    source_id: i64,
    chunks: &[NewChunk],
) -> Result<(), StoreError> {
    let mut insert = tx
        .prepare(
            "INSERT INTO chunks(source_id, content_hash, section_path, symbol, text, \
             token_count, ordinal, page_url) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .map_err(StoreError::Sql)?;
    for chunk in chunks {
        let section_path = serde_json::to_string(&chunk.section_path)?;
        insert
            .execute(params![
                source_id,
                &chunk.content_hash[..],
                section_path,
                chunk.symbol.as_ref().map(|sym| sym.name.as_str()),
                chunk.text,
                i64::from(chunk.token_count),
                i64::from(chunk.ordinal),
                chunk.page_url.as_deref(),
            ])
            .map_err(StoreError::Sql)?;
        let chunk_id = tx.last_insert_rowid();
        // `OR IGNORE` on the (symbol, chunk_id) primary key keeps a repeat
        // insert of the same generation a no-op.
        for sym in &chunk.symbols {
            tx.execute(
                "INSERT OR IGNORE INTO symbols(symbol, chunk_id, kind) VALUES (?1, ?2, ?3)",
                params![sym.name, chunk_id, sym.kind],
            )
            .map_err(StoreError::Sql)?;
        }
    }
    Ok(())
}

/// Formats a fetched-at instant as RFC 3339 UTC.
fn rfc3339_utc(instant: time::OffsetDateTime) -> Result<String, StoreError> {
    instant
        .replace_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)
        .map_err(StoreError::Timestamp)
}
