//! The M3a command handlers: `add`, `list`, and `db migrate`
//! (docs/implementation/08 §CLI contract).
//!
//! Every path that opens the store runs the migration runner at startup
//! (docs/implementation/06 §Migrations). Logs go to stderr through
//! `tracing`; stdout carries only the protocol channel.

use std::sync::Arc;

use devdocs_ingest::{
    FixtureSet, GitPin, IngestJob, gate_fetch, pending_review_refusal, refused_status,
    registry_name,
};
use devdocs_model::config::{Config, SourceConfig};
use devdocs_model::envelope::{
    ResultEnvelope, SourceListEntry, SourceListEnvelope, SourceListVersion, SourceSummary,
};
use devdocs_model::error::{Error, UpstreamCode};
use devdocs_model::source::LicenseStatus;
use devdocs_retrieve::wiring;
use devdocs_store::{Store, StoreError};
use url::Url;

/// The SKILL.md content `devdocs skill install` writes, verbatim from
/// docs/implementation/08 §The skill companion.
pub const SKILL_MD: &str = concat!(
    "---\n",
    "name: devdocs\n",
    "description: Search pinned, version-exact official docs for project dependencies.\n",
    "---\n",
    "Use `devdocs` for library documentation. Workflow:\n",
    "1. `devdocs search \"<one concept>\" --json` — search; add --library/--version if known.\n",
    "2. `devdocs open <ref> --json` — read one section. Refs come only from search results.\n",
    "3. One concept per query. Stop after 3 searches; use the best result.\n",
    "4. Exit 2 = no result; run `devdocs add <source> --version <V>` if the library is missing.\n",
    "Never cite a URL not present in a result. Trust `version_status`; do not mix versions.\n",
);

/// The current directory for the resolver's lockfile walk; a vanished
/// working directory is an upstream failure, not a usage error.
fn project_dir() -> Result<std::path::PathBuf, Error> {
    // defensive: the CLI never deletes its own working directory, so the
    // arm is unreachable from a normal invocation; kept for the resolver's
    // lockfile walk.
    std::env::current_dir().map_err(|e| Error::Upstream {
        code: UpstreamCode::UpstreamError,
        detail: format!("cannot read the current directory: {e}"),
    })
}

/// Runs `devdocs search <query>` (docs/implementation/08 §CLI contract).
///
/// # Errors
/// [`Error::NoResult`] on the cold-miss paths (exit 2 with `miss_reason`),
/// [`Error::Usage`] for a broken cursor, [`Error::Upstream`] for store
/// failures.
pub fn run_search(
    config: &Config,
    query: &str,
    library: Option<&str>,
    version: Option<&str>,
    limit: Option<u32>,
    cursor: Option<u64>,
) -> Result<ResultEnvelope, Error> {
    let store = wiring::open_store(config, false)?;
    let plan = devdocs_retrieve::Plan {
        limit: limit.unwrap_or(config.search.limit_default),
        max_tokens: config.search.max_tokens_default,
        rrf_k: config.search.rrf_k,
        cursor,
    };
    let query = devdocs_retrieve::Query::new(query, library, version);
    let project = project_dir()?;
    let mut envelope = devdocs_retrieve::serve::search(&store, &query, &project, &plan)
        .map_err(wiring::map_retrieve_error)?;
    wiring::attribute_results(config, &mut envelope);
    Ok(envelope)
}

/// Splits a `devdocs symbol` target into its `(library, version)` pair:
/// `library@version` pins the version, a bare `library` keeps the library
/// scope, and a scoped `@scope/pkg` keeps its full name
/// (docs/implementation/08 §CLI contract). The split reuses the resolver's
/// `name@version` splitter so scoped pins keep the name.
///
/// # Errors
/// [`Error::Usage`] for an empty or malformed target.
fn split_symbol_target(target: &str) -> Result<(&str, Option<&str>), Error> {
    if let Some((library, version)) = devdocs_retrieve::resolver::split_name_version(target) {
        return Ok((library, Some(version)));
    }
    // An unpinned scoped name `@scope/pkg`: the splitter returns `None`
    // for a missing version, but the full name is the library.
    if let Some(rest) = target.strip_prefix('@')
        && !rest.contains('@')
        && rest.contains('/')
    {
        return Ok((target, None));
    }
    match target.split_once('@') {
        Some(_) => Err(Error::Usage(format!(
            "bad symbol target `{target}`: expected library@version"
        ))),
        None if !target.is_empty() => Ok((target, None)),
        None => Err(Error::Usage("symbol target must not be empty".to_owned())),
    }
}

