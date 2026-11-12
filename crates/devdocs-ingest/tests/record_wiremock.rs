//! The record → replay roundtrip: a live run over a wiremock server
//! records its responses, and the written manifest replays the same run
//! offline through the fixture set — same outcome, same chunks
//! (docs/implementation/07 §Live-response recording).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use devdocs_ingest::fixture::FixtureSet;
use devdocs_ingest::guard::FetchGuard;
use devdocs_ingest::{IngestJob, Recorder};
use devdocs_model::config::{Config, SourceConfig};
use devdocs_testkit::base_config;

/// The index: one HTML page whose markdown twin is named in a `Link`
/// header, and one markdown page. The links are relative, so they join
/// against the wiremock base URL the index came from.
const INDEX: &str = concat!(
    "# Acme\n\n> Acme widgets documentation.\n\n",
    "## Guides\n\n",
    "- [Intro](/intro.html): Start here\n",
    "- [Guide](/guide.md): Deep dive\n"
);

const INTRO: &str = concat!(
    "# Intro\n\nThe Acme SDK ships a client for every runtime.\n\n",
    "## Install\n\nRun the installer and export the key.\n"
);

const GUIDE: &str = concat!(
    "# Guide\n\nPagination follows cursors across pages.\n\n",
    "## Cursors\n\nPass the cursor to walk the whole list.\n"
);

const INTRO_TWIN_HEADER: &str = "<intro.md>; rel=\"alternate\"; type=\"text/markdown\"";

/// A config with the acme source registered at the wiremock index URL.
fn config(index_url: &str) -> Config {
    let mut config = base_config();
    config.sources.insert(
        String::from("acme"),
        SourceConfig {
            kind: Some(String::from("llms_txt")),
            url: Some(index_url.to_owned()),
            license: Some(String::from("MIT")),
            attribution: None,
            expires: None,
            refresh_days: None,
        },
    );
    config
}

/// A guard for the config with the test-double exception on: the wiremock
/// server sits on loopback, which the blocklist otherwise refuses
/// (docs/implementation/04 §Fixtures).
fn test_guard(config: &Config) -> FetchGuard {
    FetchGuard::from_fetch_config(&config.fetch).with_loopback_allowed_for_tests(true)
}

