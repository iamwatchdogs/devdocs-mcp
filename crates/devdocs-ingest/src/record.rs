//! The live-response recorder (docs/implementation/07 §Live-response
//! recording): the write side of the fixture transport.
//!
//! A recording run captures every successful guard response the llms.txt
//! ingest fetches and writes one `manifest.json` — a JSON array of
//! [`RecordedResponse`] in the exact shape [`crate::fixture::FixtureSet`]
//! replays, so `add --record-to DIR` output replays through
//! `add <source> --fixture DIR`. Each entry keys on the URL the pipeline
//! requested; a redirect adds a second entry under the final URL so
//! absolute links to it replay too. An entry the run never sees — a 404 or
//! 410 probe, a failed fetch — has no manifest entry, and replay reads the
//! absence the same way (docs/implementation/07 §llms.txt adapter rule 3).
//!
//! shortcut: a redirect that moves the response's directory changes the
//! base relative links join against, and replay joins against the
//! requested URL — recording covers such sites only through their absolute
//! links; upgrade when a real gate run needs redirecting relative links.

use std::path::Path;
use std::sync::Mutex;

use url::Url;

use crate::error::IngestError;
use crate::fixture::RecordedResponse;

/// The lower-cased header name the replay path reads: the v2 link
/// relations ride the `Link:` headers (llmstxt.org v2).
const LINK_HEADER: &str = "link";

/// Collects the responses of one ingest run and writes them as a fixture
/// manifest.
#[derive(Debug, Default)]
pub struct Recorder {
    /// One entry per distinct requested URL, in fetch order.
    responses: Mutex<Vec<RecordedResponse>>,
}

impl Recorder {
    /// Assembles an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one successful guard response. The response arrives under
    /// the URL the pipeline requested; a redirect that moved it records a
    /// second entry under the final URL. A repeat fetch of the same URL
    /// replaces the earlier entry — replay serves one recorded body per
    /// URL, and the last fetch is the one a later listing saw.
    pub fn record(
        &self,
        requested: &Url,
        final_url: &Url,
        status: u16,
        link_headers: &[String],
        body: &str,
    ) {
        let mut responses = self.entries();
        Self::upsert(&mut responses, requested, status, link_headers, body);
        if final_url != requested {
            Self::upsert(&mut responses, final_url, status, link_headers, body);
        }
    }

    /// Writes `<dir>/manifest.json`: one JSON array of recorded responses,
    /// the serialization `FixtureSet::load` parses.
    ///
    /// The body lands in a sibling `manifest.json.tmp` first and renames
    /// onto the manifest, so a failed write never leaves a truncated
    /// manifest behind (atomic-enough: same-directory rename).
    ///
    /// # Errors
    /// [`IngestError::Fixture`] when the directory or the file cannot be
    /// written.
    pub fn write(&self, dir: &Path) -> Result<(), IngestError> {
        let manifest = dir.join("manifest.json");
        std::fs::create_dir_all(dir).map_err(|e| IngestError::Fixture {
            path: manifest.display().to_string(),
            message: e.to_string(),
        })?;
        let responses = self.entries();
        let json = serde_json::to_string(&*responses).map_err(|e| IngestError::Fixture {
            path: manifest.display().to_string(),
            message: e.to_string(),
        })?;
        let tmp = dir.join("manifest.json.tmp");
        std::fs::write(&tmp, json).map_err(|e| IngestError::Fixture {
            path: tmp.display().to_string(),
            message: e.to_string(),
        })?;
        std::fs::rename(&tmp, &manifest).map_err(|e| IngestError::Fixture {
            path: manifest.display().to_string(),
            message: e.to_string(),
        })?;
        Ok(())
    }

    /// Records or replaces the entry for `url`'s exact string.
    fn upsert(
        responses: &mut Vec<RecordedResponse>,
        url: &Url,
        status: u16,
        link_headers: &[String],
        body: &str,
    ) {
        let mut headers = std::collections::BTreeMap::new();
        if !link_headers.is_empty() {
            // HTTP joins same-name headers into one comma-separated list;
            // the replay's relations parser splits the value the same way.
            headers.insert(LINK_HEADER.to_owned(), link_headers.join(", "));
        }
        let response = RecordedResponse {
            url: url.as_str().to_owned(),
            status,
            headers,
            body: body.to_owned(),
        };
        match responses.iter().position(|r| r.url == response.url) {
            Some(at) => responses[at] = response,
            None => responses.push(response),
        }
    }