/// Runs `devdocs symbol <target> <symbol>` (docs/implementation/08 §CLI
/// contract): the exact symbol lookup over the resolved target. The limit
/// is the documented search default — docs/08 defines no `--limit` flag
/// for the symbol command; `--cursor` resumes a truncated page.
///
/// # Errors
/// [`Error::NoResult`] on the miss paths (exit 2: `PKG_NOT_INDEXED` for an
/// absent target, `NO_RESULT`/`symbol_absent` for an identifier the target
/// does not carry), [`Error::Usage`] for a malformed target or cursor,
/// [`Error::Upstream`] for store failures.
pub fn run_symbol(
    config: &Config,
    target: &str,
    symbol: &str,
    cursor: Option<u64>,
) -> Result<ResultEnvelope, Error> {
    let (library, version) = split_symbol_target(target)?;
    let store = wiring::open_store(config, false)?;
    let plan = devdocs_retrieve::Plan {
        limit: config.search.limit_default,
        max_tokens: config.search.max_tokens_default,
        rrf_k: config.search.rrf_k,
        cursor,
    };
    let mut envelope = devdocs_retrieve::serve::symbol(&store, library, version, symbol, &plan)
        .map_err(wiring::map_retrieve_error)?;
    wiring::attribute_results(config, &mut envelope);
    Ok(envelope)
}

/// Runs `devdocs open <ref>`.
///
/// # Errors
/// [`Error::Usage`] for a malformed ref or cursor, [`Error::NoResult`] on
/// the cold-miss paths, [`Error::Upstream`] for store failures.
pub fn run_open(
    config: &Config,
    doc_ref: &str,
    cursor: Option<u64>,
) -> Result<ResultEnvelope, Error> {
    let store = wiring::open_store(config, false)?;
    let plan = devdocs_retrieve::Plan {
        max_tokens: config.search.max_tokens_default,
        cursor,
        ..devdocs_retrieve::Plan::defaults()
    };
    let mut envelope = devdocs_retrieve::serve::open(&store, doc_ref, &plan)
        .map_err(wiring::map_retrieve_error)?;
    wiring::attribute_results(config, &mut envelope);
    Ok(envelope)
}

/// Runs `devdocs verify <ref>`: re-derives the content hash and reports
/// `HASH_MISMATCH` on drift (docs/implementation/06 §Hashing policy).
///
/// # Errors
/// [`Error::Upstream`] with the `HASH_MISMATCH` code on drift,
/// [`Error::Usage`] for a malformed ref, [`Error::NoResult`] on the
/// cold-miss paths, [`Error::Upstream`] for store failures.
pub fn run_verify(config: &Config, doc_ref: &str) -> Result<ResultEnvelope, Error> {
    let store = wiring::open_store(config, false)?;
    let mut envelope =
        devdocs_retrieve::serve::verify(&store, doc_ref).map_err(wiring::map_retrieve_error)?;
    wiring::attribute_results(config, &mut envelope);
    Ok(envelope)
}

/// Runs `devdocs versions <library>`.
///
/// # Errors
/// [`Error::NoResult`] with `not_indexed` when the library is absent,
/// [`Error::Upstream`] for store failures.
pub fn run_versions(
    config: &Config,
    library: &str,
) -> Result<ResultEnvelope<SourceSummary>, Error> {
    let store = wiring::open_store(config, false)?;
    devdocs_retrieve::serve::versions(&store, library).map_err(wiring::map_retrieve_error)
}

/// A gate refusal carrying a terminal status (`denied`, `expired`,
/// `revoked`) withdraws the previously published generation before the
/// refusal returns, so a revoked source takes itself out of retrieval even
/// though the publish-time withdrawal never runs
/// (docs/implementation/09 §License gate enforcement points, point 3).
/// A refusal without a terminal status — no entry, no store row,
/// `pending_review` — writes nothing. The withdrawal is not best-effort:
/// a failed store read refuses the command outright, and when the store
/// cannot record the withdrawal the command returns `UPSTREAM_ERROR`
/// instead of `FETCH_BLOCKED`, so a revoked source is never left serving
/// while the command claims `FETCH_BLOCKED`.
fn withdraw_on_terminal_refusal(
    store: &Arc<Store>,
    config: &Config,
    index_url: &Url,
) -> Result<(), Error> {
    let Some(status) =
        refused_status(store, config, index_url)?.filter(|status| status.is_terminal())
    else {
        return Ok(());
    };
    let withdrawn = store
        .withdraw_source_by_origin_url(index_url.as_str(), status)
        .map_err(|error| Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("gate refused with {status} but the withdrawal failed: {error}"),
        })?;
    if withdrawn > 0 {
        tracing::info!(
            status = status.as_str(),
            withdrawn,
            "withdrew the source on the gate refusal"
        );
    }
    Ok(())
}