/// Mounts `path → 200 body` on the server.
async fn mount_ok(server: &MockServer, at: &str, body: &str) {
    Mock::given(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

/// Runs one ingest against the index URL and returns the outcome plus the
/// database directory, with or without a recorder attached.
async fn run_ingest(
    config: &Config,
    index_url: &str,
    fixture: Option<&FixtureSet>,
    recorder: Option<&Recorder>,
) -> (tempfile::TempDir, devdocs_ingest::IngestOutcome) {
    let db = tempfile::tempdir().unwrap();
    let store = Arc::new(devdocs_testkit::open_migrated(&db.path().join("docs.db")).unwrap());
    let guard = test_guard(config);
    let index_url = url::Url::parse(index_url).unwrap();
    let job = IngestJob::new(config, &guard, fixture);
    let job = match recorder {
        Some(recorder) => job.with_recorder(recorder),
        None => job,
    };
    let outcome = job.run(Arc::clone(&store), &index_url, "acme", "1.0").await;
    // The publish runs in a blocking task; drop the store handle after the
    // outcome so the rows are durable for the read-back assertions.
    let outcome = outcome.unwrap();
    (db, outcome)
}

/// The stored chunk fields the roundtrip compares, minus the source id,
/// which differs between databases.
#[derive(Debug, PartialEq, Eq)]
struct ChunkRow {
    library: String,
    version: String,
    section_path: String,
    text: String,
    content_hash: Vec<u8>,
    token_count: i64,
    ordinal: i64,
}

/// Reads every stored chunk of the database as a comparable row.
fn chunk_rows(db: &std::path::Path) -> Vec<ChunkRow> {
    devdocs_testkit::read_chunks(&db.join("docs.db"))
        .unwrap()
        .into_iter()
        .map(|chunk| ChunkRow {
            library: chunk.library,
            version: chunk.version,
            section_path: chunk.section_path,
            text: chunk.text,
            content_hash: chunk.content_hash,
            token_count: chunk.token_count,
            ordinal: chunk.ordinal,
        })
        .collect()
}

/// The recorded run replays offline: the manifest carries every fetched
/// response (URL, status, Link headers, body), the replay outcome equals
/// the live one, and the published chunks are byte-equal.
#[tokio::test]
async fn a_recorded_run_replays_through_the_fixture_set() {
    let server = MockServer::start().await;
    mount_ok(&server, "/llms.txt", INDEX).await;
    Mock::given(path("/intro.html"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<!doctype html><html><body>printable</body></html>")
                .append_header("Link", INTRO_TWIN_HEADER),
        )
        .mount(&server)
        .await;
    mount_ok(&server, "/intro.md", INTRO).await;
    mount_ok(&server, "/guide.md", GUIDE).await;
    // The llms-full probe stays unmounted: wiremock answers 404, the probe
    // reads absence, and the manifest records no entry for it.

    let index_url = format!("{}/llms.txt", server.uri());
    let config = config(&index_url);
    let recorder = Recorder::new();
    let (_live_db, live) = run_ingest(&config, &index_url, None, Some(&recorder)).await;
    assert_eq!(live.pages, 2, "the twin and the markdown page ingested");

    let dir = tempfile::tempdir().unwrap();
    recorder.write(dir.path()).unwrap();

    // The manifest is one JSON array; the recorded entries carry the Link
    // header, and the 404 probe has no entry to replay.
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("manifest.json")).unwrap())
            .unwrap();
    let entries = manifest.as_array().unwrap();
    let urls: Vec<String> = entries
        .iter()
        .map(|entry| entry["url"].as_str().unwrap().to_owned())
        .collect();
    let base = server.uri();
    let expected: Vec<String> = ["/llms.txt", "/intro.html", "/intro.md", "/guide.md"]
        .iter()
        .map(|tail| format!("{base}{tail}"))
        .collect();
    assert_eq!(urls, expected, "one entry per fetched URL, in fetch order");
    let intro = entries
        .iter()
        .find(|entry| entry["url"].as_str().unwrap().ends_with("/intro.html"))
        .unwrap();
    assert_eq!(
        intro["headers"]["link"], INTRO_TWIN_HEADER,
        "the twin relation rides the recorded Link header"
    );
    assert!(
        !urls.iter().any(|url| url.ends_with("llms-full.txt")),
        "an absent probe records nothing"
    );

    // The replay: the recorded manifest serves the same run offline.
    let fixture = FixtureSet::load(dir.path()).unwrap();
    let (_replay_db, replay) = run_ingest(&config, &index_url, Some(&fixture), None).await;

    assert_eq!(live, replay, "the replay reproduces the recorded run");
    assert_eq!(
        live.events.markdown_twins_used, 1,
        "the replay served the recorded Link header"
    );
}

/// The replayed chunks are byte-equal to the recorded run's chunks: same
/// text, same hashes, same breadcrumbs, same order. The index links the
/// twin-less HTML page too, which skips in both runs alike.
#[tokio::test]
async fn the_replayed_chunks_match_the_recorded_chunks() {
    let server = MockServer::start().await;
    mount_ok(&server, "/llms.txt", INDEX).await;
    mount_ok(
        &server,
        "/intro.html",
        "<!doctype html><html><body>printable</body></html>",
    )
    .await;
    mount_ok(&server, "/intro.md", INTRO).await;
    mount_ok(&server, "/guide.md", GUIDE).await;

    let index_url = format!("{}/llms.txt", server.uri());
    let config = config(&index_url);
    let recorder = Recorder::new();
    let (live_db, _live) = run_ingest(&config, &index_url, None, Some(&recorder)).await;
    let dir = tempfile::tempdir().unwrap();
    recorder.write(dir.path()).unwrap();

    let fixture = FixtureSet::load(dir.path()).unwrap();
    let (replay_db, _replay) = run_ingest(&config, &index_url, Some(&fixture), None).await;

    assert_eq!(
        chunk_rows(live_db.path()),
        chunk_rows(replay_db.path()),
        "the replay publishes byte-identical chunks"
    );
}