    /// The recorded entries. A poisoned lock means a panic while another
    /// thread held it; the entries are independent appends, so the data
    /// stays usable and the panic surfaces at the join.
    fn entries(&self) -> std::sync::MutexGuard<'_, Vec<RecordedResponse>> {
        self.responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::Recorder;
    use crate::fixture::FixtureSet;
    use url::Url;

    /// The manifest loads as a fixture set and serves by the requested
    /// URL; a Link header round-trips under its lower-cased name.
    #[test]
    fn written_manifest_replays_through_the_fixture_set() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new();
        let url = Url::parse("https://docs.acme.test/llms.txt").unwrap();
        recorder.record(
            &url,
            &url,
            200,
            &[String::from("<https://x/t.md>; rel=\"alternate\"")],
            "index",
        );
        recorder.write(dir.path()).unwrap();

        let fixture = FixtureSet::load(dir.path()).unwrap();
        let hit = fixture.serve(&url).unwrap();
        assert_eq!(hit.status, 200);
        assert_eq!(hit.body, "index");
        assert_eq!(
            hit.headers.get("link").map(String::as_str),
            Some("<https://x/t.md>; rel=\"alternate\"")
        );
        // The temp file renames away: only the manifest remains.
        assert!(!dir.path().join("manifest.json.tmp").exists());
    }

    /// A redirect records the requested URL and the final URL, both
    /// serving the same response.
    #[test]
    fn a_redirect_records_both_urls() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new();
        let requested = Url::parse("https://docs.acme.test/llms.txt").unwrap();
        let final_url = Url::parse("https://www.docs.acme.test/llms.txt").unwrap();
        recorder.record(&requested, &final_url, 200, &[], "moved");
        recorder.write(dir.path()).unwrap();

        let fixture = FixtureSet::load(dir.path()).unwrap();
        assert_eq!(fixture.serve(&requested).unwrap().body, "moved");
        assert_eq!(fixture.serve(&final_url).unwrap().body, "moved");
    }

    /// A repeat fetch replaces the earlier entry: replay serves the body
    /// the last fetch saw.
    #[test]
    fn a_refetch_replaces_the_recorded_body() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new();
        let url = Url::parse("https://docs.acme.test/page.md").unwrap();
        recorder.record(&url, &url, 200, &[], "first");
        recorder.record(&url, &url, 200, &[], "second");
        recorder.write(dir.path()).unwrap();

        let fixture = FixtureSet::load(dir.path()).unwrap();
        assert_eq!(fixture.serve(&url).unwrap().body, "second");
    }

    /// A fetch-order manifest without headers or redirects serializes
    /// compactly, one entry per recorded response.
    #[test]
    fn the_manifest_is_one_json_array_in_fetch_order() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new();
        let a = Url::parse("https://docs.acme.test/llms.txt").unwrap();
        let b = Url::parse("https://docs.acme.test/intro.md").unwrap();
        recorder.record(&a, &a, 200, &[], "index");
        recorder.record(&b, &b, 200, &[], "page");
        recorder.write(dir.path()).unwrap();

        let text = std::fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let entries = value.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["url"], "https://docs.acme.test/llms.txt");
        assert_eq!(entries[1]["url"], "https://docs.acme.test/intro.md");
    }

    /// A write into a missing directory creates it; an unwritable target
    /// fails with the fixture error.
    #[test]
    fn writes_create_the_directory_and_fail_as_fixture_errors() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let recorder = Recorder::new();
        recorder
            .write(&nested)
            .expect("the nested directory is created");
        assert!(nested.join("manifest.json").exists());

        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "text").unwrap();
        let error = recorder.write(&file).unwrap_err();
        assert!(matches!(error, crate::error::IngestError::Fixture { .. }));
    }
}