/// Runs `devdocs add <source>`.
///
/// Order matters: the license gate denies by default. A URL with no
/// registry entry refuses before the store opens, so a refused source costs
/// no bytes and writes no database. A registered entry consults the store —
/// the authoritative license state, swept for passed expiries first — and a
/// refusal carrying a terminal status withdraws the previously published
/// generation before it returns (docs/implementation/09 §License gate
/// enforcement points, point 3): the publish-time withdrawal cannot run,
/// because this refusal happens before any publish.
///
/// The effective kind dispatches the adapter (docs/implementation/07
/// §Priority ladder): an explicit `--kind` wins, then the config
/// `[sources.<name>].kind`, then `llms_txt`.
///
/// # Errors
/// [`Error`] for the gate refusal (`FETCH_BLOCKED`), guard refusals and
/// failures, parse failures, git failures, and store failures.
pub fn run_add(
    config: &Config,
    source: &str,
    version: Option<&str>,
    kind: Option<&str>,
    fixture_dir: Option<&std::path::Path>,
) -> Result<serde_json::Value, Error> {
    let index_url = Url::parse(source).map_err(|e| Error::Usage(format!("bad source URL: {e}")))?;

    // 1. The license gate: deny by default. The unregistered refusal runs
    //    before the store opens, and the usage validation (the effective
    //    kind) runs before the store opens too, so a rejected add still
    //    writes no database. The registered path then sweeps the store and
    //    reads its authoritative state.
    let Some(name) = registry_name(config, &index_url)? else {
        return Err(pending_review_refusal(&index_url));
    };

    // 2. The effective kind. `--kind` wins, then the config entry.
    let effective_kind = kind
        .or_else(|| {
            config
                .sources
                .get(&name)
                .and_then(|entry| entry.kind.as_deref())
        })
        .unwrap_or("llms_txt");

    // The config entry's kind is a free-form string the CLI edge never
    // validates, so an unknown kind would silently run the llms.txt
    // adapter over that URL and publish wrong content.
    if !matches!(effective_kind, "llms_txt" | "git") {
        return Err(Error::Usage(format!(
            "source kind {effective_kind} is not supported yet; this slice ingests llms_txt and git sources only"
        )));
    }

    if effective_kind == "git" {
        // `--fixture` replays recorded HTTP responses; the git adapter has
        // no replay surface, so a silent drop would run against the network.
        if fixture_dir.is_some() {
            return Err(Error::Usage(
                "--fixture replays HTTP recordings and does not apply to git sources".to_owned(),
            ));
        }
        // The pin must be explicit: there is no "latest" tag to resolve.
        let Some(pin) = version else {
            return Err(Error::Usage(
                "git sources require --version (a tag or a commit SHA)".to_owned(),
            ));
        };
        let store = wiring::open_store(config, false)?;
        let grant = match gate_fetch(&store, config, &index_url) {
            Ok(grant) => grant,
            Err(error) => {
                withdraw_on_terminal_refusal(&store, config, &index_url)?;
                return Err(error);
            }
        };
        return run_add_git(config, &store, &index_url, &grant.name, pin);
    }

    let store = wiring::open_store(config, false)?;
    let grant = match gate_fetch(&store, config, &index_url) {
        Ok(grant) => grant,
        Err(error) => {
            withdraw_on_terminal_refusal(&store, config, &index_url)?;
            return Err(error);
        }
    };

    // 3. The fixture set, when replaying.
    let fixture = fixture_dir
        .map(FixtureSet::load)
        .transpose()
        .map_err(map_ingest_error)?;

    // 4. The guard and the ingest run. The tokio runtime is local to the
    // command: the rest of the CLI stays synchronous.
    let guard = devdocs_ingest::FetchGuard::from_fetch_config(&config.fetch);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("cannot start the async runtime: {e}"),
        })?;

    let version = version.unwrap_or("latest");
    let job = IngestJob::new(config, &guard, fixture.as_ref());
    let result = runtime.block_on(job.run(Arc::clone(&store), &index_url, &grant.name, version));
    // A run that hit the total cap can leave its in-flight segmentation
    // blocking task running; do not wait for it when the runtime drops.
    runtime.shutdown_background();
    let outcome = result.map_err(map_ingest_error)?;
    add_report(&outcome)
}

