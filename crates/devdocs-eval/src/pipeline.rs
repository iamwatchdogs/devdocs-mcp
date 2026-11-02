//! The Phase 0 retrieval stub (docs/implementation/14 §Harness flow).
//!
//! While M4 is in flight (docs/implementation/13), the harness runs one
//! FTS5 rung through the store: `build_match_query` over the question text,
//! `search_fts` with the documented bm25 weights, filtered to the
//! question's library. The devdocs-retrieve pipeline replaces this stub;
//! the call site and the timings stay the same.

use devdocs_store::{Store, build_match_query};

use crate::error::EvalError;
use crate::metrics::RetrievedRef;
use crate::suite::Question;

/// The result budget the harness retrieves with: the search default
/// (docs/implementation/08 §CLI contract).
const RESULT_LIMIT: u32 = 8;

/// Runs the stub retrieval for one question.
///
/// # Errors
/// Propagates store failures.
pub fn retrieve(store: &Store, question: &Question) -> Result<Vec<RetrievedRef>, EvalError> {
    let expr = build_match_query(&question.question);
    let hits = store.search_fts(&expr, RESULT_LIMIT)?;
    let refs = hits
        .iter()
        .filter(|h| h.library.eq_ignore_ascii_case(&question.library))
        .enumerate()
        .map(|(i, h)| RetrievedRef {
            rank: i + 1,
            library: h.library.clone(),
            version: h.version.clone(),
            segments: h.section_path.clone(),
            symbol: h.symbol.clone(),
        })
        .collect();
    Ok(refs)
}
