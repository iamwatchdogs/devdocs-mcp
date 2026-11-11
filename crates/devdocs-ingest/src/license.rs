//! The fetch-time license gate (docs/implementation/07 §License gate).
//!
//! Deny by default: a source with no `[sources]` registry entry is
//! `pending_review` and cannot fetch. A registered source's mutable license
//! state lives in the store, which is authoritative
//! (docs/implementation/07 §License gate): the gate sweeps passed expiries,
//! reads the stored state, and refuses anything below `approved_fetch`, so
//! `denied`, `expired`, and `revoked` sources cannot re-sync without
//! re-approval. The configured `sources.<name>.expires` date rides the
//! grant to the first publish, which seeds it into the store row; a sync
//! whose configured date has already passed and would become a recorded
//! seed refuses at the gate. The
//! gate runs before any fetch, so a refused source costs
//! no bytes on the wire. The store's publish gate (`approved_index`) is the
//! second enforcement point
//! (docs/implementation/09 §License gate enforcement points).

use devdocs_model::config::Config;
use devdocs_model::error::{Error, UpstreamCode};
use devdocs_model::source::LicenseStatus;
use devdocs_store::Store;
use url::Url;

/// The stage a registered source with no store row syncs its first
/// generation under. Before the first publish there is no store row to
/// read, and the publish gate refuses to activate below the index stage
/// (docs/implementation/09 §License gate enforcement points, point 2), so
/// the first sync of a freshly registered entry lands exactly on that
/// floor: recording the `[sources.<name>]` entry is the operator's approval
/// decision, and every later sync reads the store row the first publish
/// created — deny, revoke, or expire it there and the gate refuses.
const FIRST_SYNC_STAGE: LicenseStatus = LicenseStatus::ApprovedIndex;

/// The fetch-time license decision for one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseGrant {
    /// The `[sources.<name>]` key; the library name for the ingest run.
    pub name: String,
    /// The recorded SPDX id, when the entry names one.
    pub license: Option<String>,
    /// The configured lapse date (`sources.<name>.expires`) the first
    /// publish stamps into the store; `None` when the entry names no date.
    pub expires: Option<time::Date>,
    /// The recorded approval stage.
    pub status: LicenseStatus,
}

/// Resolves the fetch-time license decision for the index URL.
///
/// The store is authoritative: the sweep runs first, so a source whose
/// recorded expiry has passed reads back as `expired` and refuses; then the
/// library's stored states gate the run. When the library's rows disagree,
/// the least privileged state applies, so a sync can never upgrade a
/// refused library. A registered source with no store row grants
/// the index stage (`FIRST_SYNC_STAGE`) for its first publish. A configured
/// expiry that has already passed refuses whenever it would become a
/// recorded seed — the first sync, or any sync where a row of the library
/// records no date the publish would fill. The grant carries the
/// configured date either way, so the first publish can seed it into the
/// store row.
///
/// # Errors
/// [`Error::Upstream`] with `FETCH_BLOCKED` when no registry entry matches
/// the URL (the `pending_review` deny-by-default refusal), when the
/// stored state sits below `approved_fetch`, or when the configured expiry
/// has already passed and would become a recorded seed;
/// [`Error::Upstream`] with `UPSTREAM_ERROR` when the store read fails;
/// [`Error::ConfigInvalid`] when the entry's `expires` key does not parse.
pub fn gate_fetch(store: &Store, config: &Config, url: &Url) -> Result<LicenseGrant, Error> {
    let Some((name, entry)) = find_entry(config, url)? else {
        return Err(pending_review_refusal(url));
    };

    // The sweep runs at the sync gate too: a source whose recorded expiry
    // has passed flips to `expired` here and refuses, so it cannot re-sync
    // without re-approval (docs/implementation/07 §License gate).
    store
        .sweep_expired(&devdocs_model::now_rfc3339())
        .map_err(|error| store_error(&error))?;

    let states = store
        .license_states(&name)
        .map_err(|error| store_error(&error))?;
    let expires = entry.parsed_expires(&format!("sources.{name}"))?;
    // A passed configured date refuses whenever it would become a recorded
    // seed (docs/implementation/07 §License gate); the probe decides when.
    if let Some(date) = expires.filter(|date| *date <= time::OffsetDateTime::now_utc().date())
        && configured_expiry_would_seed(store, &name, &states)?
    {
        return Err(expired_configured_expiry(url, &name, date));
    }
    let grant_status = LicenseStatus::least_privileged(states).unwrap_or(FIRST_SYNC_STAGE);
    if !grant_status.allows_fetch() {
        return Err(Error::Upstream {
            code: UpstreamCode::FetchBlocked,
            detail: format!(
                "source {url} has license status {grant_status}; fetching needs approved_fetch or higher"
            ),
        });
    }

    Ok(LicenseGrant {
        name,
        license: entry.license.clone(),
        expires,
        status: grant_status,
    })
}

