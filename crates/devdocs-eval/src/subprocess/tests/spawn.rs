// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use crate::subprocess::run_once_deadline;

use super::question;
#[cfg(unix)]
use super::stub;

#[test]
fn a_spawn_failure_is_a_subprocess_error() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("devdocs");
    std::fs::write(&bin, "not executable").unwrap();
    let err = run_once_deadline(
        &bin,
        &dir.path().join("fixture.db"),
        &dir.path().join("corpus.toml"),
        &dir.path().join("config-home"),
        &question(),
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(err.to_string().contains("cannot spawn"), "{err}");
}

#[cfg(unix)]
#[test]
fn a_stalled_call_is_killed_at_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), "devdocs", "#!/bin/sh\nexec sleep 30\n");
    let db = dir.path().join("fixture.db");
    let err = run_once_deadline(
        &bin,
        &db,
        &dir.path().join("corpus.toml"),
        &dir.path().join("config-home"),
        &question(),
        Duration::from_millis(100),
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("deadline"), "{message}");
    assert!(message.contains("killed"), "{message}");
}
