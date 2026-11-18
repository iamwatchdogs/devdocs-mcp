//! The quarantine list and release: the operator surface the injection
//! filter feeds (docs/implementation/07 §Injection filter).
//!
//! A quarantined or review-flagged chunk stays stored but out of serving.
//! The list shows both states; the release clears the review state and
//! restores the serve. The release is not a license move — it never
//! touches `sources.license_status` and does not run through
//! `LicenseStatus::can_transition_to` — but it still lands in the same
//! `meta` audit trail, with `to = released` and the chunk's library in
//! the docs/implementation/07 audit format.

use rusqlite::OptionalExtension;
use rusqlite::TransactionBehavior;
use rusqlite::params;

use crate::error::StoreError;
use crate::license::append_audit;
use crate::store::Store;

/// The quarantine state of one listed chunk: the `quarantined` flag, or
/// the review-only flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineState {
    /// The chunk is quarantined and excluded from serving.
    Quarantined,
    /// The chunk serves but sits flagged for operator review.
    Review,
}

impl QuarantineState {
    /// The storage/CLI spelling of the state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quarantined => "quarantined",
            Self::Review => "review",
        }
    }
}

/// One row of the quarantine list: a chunk that carries a quarantine or
/// review flag, with the identity the operator needs to judge it.
#[derive(Debug, Clone, PartialEq)]
pub struct QuarantineRow {
    /// The chunk row (`chunks.chunk_id`).
    pub chunk_id: i64,
    /// Normalized library name of the chunk's source.
    pub library: String,
    /// Exact version of the chunk's source.
    pub version: String,
    /// Breadcrumb of section titles, read from the JSON column.
    pub section_path: Vec<String>,
    /// The comma-joined filter rules that fired; `None` when no rule was
    /// recorded.
    pub rule_names: Option<String>,
    /// The flag state the row currently carries.
    pub state: QuarantineState,
    /// Position of the chunk inside its source (`chunks.ordinal`).
    pub ordinal: i64,
    /// The linked-page URL the chunk's body came from; `None` when the
    /// chunk carries none.
    pub page_url: Option<String>,
}

impl Store {
    /// Lists every chunk that carries a quarantine or review flag —
    /// `quarantined = 1 OR review_flagged = 1` — ordered by library,
    /// version, and ordinal. A quarantined row reports
    /// [`QuarantineState::Quarantined`], a review-only row reports
    /// [`QuarantineState::Review`].
    ///
    /// # Errors
    /// [`StoreError::Sql`] on query failures, or a conversion failure
    /// when a row carries a malformed `section_path`.
    pub fn quarantine_list(&self) -> Result<Vec<QuarantineRow>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = conn.prepare(
            "SELECT c.chunk_id, s.library, s.version, c.section_path, c.rule_names, \
             c.quarantined, c.ordinal, c.page_url \
             FROM chunks c JOIN sources s ON s.source_id = c.source_id \
             WHERE c.quarantined = 1 OR c.review_flagged = 1 \
             ORDER BY s.library, s.version, c.ordinal",
        )?;
        let rows = statement.query_map([], |row| {
            let quarantined: i64 = row.get(5)?;
            Ok(QuarantineRow {
                chunk_id: row.get(0)?,
                library: row.get(1)?,
                version: row.get(2)?,
                section_path: crate::search::section_path(row, 3, row.get(0)?)?,
                rule_names: row.get(4)?,
                state: if quarantined != 0 {
                    QuarantineState::Quarantined
                } else {
                    QuarantineState::Review
                },
                ordinal: row.get(6)?,
                page_url: row.get(7)?,
            })
        })?;
        let mut listed = Vec::new();
        for row in rows {
            listed.push(row.map_err(StoreError::Sql)?);
        }
        Ok(listed)
    }

    /// Releases one flagged chunk: clears `quarantined` and
    /// `review_flagged`, nulls `rule_names`, and keeps `code_caution` —
    /// the caution is content-derived, not review state
    /// (docs/implementation/07 §Injection filter). The chunk returns to
    /// every serving read on the next query. The release does not touch
    /// the source's license state; it lands one audit line in `meta`
    /// carrying the chunk's library, the state before (`quarantined` or
    /// `review`) as `from`, `released` as `to`, and the passed operator
    /// (the CLI edge defaults to `operator:cli`). The audit note names
    /// the released chunk id, with the caller's note appended when one is
    /// given. Returns the flag state the chunk carried before the
    /// release. That state is the `from` of the audit line. The store
    /// reads it inside the same transaction that clears the flags, so no
    /// caller-side pre-scan exists and no window opens between the read
    /// and the write.
    ///
    /// # Errors
    /// [`StoreError::UnknownChunk`] when no chunk row carries `chunk_id`;
    /// [`StoreError::ChunkNotFlagged`] when the chunk holds no quarantine
    /// or review flag any more; [`StoreError::LockPoisoned`] or
    /// [`StoreError::Sql`] on failures.
    pub fn quarantine_release(
        &self,
        chunk_id: i64,
        operator: &str,
        note: Option<&str>,
    ) -> Result<QuarantineState, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let flagged: Option<(i64, i64, String)> = tx
            .query_row(
                "SELECT c.quarantined, c.review_flagged, s.library \
                 FROM chunks c JOIN sources s ON s.source_id = c.source_id \
                 WHERE c.chunk_id = ?1",
                params![chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(StoreError::Sql)?;
        let Some((quarantined, review_flagged, library)) = flagged else {
            return Err(StoreError::UnknownChunk { chunk_id });
        };
        if quarantined == 0 && review_flagged == 0 {
            return Err(StoreError::ChunkNotFlagged { chunk_id });
        }
        let from = if quarantined != 0 {
            QuarantineState::Quarantined
        } else {
            QuarantineState::Review
        };

        tx.execute(
            "UPDATE chunks SET quarantined = 0, review_flagged = 0, rule_names = NULL \
             WHERE chunk_id = ?1",
            params![chunk_id],
        )
        .map_err(StoreError::Sql)?;

        let trimmed_note = note.map(str::trim).filter(|note| !note.is_empty());
        let audit_note = match trimmed_note {
            Some(note) => format!("chunk:{chunk_id} {note}"),
            None => format!("chunk:{chunk_id}"),
        };
        append_audit(
            &tx,
            &library,
            from.as_str(),
            "released",
            operator,
            Some(&audit_note),
        )?;
        tx.commit().map_err(StoreError::Sql)?;
        Ok(from)
    }
}