/// Runs the git branch of `devdocs add`: the pin is the `--version` value
/// (40 hex → commit pin, otherwise tag pin), the clone runs inside
/// [`IngestJob::run_git`]'s `spawn_blocking` — not through the HTTP fetch
/// guard, which does not govern the git transport
/// (docs/implementation/07 §Git adapter) — and the version label is the
/// pin itself. The store handle arrives already open: the license gate
/// opened it, and a second open would migrate the same file twice.
///
/// # Errors
/// [`Error`] for any ingest failure (`IngestJob::run_git` maps the whole
/// plan/fetch/segment/publish chain).
fn run_add_git(
    config: &Config,
    store: &Arc<Store>,
    repo_url: &Url,
    library: &str,
    pin: &str,
) -> Result<serde_json::Value, Error> {
    let guard = devdocs_ingest::FetchGuard::from_fetch_config(&config.fetch);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("cannot start the async runtime: {e}"),
        })?;

    // The clone directory lands under the configured data root — not the
    // db's parent, which a custom `--db` path can point anywhere; the git
    // temp dir stays inside DEVDOCS_HOME, a private directory the adapter
    // creates with mode 0700 when missing.
    let data_root = devdocs_model::config::data_root()?;
    let job = IngestJob::new(config, &guard, None);
    let result = runtime.block_on(job.run_git(
        Arc::clone(store),
        repo_url,
        library,
        pin,
        GitPin::from(pin),
        &data_root,
    ));
    // A total-cap expiry drops the waiting future but the clone's
    // spawn_blocking closure keeps running: ask the git runner to kill the
    // in-flight child, then give the runtime a short grace so the clone
    // dir cleanup can finish instead of waiting on the dropped task.
    if result
        .as_ref()
        .err()
        .is_some_and(|error| matches!(error, devdocs_ingest::IngestError::TotalTimeout { .. }))
    {
        devdocs_ingest::git::request_shutdown();
    }
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    let outcome = result.map_err(map_ingest_error)?;
    add_report(&outcome)
}

/// The `add --json` report both adapters return; the event report also
/// goes to stderr as log lines (the default filter is `warn`, so the lines
/// surface only under `--verbose`)
/// (docs/implementation/07 §Normalization pipeline).
fn add_report(outcome: &devdocs_ingest::IngestOutcome) -> Result<serde_json::Value, Error> {
    tracing::info!(
        pages = outcome.pages,
        skipped = outcome.skipped_pages,
        chunks = outcome.chunks,
        "ingest finished"
    );
    outcome.events.log();
    // defensive: `IngestEvents` is a plain struct of integer counters, so
    // `serde_json::to_value` cannot fail on it; the mapping exists only to
    // fit the `Result`-returning call chain, never to run.
    let events = serde_json::to_value(&outcome.events).map_err(|e| Error::Upstream {
        code: UpstreamCode::UpstreamError,
        detail: format!("cannot serialize the ingest events: {e}"),
    })?;
    Ok(serde_json::json!({
        "library": outcome.library,
        "version": outcome.version,
        "origin_url": outcome.origin_url,
        "license_status": outcome.license_status.to_string(),
        "pages": outcome.pages,
        "skipped_pages": outcome.skipped_pages,
        "chunks": outcome.chunks,
        "commit_sha": outcome.commit_sha,
        "events": events,
    }))
}

/// Runs `devdocs list`.
///
/// The expiry sweep runs first, like on every read path
/// (docs/implementation/07 §Expiry sweep), so the reported states reflect
/// passed expiries.
///
/// # Errors
/// [`Error::Upstream`] when the store cannot open or the query fails.
pub fn run_list(config: &Config) -> Result<ResultEnvelope<SourceSummary>, Error> {
    let store = wiring::open_store(config, false)?;
    store
        .sweep_expired(&devdocs_model::now_rfc3339())
        .map_err(|e| wiring::map_store_error(&e))?;
    let results = store
        .list_sources()
        .map_err(|e| wiring::map_store_error(&e))?;
    Ok(ResultEnvelope {
        results,
        truncated: false,
        next_cursor: None,
        served_at: devdocs_model::envelope::now_rfc3339(),
    })
}

/// Runs `devdocs source approve <library> <stage>`
/// (docs/implementation/07 §License gate): the transition moves through
/// [`Store::transition_license`], which appends the audit line, enforces
/// the forward-only progression, and stamps `--expires` inside the same
/// transaction as the move. The typed store refusals
/// (illegal move, missing note, unknown library) surface as `Usage`
/// exit 1; the store is authoritative for the mutable state
/// (docs/implementation/07 §License gate).
///
/// # Errors
/// [`Error::Usage`] for an unknown stage, an illegal transition, a
/// missing note, or an unknown library; [`Error::Upstream`] when the
/// store fails or `--expires` does not parse as a `YYYY-MM-DD` date.
pub fn run_source_approve(
    config: &Config,
    name: &str,
    stage: &str,
    note: Option<&str>,
    operator: Option<&str>,
    expires: Option<&str>,
) -> Result<serde_json::Value, Error> {
    let to = parse_stage(stage)?;
    let expires = parse_expires(expires)?;
    let store = wiring::open_store(config, false)?;
    // The sweep runs before the transition read: a source whose expiry has
    // passed flips to the absorbing `expired` first and cannot be moved
    // forward or re-dated without the flip on the audit trail
    // (docs/implementation/07 §Expiry sweep).
    store
        .sweep_expired(&devdocs_model::now_rfc3339())
        .map_err(|e| wiring::map_store_error(&e))?;
    let from = least_privileged_state(&store, name)?;
    let rows = store
        .transition_license(
            name,
            to,
            operator.unwrap_or("anonymous"),
            note,
            expires.as_deref(),
        )
        .map_err(map_license_refusal)?;
    Ok(transition_report(
        name,
        from,
        to,
        rows,
        operator,
        note,
        expires.as_deref(),
    ))
}

