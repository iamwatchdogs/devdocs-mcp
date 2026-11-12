//! The store open, error mapping, and attribution steps both edges share
//! (docs/implementation/08 §MCP adapter).
//!
//! The CLI edge (`devdocs` commands) and the MCP adapter
//! (`devdocs-mcp`) run the same four steps around the serve layer, so
//! both edges serve identical envelopes for identical queries. The CLI
//! keeps only its own extras: the lockfile walk directory and the exit
//! codes.

use std::sync::Arc;

use devdocs_model::config::Config;
use devdocs_model::envelope::ResultEnvelope;
use devdocs_model::error::{Error, UpstreamCode};
use devdocs_store::{MigrateOptions, Store};

/// Opens the store at the configured path and runs the startup migration
/// (docs/implementation/06 §Migrations). The parent directory is created
/// first: a fresh data root must not fail the first `add` or `list`.
/// `force` passes [`MigrateOptions::force`] through; both edges run the
/// read paths with `false`, the `db migrate` command with the CLI's
/// `--force` flag.
///
/// # Errors
/// [`Error::Upstream`] when the directory cannot be created, the store
/// cannot open, or a migration fails.
pub fn open_store(config: &Config, force: bool) -> Result<Arc<Store>, Error> {
    let path = config.index.path.clone();
    if let Some(parent) = path.parent() {
        // The store's path checks expect a private data directory. Create a
        // missing one with mode 0700 in the same call — including the
        // intermediate ancestors. An existing directory keeps its mode; the
        // store warns on drift at open (docs/implementation/09 §Storage).
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|e| Error::Upstream {
                    code: UpstreamCode::UpstreamError,
                    detail: format!("cannot create the data directory {}: {e}", parent.display()),
                })?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir_all(parent).map_err(|e| Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("cannot create the data directory {}: {e}", parent.display()),
        })?;
    }
    let store = Store::open(&path).map_err(|e| map_store_error(&e))?;
    store
        .migrate_with_options(&MigrateOptions { force })
        .map_err(|e| map_store_error(&e))?;
    Ok(Arc::new(store))
}

/// Maps a store error onto the upstream code (exit 3).
#[must_use]
pub fn map_store_error(error: &devdocs_store::StoreError) -> Error {
    Error::Upstream {
        code: UpstreamCode::UpstreamError,
        detail: error.to_string(),
    }
}

/// Maps a retrieval error onto the model error
/// (docs/implementation/08 §Error codes): misses carry their
/// `miss_reason`, broken refs and cursors are usage errors, drift and
/// store failures are upstream.
#[must_use]
pub fn map_retrieve_error(error: super::RetrieveError) -> Error {
    match error {
        // defensive: a store failure after the index opens is only
        // reachable through a corrupted database, which the store-layer
        // tests cover; this arm keeps the mapping total.
        super::RetrieveError::Store(e) => map_store_error(&e),
        super::RetrieveError::Miss(reason) => Error::NoResult { reason },
        super::RetrieveError::Usage(e) => Error::Usage(e.to_string()),
        super::RetrieveError::HashMismatch(detail) => Error::Upstream {
            code: UpstreamCode::HashMismatch,
            detail,
        },
    }
}

/// Joins the declarative config attribution onto the served items by
/// library (docs/implementation/07 §License gate): the registry entry name
/// is the published library identity, so the `[sources.<name>].attribution`
/// line fills `ResultItem.attribution`. An item that already carries an
/// attribution keeps it; an item whose ref parses to no registry entry
/// stays unattributed.
pub fn attribute_results(config: &Config, envelope: &mut ResultEnvelope) {
    for item in &mut envelope.results {
        if item.attribution.is_some() {
            continue;
        }
        let Some(parsed) = devdocs_model::docref::ParsedRef::parse(&item.doc_ref) else {
            continue;
        };
        item.attribution = config
            .sources
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&parsed.library))
            .and_then(|(_, entry)| entry.attribution.clone());
    }
}
