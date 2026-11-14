//! Fixture replay for git sources: a committed fixture directory stands
//! in for the clone (docs/implementation/07 §Git adapter, fixture
//! replay).
//!
//! The directory layout is the materialized docs tree plus one
//! `commit-sha` file naming the recorded commit:
//!
//! ```text
//! <fixture-dir>/
//!   commit-sha        # the recorded 40-hex commit SHA
//!   README.md         # the tree the adapter walked, at its stored
//!   docs/guide.md     #   relative layout
//! ```
//!
//! Replay validates the SHA shape, skips plan/clone/network entirely, and
//! runs the same walk/filter chain as a real git fetch, so the chunks
//! come out identical to a clone-based ingest of that tree. The
//! `commit-sha` file itself carries no allowlisted extension, so the walk
//! never ingests it.

use std::path::Path;

use crate::error::IngestError;

use super::clone::{GitFetch, is_commit_sha};
use super::walk::walk_docs_with;

/// The file naming the recorded commit, relative to the fixture dir.
const COMMIT_SHA_FILE: &str = "commit-sha";

/// Replays a git source from a fixture directory: reads and validates the
/// recorded commit SHA, then walks the materialized tree with the
/// fixture-replay walker pin (`require_git(false)`) and the same
/// extension allowlist as the clone path
/// (docs/implementation/07 §Git adapter, fixture replay).
///
/// # Errors
/// [`IngestError::Fixture`] when the `commit-sha` file is missing,
/// unreadable, or does not carry exactly 40 hex characters, and
/// [`IngestError::GitWalk`] when the walk fails at the I/O level.
pub(crate) fn replay(dir: &Path, extensions: &[String]) -> Result<GitFetch, IngestError> {
    let sha_path = dir.join(COMMIT_SHA_FILE);
    let recorded = std::fs::read_to_string(&sha_path).map_err(|e| IngestError::Fixture {
        path: sha_path.display().to_string(),
        message: format!("missing or unreadable {COMMIT_SHA_FILE} file: {e}"),
    })?;
    // The clone path normalizes commit pins to lowercase and records the
    // rev-parse output (lowercase); replay normalizes the recorded SHA the
    // same way, so both paths publish one version identity per commit.
    let recorded = recorded.trim();
    if !is_commit_sha(recorded) {
        return Err(IngestError::Fixture {
            path: sha_path.display().to_string(),
            message: format!("`{recorded}` is not a 40-hex commit SHA"),
        });
    }
    let commit_sha = recorded.to_ascii_lowercase();
    let (docs, skipped_non_utf8) = walk_docs_with(dir, extensions, false)?;
    Ok(GitFetch {
        commit_sha,
        fallback_used: false,
        docs,
        skipped_non_utf8,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use tempfile::tempdir;

    use super::super::test_util::default_extensions;
    use super::replay;
    use crate::error::IngestError;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    /// Builds a materialized fixture dir: the docs tree plus the
    /// `commit-sha` file (docs/implementation/07 §Git adapter, fixture
    /// replay).
    fn fixture_dir(dir: &std::path::Path, sha: Option<&str>) {
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("README.md"), "# docs\n\nThe index page.\n").unwrap();
        std::fs::write(dir.join("docs/guide.md"), "# Guide\n\nThe guide body.\n").unwrap();
        if let Some(sha) = sha {
            std::fs::write(dir.join("commit-sha"), format!("{sha}\n")).unwrap();
        }
    }

    /// Replay walks the materialized tree without git and records the
    /// SHA; the `commit-sha` file itself never joins the corpus.
    #[test]
    fn replay_walks_the_fixture_tree_and_records_the_sha() {
        let dir = tempdir().unwrap();
        fixture_dir(dir.path(), Some(SHA));

        let fetched = replay(dir.path(), &default_extensions()).unwrap();
        assert_eq!(fetched.commit_sha, SHA);
        assert!(!fetched.fallback_used);
        let paths: Vec<&str> = fetched.docs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths.len(), 2, "paths: {paths:?}");
        assert!(paths.contains(&"README.md"), "paths: {paths:?}");
        assert!(paths.contains(&"docs/guide.md"), "paths: {paths:?}");
    }

    /// A recorded SHA normalizes to lowercase, the same identity rule the
    /// clone path applies to commit pins.
    #[test]
    fn replay_normalizes_the_recorded_sha_to_lowercase() {
        let dir = tempdir().unwrap();
        fixture_dir(dir.path(), Some(&SHA.to_ascii_uppercase()));

        let fetched = replay(dir.path(), &default_extensions()).unwrap();
        assert_eq!(fetched.commit_sha, SHA);
    }

    /// A missing `commit-sha` file fails as `Fixture` naming the file.
    #[test]
    fn replay_refuses_a_missing_commit_sha_file() {
        let dir = tempdir().unwrap();
        fixture_dir(dir.path(), None);

        let error = replay(dir.path(), &default_extensions()).unwrap_err();
        match error {
            IngestError::Fixture { path, message } => {
                assert!(path.ends_with("commit-sha"), "path: {path}");
                assert!(message.contains("missing"), "message: {message}");
            }
            other => panic!("expected Fixture, got {other:?}"),
        }
    }

    /// A recorded value that is not exactly 40 hex fails as `Fixture`:
    /// a wrong-length value and a non-hex value both refuse.
    #[test]
    fn replay_refuses_a_malformed_commit_sha() {
        for sha in ["0123456789abcdef0123456789abcdef0123456", "v1.0.0"] {
            let dir = tempdir().unwrap();
            fixture_dir(dir.path(), Some(sha));
            let error = replay(dir.path(), &default_extensions()).unwrap_err();
            match error {
                IngestError::Fixture { path, message } => {
                    assert!(path.ends_with("commit-sha"), "path: {path}");
                    assert!(message.contains("40-hex"), "message: {message}");
                }
                other => panic!("expected Fixture for {sha}, got {other:?}"),
            }
        }
    }
}