/// Runs `devdocs source deny <library> --note --operator`
/// (docs/implementation/07 §License gate): one typed move onto `denied`
/// through [`Store::transition_license`].
///
/// # Errors
/// [`Error::Usage`] for an illegal transition or an unknown library;
/// [`Error::Upstream`] when the store fails.
pub fn run_source_deny(
    config: &Config,
    name: &str,
    note: Option<&str>,
    operator: Option<&str>,
) -> Result<serde_json::Value, Error> {
    let store = wiring::open_store(config, false)?;
    // The sweep runs before the transition read, same as `source approve`
    // (docs/implementation/07 §Expiry sweep).
    store
        .sweep_expired(&devdocs_model::now_rfc3339())
        .map_err(|e| wiring::map_store_error(&e))?;
    let from = least_privileged_state(&store, name)?;
    let rows = store
        .transition_license(
            name,
            LicenseStatus::Denied,
            operator.unwrap_or("anonymous"),
            note,
            None,
        )
        .map_err(map_license_refusal)?;
    Ok(transition_report(
        name,
        from,
        LicenseStatus::Denied,
        rows,
        operator,
        note,
        None,
    ))
}

/// Runs `devdocs source list`: the declarative config entries joined with
/// the store-authoritative state — url, license SPDX id, and attribution
/// from `[sources.<name>]`; status, versions, fetched-at, and the enforced
/// expiry from the store. The config `expires` only shows while the
/// library has no store rows, before anything is enforced
/// (docs/implementation/07 §License gate). The sweep runs first, so the
/// reported state reflects passed expiries (docs/implementation/07
/// §License gate).
///
/// # Errors
/// [`Error::Upstream`] when the store cannot open or the read fails.
pub fn run_source_list(config: &Config) -> Result<SourceListEnvelope, Error> {
    let store = wiring::open_store(config, false)?;
    store
        .sweep_expired(&devdocs_model::now_rfc3339())
        .map_err(|e| wiring::map_store_error(&e))?;
    let rows = store
        .list_sources()
        .map_err(|e| wiring::map_store_error(&e))?;
    Ok(SourceListEnvelope {
        results: config
            .sources
            .iter()
            .map(|(name, entry)| source_list_entry(name, entry, &rows))
            .collect(),
        served_at: devdocs_model::now_rfc3339(),
    })
}

/// Builds one `source list` entry: the declarative config fields joined
/// with the store-authoritative state of the library's rows
/// (docs/implementation/07 §License gate).
fn source_list_entry(name: &str, entry: &SourceConfig, rows: &[SourceSummary]) -> SourceListEntry {
    let library_rows: Vec<_> = rows
        .iter()
        .filter(|row| row.library.eq_ignore_ascii_case(name))
        .collect();
    let versions: Vec<SourceListVersion> = library_rows
        .iter()
        .map(|row| SourceListVersion {
            version: row.version.clone(),
            status: row.license_status.clone(),
            fetched_at: row.fetched_at.clone(),
            active: row.active,
        })
        .collect();
    let status = LicenseStatus::least_privileged(
        library_rows
            .iter()
            .filter_map(|row| LicenseStatus::parse(&row.license_status)),
    )
    .map_or_else(|| String::from("pending_review"), |state| state.to_string());
    // The earliest enforced lapse is the least privileged row; once rows
    // exist the config value no longer speaks for the store
    // (docs/implementation/07 §License gate).
    let expires = if library_rows.is_empty() {
        entry.expires.clone()
    } else {
        library_rows
            .iter()
            .filter_map(|row| row.license_expires_at.clone())
            .min()
    };
    SourceListEntry {
        name: name.to_owned(),
        url: entry.url.clone(),
        kind: entry.kind.clone(),
        license: entry.license.clone(),
        attribution: entry.attribution.clone(),
        expires,
        status,
        versions,
    }
}

/// Parses the `<stage>` argument of `devdocs source approve` through the
/// model's license-state machine vocabulary.
fn parse_stage(stage: &str) -> Result<LicenseStatus, Error> {
    LicenseStatus::parse(stage).ok_or_else(|| {
        Error::Usage(format!(
            "unknown license stage {stage:?}; expected one of \
             pending_review, approved_fetch, approved_index, approved_embed, \
             approved_cache, approved_snippet, denied, expired, revoked"
        ))
    })
}

/// Parses the `--expires` flag as a `YYYY-MM-DD` date, the same shape the
/// config `expires` key takes.
fn parse_expires(expires: Option<&str>) -> Result<Option<String>, Error> {
    expires
        .map(|raw| {
            devdocs_model::config::parse_expires_date(raw, "--expires")
                .map(|_| raw.to_owned())
                .map_err(|e| Error::Usage(e.to_string()))
        })
        .transpose()
}