/// Whether the configured expiry date would become a recorded seed this
/// sync: with no store rows the publish stamps it into the fresh row, and
/// with rows present the publish's COALESCE fills any row that records no
/// date of its own. The probe matches the seed's library-wide scope, so an
/// all-dated library keeps the store authoritative — the sweep already
/// handled every recorded date, and the inert config date cannot seed
/// (docs/implementation/07 §License gate).
///
/// # Errors
/// [`Error::Upstream`] with `UPSTREAM_ERROR` when the store read fails.
fn configured_expiry_would_seed(
    store: &Store,
    library: &str,
    states: &[LicenseStatus],
) -> Result<bool, Error> {
    if states.is_empty() {
        return Ok(true);
    }
    store
        .has_undated_rows(library)
        .map_err(|error| store_error(&error))
}

/// The refusal for a registered source whose `sources.<name>.expires` date
/// has already passed and would become a recorded seed: recording the
/// entry is the operator's approval decision, so a lapsed date refuses the
/// sync instead of seeding the passed date into a row.
fn expired_configured_expiry(url: &Url, name: &str, date: time::Date) -> Error {
    Error::Upstream {
        code: UpstreamCode::FetchBlocked,
        detail: format!(
            "source {url} has a passed configured expiry {date}; move sources.{name}.expires \
             to a future date to allow fetching"
        ),
    }
}

/// The deny-by-default refusal for a URL with no `[sources]` registry
/// entry. Exposed for the add path, which runs it before the store opens so
/// the refusal still writes no database.
#[must_use]
pub fn pending_review_refusal(url: &Url) -> Error {
    Error::Upstream {
        code: UpstreamCode::FetchBlocked,
        detail: format!(
            "source {url} is pending_review; record it under [sources.<name>] to allow fetching"
        ),
    }
}

/// The registry name behind `url`, when an entry matches. The add path
/// resolves the effective kind from it before the store opens, so a usage
/// error still writes no database.
///
/// # Errors
/// [`Error::ConfigInvalid`] when an entry carries an unparsable URL.
pub fn registry_name(config: &Config, url: &Url) -> Result<Option<String>, Error> {
    Ok(find_entry(config, url)?.map(|(name, _)| name))
}

/// The stored license state behind a gate refusal for `url`, when the URL
/// has a registry entry and the store holds rows for it. The add path uses
/// it to withdraw a source whose license turned terminal before the refusal
/// returns (docs/implementation/09 §License gate enforcement points,
/// point 3). The gate sweeps before it refuses, so the state read here is
/// post-sweep. The withdrawal is mandatory for a terminal state, so a
/// failed store read propagates instead of silently skipping it.
///
/// # Errors
/// [`Error::Upstream`] with `UPSTREAM_ERROR` when the store read fails, or
/// [`Error::ConfigInvalid`] when an entry carries an unparsable URL.
pub fn refused_status(
    store: &Store,
    config: &Config,
    url: &Url,
) -> Result<Option<LicenseStatus>, Error> {
    let Some((name, _)) = find_entry(config, url)? else {
        return Ok(None);
    };
    let states = store
        .license_states(&name)
        .map_err(|error| store_error(&error))?;
    Ok(LicenseStatus::least_privileged(states))
}

