//! Atomic generation publish.
//!
//! Contract: docs/implementation/07 §Atomic publish — an ingest run writes
//! nothing visible until it succeeds; inside one transaction the old chunk
//! set is removed, the new chunks and symbols are inserted (the FTS
//! triggers fire), and the source row flips to `active`. On any error the
//! transaction rolls back and the previous generation stays active and
//! queryable.

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
    /// Exact symbol this chunk documents, when one applies.
    pub symbol: Option<String>,
    /// The `symbols.kind` value; required together with `symbol`.
    pub symbol_kind: Option<String>,
    /// Body with the contextual header prepended.
    pub text: String,
    /// Token count of `text`.
    pub token_count: u32,
    /// SHA-256 of the canonical body (see the hashing helpers).
    pub content_hash: [u8; 32],
    /// Position of the chunk inside its source, zero-based.
    pub ordinal: u32,
}

impl Store {
    /// Replaces one source's chunk set atomically.
    ///
    /// The source row is upserted by its unique key, the old chunk set is
    /// removed, the new chunks and symbols are inserted with the FTS
    /// triggers firing, and the source flips to `active` — all inside one
    /// transaction. The license gate refuses sources below the index
    /// approval stage before any write happens.
    pub fn publish_generation(
        &self,
        src: &devdocs_model::SourceRecord,
        chunks: &[NewChunk],
    ) -> Result<(), StoreError> {
        if !src.license_status.allows_index() {
            return Err(StoreError::LicenseNotApproved {
                status: src.license_status,
            });
        }

        let mut conn = self.conn.lock().map_err(|_| StoreError::LockPoisoned)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
}

/// Runs the whole publish sequence inside `tx`.
fn publish_inside(
    tx: &rusqlite::Transaction<'_>,
    src: &devdocs_model::SourceRecord,
    chunks: &[NewChunk],
) -> Result<(), StoreError> {
    let source_id: i64 = tx
        .query_row(
            "INSERT INTO sources(library, version, kind, origin_url, commit_sha, license, \
             license_status, fetched_at, active) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0) \
             ON CONFLICT(library, version, kind, origin_url) DO UPDATE SET \
             commit_sha = excluded.commit_sha, license = excluded.license, \
             license_status = excluded.license_status, fetched_at = excluded.fetched_at, \
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
    for chunk in chunks {
        validate(chunk)?;
    }

    let mut insert = tx
        .prepare(
            "INSERT INTO chunks(source_id, content_hash, section_path, symbol, text, \
             token_count, ordinal) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(StoreError::Sql)?;
    for chunk in chunks {
        let section_path = serde_json::to_string(&chunk.section_path)
            .map_err(|source| StoreError::InvalidChunk(format!("{source}")))?;
        insert
            .execute(params![
                source_id,
                &chunk.content_hash[..],
                section_path,
                chunk.symbol,
                chunk.text,
                i64::from(chunk.token_count),
                i64::from(chunk.ordinal),
            ])
            .map_err(StoreError::Sql)?;
        if let (Some(symbol), Some(kind)) = (&chunk.symbol, &chunk.symbol_kind) {
            tx.execute(
                "INSERT INTO symbols(symbol, chunk_id, kind) VALUES (?1, ?2, ?3)",
                params![symbol, tx.last_insert_rowid(), kind],
            )
            .map_err(StoreError::Sql)?;
        }
    }
    Ok(())
}

/// Rejects chunk shapes the schema cannot represent.
fn validate(chunk: &NewChunk) -> Result<(), StoreError> {
    if chunk.symbol.is_some() != chunk.symbol_kind.is_some() {
        return Err(StoreError::InvalidChunk(format!(
            "ordinal {}: symbol and symbol_kind must be set together",
            chunk.ordinal
        )));
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