/// The library's least privileged stored state, for the report's `from`.
fn least_privileged_state(store: &Store, name: &str) -> Result<LicenseStatus, Error> {
    let states = store
        .license_states(name)
        .map_err(|e| wiring::map_store_error(&e))?;
    LicenseStatus::least_privileged(states).ok_or_else(|| {
        Error::Usage(format!(
            "no indexed source for library {name:?}; run: devdocs source list"
        ))
    })
}

/// Maps the typed license-transition refusals onto `Usage` exit 1; every
/// other store failure keeps its upstream mapping.
fn map_license_refusal(error: StoreError) -> Error {
    match error {
        StoreError::InvalidTransition { .. }
        | StoreError::MissingNote
        | StoreError::UnknownLibrary { .. } => Error::Usage(error.to_string()),
        other => wiring::map_store_error(&other),
    }
}

/// The JSON report one transition prints on stdout.
#[allow(clippy::too_many_arguments)]
fn transition_report(
    name: &str,
    from: LicenseStatus,
    to: LicenseStatus,
    rows: usize,
    operator: Option<&str>,
    note: Option<&str>,
    expires: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "library": name,
        "from": from.to_string(),
        "to": to.to_string(),
        "rows": rows,
        "operator": operator.unwrap_or("anonymous"),
        "note": note,
        "expires": expires,
    })
}

/// Runs `devdocs db migrate`.
///
/// # Errors
/// [`Error::Upstream`] when the store cannot open or a migration fails;
/// a destructive pending migration without `--force` maps to the same
/// upstream code with a hint naming `--force`.
pub fn run_db_migrate(config: &Config, force: bool) -> Result<serde_json::Value, Error> {
    let store = wiring::open_store(config, force)?;
    // The startup migration inside wiring::open_store already ran; report the
    // schema state.
    let version = store
        .user_version()
        .map_err(|e| wiring::map_store_error(&e))?;
    Ok(serde_json::json!({
        "schema_version": version,
        "forced": force,
    }))
}

/// Runs `devdocs mcp --transport stdio` (docs/implementation/08 §MCP
/// adapter): serves the three MCP tools over the stdio JSON-RPC stream
/// until the client disconnects. Nothing prints on stdout — stdout carries
/// the protocol stream only; logs stay on stderr through the subscriber
/// the binary set up (.agents/instructions/observability).
///
/// # Errors
/// [`Error::Usage`] for a transport other than `stdio`; [`Error::Upstream`]
/// when the store cannot open or migrate, or the serve loop fails.
pub fn run_mcp(config: &Config, transport: &str) -> Result<(), Error> {
    // Clap validates the value, but this function is public: a caller that
    // skips clap gets a usage error instead of a silent wrong transport.
    if transport != "stdio" {
        return Err(Error::Usage(format!(
            "unsupported transport {transport}: only stdio is supported"
        )));
    }
    // The runtime is local to the command, the same shape the ingest
    // commands run their async work in: the rest of the CLI stays
    // synchronous, and no second runtime spawns inside the serve loop.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("cannot start the async runtime: {e}"),
        })?;
    let result = runtime.block_on(devdocs_mcp::serve(config));
    // The serve loop ended; the transport's blocking tasks stop with the
    // runtime without blocking the exit.
    runtime.shutdown_background();
    result
}