/// Maps a store failure onto the upstream code: the gate cannot decide
/// without the store, so it fails closed.
fn store_error(error: &devdocs_store::StoreError) -> Error {
    Error::Upstream {
        code: UpstreamCode::UpstreamError,
        detail: error.to_string(),
    }
}

/// Finds the registry entry whose URL equals `url` exactly.
fn find_entry<'a>(
    config: &'a Config,
    url: &Url,
) -> Result<Option<(String, &'a devdocs_model::config::SourceConfig)>, Error> {
    for (name, entry) in &config.sources {
        let Some(candidate) = entry.parsed_url(&format!("sources.{name}"))? else {
            continue;
        };
        if candidate.as_str() == url.as_str() {
            return Ok(Some((name.clone(), entry)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::BTreeMap;

    use super::{gate_fetch, pending_review_refusal, refused_status};
    use devdocs_model::config::{Config, SourceConfig};
    use devdocs_model::error::{Error, UpstreamCode};
    use devdocs_model::source::LicenseStatus;
    use devdocs_testkit::seed_source_status;
    use url::Url;

    /// A config with one registered source. The entry carries no license
    /// status: the store owns that state (docs/implementation/07 §License
    /// gate).
    fn config_with(name: &str, url: &str) -> Config {
        let mut sources = BTreeMap::new();
        sources.insert(
            String::from(name),
            SourceConfig {
                kind: Some(String::from("llms_txt")),
                url: Some(String::from(url)),
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

    /// A URL for the tests.
    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    /// A migrated store with one source row at `status` (and an optional
    /// expiry date), plus the kept-alive temp directory.
    fn store_with(
        status: &str,
        expires: Option<&str>,
    ) -> (devdocs_store::Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = seed_source_status(&dir.path().join("docs.db"), "acme", "1.0", status, expires)
            .unwrap();
        (store, dir)
    }

    /// An empty migrated store, plus the kept-alive temp directory.
    fn empty_store() -> (devdocs_store::Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = devdocs_testkit::open_migrated(&dir.path().join("docs.db")).unwrap();
        (store, dir)
    }

    /// Asserts the error is `FETCH_BLOCKED` naming the stored status.
    fn assert_fetch_blocked(error: Error, needle: &str) {
        match error {
            Error::Upstream {
                code: UpstreamCode::FetchBlocked,
                detail,
            } => assert!(detail.contains(needle), "detail: {detail}"),
            other => panic!("expected FetchBlocked, got {other:?}"),
        }
    }

    /// No registry entry denies with `FETCH_BLOCKED` — the deny-by-default
    /// contract — and the standalone refusal matches the gate's.
    #[test]
    fn missing_entry_denies_by_default() {
        let (store, _dir) = empty_store();
        let config = config_with("other", "https://docs.other.test/llms.txt");
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        assert_fetch_blocked(error, "pending_review");
        assert_eq!(
            refused_status(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap(),
            None
        );
    }

    /// A registered source with no store row has never been synced: the
    /// first sync grants the bootstrap stage, the publish gate's floor.
    #[test]
    fn registered_source_without_a_store_row_bootstraps_at_the_index_stage() {
        let (store, _dir) = empty_store();
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.name, "acme");
        assert_eq!(grant.status, LicenseStatus::ApprovedIndex);
        assert_eq!(grant.license.as_deref(), Some("MIT"));
        // Nothing to withdraw behind a refusal that never happened.
        assert_eq!(
            refused_status(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap(),
            None
        );
    }

    /// A registered source with no store row whose configured expiry has
    /// already passed refuses the first sync with `FETCH_BLOCKED`, naming
    /// the date and the `sources.<name>.expires` key: the bootstrap guard.
    #[test]
    fn a_passed_configured_date_refuses_the_first_fetch() {
        let (store, _dir) = empty_store();
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2026-01-01"));
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        match error {
            Error::Upstream {
                code: UpstreamCode::FetchBlocked,
                detail,
            } => assert!(
                detail.contains("2026-01-01") && detail.contains("sources.acme.expires"),
                "detail: {detail}"
            ),
            other => panic!("expected FetchBlocked, got {other:?}"),
        }
    }

    /// A passed configured date refuses even when the library already has
    /// an approved row: the row records no expiry of its own, so the
    /// publish's COALESCE would seed the passed date into it and the next
    /// read would sweep the source to `expired` while the sync reported
    /// success (docs/implementation/07 §License gate).
    #[test]
    fn a_passed_configured_date_refuses_a_row_without_a_recorded_expiry() {
        let (store, _dir) = store_with("approved_index", None);
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2026-01-01"));
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        match error {
            Error::Upstream {
                code: UpstreamCode::FetchBlocked,
                detail,
            } => assert!(
                detail.contains("2026-01-01") && detail.contains("sources.acme.expires"),
                "detail: {detail}"
            ),
            other => panic!("expected FetchBlocked, got {other:?}"),
        }
    }

    /// An operator-dated row grants despite a passed configured date: the
    /// row already records a date, so the config value is inert — the
    /// publish's COALESCE never clobbers it, and refusing would block a
    /// legal re-sync on a date the store outranks.
    #[test]
    fn an_operator_dated_row_grants_despite_a_passed_configured_date() {
        let (store, _dir) = store_with("approved_index", Some("2999-06-01"));
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2026-01-01"));
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.status, LicenseStatus::ApprovedIndex);

        // The store stays authoritative for the recorded date.
        let recorded: Vec<Option<String>> = store
            .list_sources()
            .unwrap()
            .into_iter()
            .map(|row| row.license_expires_at)
            .collect();
        assert_eq!(recorded, vec![Some(String::from("2999-06-01"))]);
    }

    /// One undated row among dated ones refuses a passed configured date:
    /// the probe reads the library-wide scope the seed's MAX subquery
    /// reads, so the one NULL column the publish would fill refuses the
    /// sync.
    #[test]
    fn one_undated_row_among_dated_ones_refuses_a_passed_configured_date() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        let store =
            seed_source_status(&db, "acme", "1.0", "approved_index", Some("2999-06-01")).unwrap();
        seed_source_status(&db, "acme", "2.0", "approved_index", None).unwrap();
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2026-01-01"));
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        assert_fetch_blocked(error, "2026-01-01");
    }

    /// An all-dated library grants a passed configured date: no row records
    /// a NULL expiry, so the config date cannot seed anything and the sweep
    /// already handled every recorded date.
    #[test]
    fn an_all_dated_library_grants_a_passed_configured_date() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        let store =
            seed_source_status(&db, "acme", "1.0", "approved_index", Some("2999-06-01")).unwrap();
        seed_source_status(&db, "acme", "2.0", "approved_index", Some("2999-01-01")).unwrap();
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2026-01-01"));
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.status, LicenseStatus::ApprovedIndex);
        assert_eq!(
            store.license_states("acme").unwrap(),
            vec![LicenseStatus::ApprovedIndex]
        );
    }

    /// The bootstrap grant carries the configured expiry date to the first
    /// publish, so the publish can seed it into the store row.
    #[test]
    fn the_bootstrap_grant_carries_the_configured_expiry() {
        let (store, _dir) = empty_store();
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2999-01-01"));
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.name, "acme");
        assert_eq!(grant.status, LicenseStatus::ApprovedIndex);
        assert_eq!(
            grant.expires.map(|date| date.to_string()),
            Some(String::from("2999-01-01"))
        );
    }

    /// With store rows present the store is authoritative for the status,
    /// but the grant still carries the configured date: the publish's
    /// COALESCE decides whether it seeds the row.
    #[test]
    fn an_existing_row_still_carries_the_configured_expiry_in_the_grant() {
        let (store, _dir) = store_with("approved_fetch", None);
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.get_mut("acme").unwrap().expires = Some(String::from("2999-01-01"));
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.status, LicenseStatus::ApprovedFetch);
        assert_eq!(
            grant.expires.map(|date| date.to_string()),
            Some(String::from("2999-01-01"))
        );
    }

    /// A `denied` store row refuses the sync with `FETCH_BLOCKED` (AC8).
    #[test]
    fn denied_source_refuses_sync() {
        let (store, _dir) = store_with("denied", None);
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let target = url("https://docs.acme.test/llms.txt");
        let error = gate_fetch(&store, &config, &target).unwrap_err();
        assert_fetch_blocked(error, "denied");
        assert_eq!(
            refused_status(&store, &config, &target).unwrap(),
            Some(LicenseStatus::Denied)
        );
    }

    /// A `revoked` store row refuses the sync with `FETCH_BLOCKED` (AC8),
    /// and the refusal carries the stored status for the withdrawal.
    #[test]
    fn revoked_source_refuses_sync() {
        let (store, _dir) = store_with("revoked", None);
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let target = url("https://docs.acme.test/llms.txt");
        let error = gate_fetch(&store, &config, &target).unwrap_err();
        assert_fetch_blocked(error, "revoked");
        assert_eq!(
            refused_status(&store, &config, &target).unwrap(),
            Some(LicenseStatus::Revoked)
        );
    }

    /// A store row below `approved_fetch` refuses: `pending_review` fetches
    /// nothing (docs/implementation/09 §License gate enforcement points,
    /// point 1).
    #[test]
    fn pending_review_row_refuses_sync() {
        let (store, _dir) = store_with("pending_review", None);
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        assert_fetch_blocked(error, "pending_review");
    }

    /// `approved_fetch` and every later stage fetch as before, and the
    /// grant carries the stored stage.
    #[test]
    fn approved_stages_fetch_as_before() {
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        for status in [
            "approved_fetch",
            "approved_index",
            "approved_embed",
            "approved_cache",
            "approved_snippet",
        ] {
            let (store, _dir) = store_with(status, None);
            let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt"))
                .unwrap_or_else(|e| panic!("{status}: {e}"));
            assert_eq!(grant.name, "acme", "{status}");
            assert_eq!(
                grant.status,
                LicenseStatus::parse(status).unwrap(),
                "{status}"
            );
        }
    }

    /// An expired source refuses: the sweep runs at the sync gate first, so
    /// the passed expiry flips to `expired` before the state read, and the
    /// source cannot re-sync without re-approval (docs/implementation/07
    /// §License gate).
    #[test]
    fn expired_source_refuses_after_the_sweep() {
        let (store, _dir) = store_with("approved_index", Some("2026-01-01"));
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let target = url("https://docs.acme.test/llms.txt");
        let error = gate_fetch(&store, &config, &target).unwrap_err();
        assert_fetch_blocked(error, "expired");
        // The sweep landed: the stored state is now `expired`, and the
        // refusal carries it for the withdrawal.
        assert_eq!(
            store.license_states("acme").unwrap(),
            vec![LicenseStatus::Expired]
        );
        assert_eq!(
            refused_status(&store, &config, &target).unwrap(),
            Some(LicenseStatus::Expired)
        );
    }

    /// A future expiry does not flip anything and the source fetches.
    #[test]
    fn a_future_expiry_fetches() {
        let (store, _dir) = store_with("approved_index", Some("2999-01-01"));
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.status, LicenseStatus::ApprovedIndex);
        assert_eq!(
            store.license_states("acme").unwrap(),
            vec![LicenseStatus::ApprovedIndex]
        );
    }

    /// When the library's rows disagree, the least privileged state gates:
    /// a refused row keeps the library out of the sync.
    #[test]
    fn disagreeing_rows_gate_at_the_least_privileged_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        let store = seed_source_status(&db, "acme", "1.0", "approved_snippet", None).unwrap();
        seed_source_status(&db, "acme", "2.0", "denied", None).unwrap();
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        assert_fetch_blocked(error, "denied");
    }

    /// A terminal row refuses even when a non-terminal rank-zero state ties
    /// it: `{pending_review, revoked}` reports `revoked` — the actual
    /// terminal refusal — so `withdraw_on_terminal_refusal` runs the
    /// mandatory withdrawal (docs/implementation/09 §License gate
    /// enforcement points, point 3).
    #[test]
    fn a_terminal_row_refuses_when_a_rank_zero_state_ties_it() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("docs.db");
        let store = seed_source_status(&db, "acme", "1.0", "pending_review", None).unwrap();
        seed_source_status(&db, "acme", "2.0", "revoked", None).unwrap();
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let target = url("https://docs.acme.test/llms.txt");
        let error = gate_fetch(&store, &config, &target).unwrap_err();
        assert_fetch_blocked(error, "revoked");
        // The refusal carries the terminal state, so the withdrawal runs.
        assert_eq!(
            refused_status(&store, &config, &target).unwrap(),
            Some(LicenseStatus::Revoked)
        );
    }

    /// The standalone deny-by-default refusal matches the gate's.
    #[test]
    fn pending_review_refusal_matches_the_gate_refusal() {
        let gate = pending_review_refusal(&url("https://docs.acme.test/llms.txt"));
        assert_fetch_blocked(gate, "pending_review");
    }

    /// A store read failure fails the gate closed: the fetch refuses with
    /// the upstream mapping instead of deciding on partial state
    /// (docs/implementation/09 §License gate enforcement points, point 3).
    #[test]
    fn a_store_read_failure_refuses_the_fetch_closed() {
        // A hand-corrupted `license_status` — only possible outside the
        // store's own writers, past the column's CHECK constraint — makes
        // the state read fail.
        let dir = tempfile::tempdir().unwrap();
        let store = devdocs_testkit::open_migrated_and(&dir.path().join("docs.db"), |conn| {
            devdocs_testkit::seed_source(conn, "acme", true)?;
            conn.execute_batch(
                "PRAGMA ignore_check_constraints = 1;
                 UPDATE sources SET license_status = 'bogus';
                 PRAGMA ignore_check_constraints = 0;",
            )?;
            Ok(())
        })
        .unwrap();
        let config = config_with("acme", "https://docs.acme.test/llms.txt");
        let error =
            gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap_err();
        match error {
            Error::Upstream {
                code: UpstreamCode::UpstreamError,
                detail,
            } => assert!(
                detail.contains("bogus") && detail.contains("unknown license status"),
                "detail: {detail}"
            ),
            other => panic!("expected the store-error mapping, got {other:?}"),
        }
    }

    /// A registry entry without a URL is skipped, not an error.
    #[test]
    fn entry_without_a_url_is_skipped() {
        let (store, _dir) = empty_store();
        let mut config = config_with("acme", "https://docs.acme.test/llms.txt");
        config.sources.insert(
            String::from("ghost"),
            devdocs_model::config::SourceConfig {
                kind: None,
                url: None,
                license: None,
                attribution: None,
                expires: None,
                refresh_days: None,
            },
        );
        assert_eq!(
            super::registry_name(&config, &url("https://docs.acme.test/llms.txt")).unwrap(),
            Some(String::from("acme"))
        );
        assert_eq!(
            super::registry_name(&config, &url("https://docs.other.test/llms.txt")).unwrap(),
            None
        );
        let grant = gate_fetch(&store, &config, &url("https://docs.acme.test/llms.txt")).unwrap();
        assert_eq!(grant.name, "acme");
    }
}