/// Maps an ingest error onto the CLI error model: guard refusals become
/// `FETCH_BLOCKED`, everything else `UPSTREAM_ERROR` (docs/implementation/08
/// §Error codes).
fn map_ingest_error(error: devdocs_ingest::IngestError) -> Error {
    use devdocs_ingest::IngestError as E;
    match error {
        E::Guard(guard) if guard.is_refusal() => Error::Upstream {
            code: UpstreamCode::FetchBlocked,
            detail: guard.to_string(),
        },
        E::Guard(guard) => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: guard.to_string(),
        },
        E::License(model) => model,
        E::LicenseNameMismatch { expected, actual } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!(
                "LICENSE_MISMATCH: library `{actual}` does not match the registry entry `{expected}`"
            ),
        },
        E::Store(store) => wiring::map_store_error(&store),
        E::LlmsTxtEmpty { url } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("LLMS_TXT_EMPTY: no link lines in {url}"),
        },
        E::LlmsTxtInvalid { url, reason } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("LLMS_TXT_INVALID: {reason} ({url})"),
        },
        E::Fixture { path, message } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("fixture {path}: {message}"),
        },
        E::Tokenizer { message } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("tokenizer: {message}"),
        },
        E::TotalTimeout { seconds } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("the ingest run exceeded the {seconds}s total cap"),
        },
        E::Join(message) => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("blocking task failed: {message}"),
        },
        E::NoIngestablePages { index } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("no ingestable pages for {index}"),
        },
        E::GitBinaryMissing => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: "GIT_BINARY_MISSING: the git executable was not found on PATH".to_owned(),
        },
        // An unusable git version is not a user-argument problem: the
        // adapter refuses the run the same way as a missing binary
        // (docs/implementation/09 §Git subprocess safety).
        E::GitTooOld { found } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_TOO_OLD: {found} does not meet the required git 2.32"),
        },
        // A pin starting with `-` is a user-input mistake, not an upstream
        // failure: nothing ran, so it maps onto USAGE like the sibling
        // input checks in `run_add` (docs/implementation/05 §Error model).
        E::GitInvalidPin { pin } => {
            Error::Usage(format!("GIT_INVALID_PIN: pin `{pin}` starts with `-`"))
        }
        E::GitPinNotTag { pin } => Error::Usage(format!(
            "GIT_PIN_NOT_TAG: pin `{pin}` is not a tag; use a tag name or a commit SHA"
        )),
        E::GitUrlMismatch { planned, actual } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!(
                "GIT_URL_MISMATCH: fetch URL `{actual}` does not match the planned URL `{planned}`"
            ),
        },
        E::GitCommand {
            command, stderr, ..
        } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_COMMAND_FAILED: `{command}` failed: {stderr}"),
        },
        E::GitSpawn { message } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_SPAWN_FAILED: {message}"),
        },
        E::GitTimeout { command, seconds } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!(
                "GIT_TIMEOUT: `{command}` exceeded the {seconds}s deadline and was killed"
            ),
        },
        E::GitTempDir { message } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_TEMP_DIR: {message}"),
        },
        E::GitWalk { message } => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_WALK_FAILED: {message}"),
        },
        E::GitCommitShaInvalid(sha) => Error::Upstream {
            code: UpstreamCode::UpstreamError,
            detail: format!("GIT_COMMIT_SHA_INVALID: rev-parse returned `{sha}`"),
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::{map_ingest_error, withdraw_on_terminal_refusal};
    use devdocs_ingest::IngestError;
    use devdocs_model::config::{Config, SourceConfig};
    use devdocs_model::error::{Error, UpstreamCode};
    use url::Url;

    /// A config with one registered acme entry, the shape the gate tests
    /// use.
    fn config_with_acme() -> Config {
        let mut sources = BTreeMap::new();
        sources.insert(
            String::from("acme"),
            SourceConfig {
                kind: Some(String::from("llms_txt")),
                url: Some(String::from("https://docs.acme.test/llms.txt")),
                license: Some(String::from("MIT")),
                attribution: None,
                expires: None,
                refresh_days: None,
            },
        );
        Config {
            fetch: devdocs_model::config::FetchConfig {
                concurrency: 4,
                timeout_ms: 15_000,
                max_bytes: 20 * 1024 * 1024,
                max_redirects: 3,
                user_agent: String::from("devdocs/0.2.0"),
            },
            index: devdocs_model::config::IndexConfig {
                path: std::path::PathBuf::from("/tmp/unused.db"),
                embeddings: false,
            },
            search: devdocs_model::config::SearchConfig {
                // nosemgrep: codacy.yaml.security.hard-coded-tokens
                max_tokens_default: 2000,
                limit_default: 8,
                rrf_k: 60,
            },
            ingest: devdocs_model::config::IngestConfig {
                git_extensions: devdocs_model::config::DEFAULT_GIT_EXTENSIONS
                    .iter()
                    .map(|ext| (*ext).to_owned())
                    .collect(),
            },
            sources,
        }
    }

    /// A failed withdrawal surfaces `UPSTREAM_ERROR`, not `FETCH_BLOCKED`:
    /// a terminal source is never left serving while the command claims
    /// the fetch was blocked (docs/implementation/09 §License gate
    /// enforcement points, point 3). The failure is forced with a
    /// RAISE(ABORT) trigger on `sources` — a test-only schema edit on a
    /// second connection, no production seam.
    #[test]
    fn a_failed_withdrawal_surfaces_upstream_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        let store = Arc::new(
            devdocs_testkit::seed_source_status(&db, "acme", "1.0", "denied", None).unwrap(),
        );
        let raw = rusqlite::Connection::open(&db).unwrap();
        raw.execute(
            "UPDATE sources SET origin_url = 'https://docs.acme.test/llms.txt'",
            [],
        )
        .unwrap();
        raw.execute(
            "CREATE TRIGGER block_withdraw BEFORE UPDATE ON sources \
             BEGIN SELECT RAISE(ABORT, 'test blocked'); END;",
            [],
        )
        .unwrap();

        let config = config_with_acme();
        let url = Url::parse("https://docs.acme.test/llms.txt").unwrap();
        let error = withdraw_on_terminal_refusal(&store, &config, &url).unwrap_err();
        match error {
            Error::Upstream {
                code: UpstreamCode::UpstreamError,
                detail,
            } => assert!(detail.contains("withdrawal failed"), "detail: {detail}"),
            other => panic!("expected UPSTREAM_ERROR, got {other:?}"),
        }
    }

    /// A store failure inside a license transition keeps the upstream
    /// mapping: only the typed refusals (illegal move, missing note,
    /// unknown library) read as usage errors — the rest is the store's
    /// failure, not the operator's (docs/implementation/05 §Error model).
    #[test]
    fn a_store_failure_in_a_transition_keeps_the_upstream_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        devdocs_testkit::seed_source_status(&db, "acme", "1.0", "approved_index", None).unwrap();
        let raw = rusqlite::Connection::open(&db).unwrap();
        raw.execute(
            "CREATE TRIGGER block_transition BEFORE UPDATE OF license_status ON sources \
             BEGIN SELECT RAISE(ABORT, 'test blocked'); END;",
            [],
        )
        .unwrap();
        drop(raw);

        let mut config = config_with_acme();
        config.index.path = db;
        let error = super::run_source_deny(&config, "acme", Some("robots disallow"), Some("test"))
            .unwrap_err();
        match error {
            Error::Upstream {
                code: UpstreamCode::UpstreamError,
                detail,
            } => assert!(detail.contains("test blocked"), "detail: {detail}"),
            other => panic!("expected the upstream mapping, got {other:?}"),
        }
    }

    /// The git adapter error kinds and the stable code prefixes their
    /// CLI mapping must keep (docs/implementation/05 §Error model).
    fn git_error_cases() -> Vec<(IngestError, &'static str)> {
        vec![
            (
                IngestError::GitUrlMismatch {
                    planned: "file:///a".to_owned(),
                    actual: "file:///b".to_owned(),
                },
                "GIT_URL_MISMATCH",
            ),
            (
                IngestError::GitCommand {
                    command: "git clone".to_owned(),
                    stderr: "boom".to_owned(),
                    full_stderr: "boom".to_owned(),
                },
                "GIT_COMMAND_FAILED",
            ),
            (
                IngestError::GitSpawn {
                    message: "no exec".to_owned(),
                },
                "GIT_SPAWN_FAILED",
            ),
            (
                IngestError::GitTimeout {
                    command: "git clone".to_owned(),
                    seconds: 600,
                },
                "GIT_TIMEOUT",
            ),
            (
                IngestError::GitTempDir {
                    message: "cannot create /tmp/x: denied".to_owned(),
                },
                "GIT_TEMP_DIR",
            ),
            (
                IngestError::GitWalk {
                    message: "unreadable".to_owned(),
                },
                "GIT_WALK_FAILED",
            ),
            (
                IngestError::GitCommitShaInvalid("not-a-sha".to_owned()),
                "GIT_COMMIT_SHA_INVALID",
            ),
            (
                IngestError::GitTooOld {
                    found: "git version 2.31.7".to_owned(),
                },
                "GIT_TOO_OLD",
            ),
        ]
    }

    /// Every remaining git adapter error kind maps onto `UPSTREAM_ERROR`
    /// with its stable code prefix intact (docs/implementation/05 §Error
    /// model).
    #[test]
    fn git_errors_map_upstream_with_their_code_prefix() {
        for (error, code) in git_error_cases() {
            let Error::Upstream { code: got, detail } = map_ingest_error(error) else {
                panic!("expected an upstream error for {code}");
            };
            assert_eq!(got, UpstreamCode::UpstreamError);
            assert!(
                detail.starts_with(&format!("{code}:")),
                "detail `{detail}` lacks the {code} prefix"
            );
        }
    }

    /// A pin that is not a tag is a user-input mistake: it maps onto
    /// `USAGE`, not `UPSTREAM_ERROR` (docs/implementation/05 §Error
    /// model).
    #[test]
    fn a_pin_that_is_not_a_tag_maps_to_usage() {
        let error = map_ingest_error(IngestError::GitPinNotTag {
            pin: "main".to_owned(),
        });
        let Error::Usage(detail) = error else {
            panic!("expected a usage error for a non-tag pin");
        };
        assert!(detail.starts_with("GIT_PIN_NOT_TAG:"), "detail: {detail}");
        assert!(detail.contains("is not a tag"), "detail: {detail}");
    }

    /// An option-shaped pin is a user-input mistake: it maps onto
    /// `USAGE`, not `UPSTREAM_ERROR`, with the `GIT_INVALID_PIN` detail
    /// prefix kept (docs/implementation/05 §Error model).
    #[test]
    fn an_option_shaped_pin_maps_to_usage() {
        let error = map_ingest_error(IngestError::GitInvalidPin {
            pin: "-oProxyCommand=no".to_owned(),
        });
        let Error::Usage(detail) = error else {
            panic!("expected a usage error for an option-shaped pin");
        };
        assert!(detail.starts_with("GIT_INVALID_PIN:"), "detail: {detail}");
    }
}
