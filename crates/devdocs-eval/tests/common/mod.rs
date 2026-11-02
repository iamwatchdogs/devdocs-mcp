//! Shared fixtures for the harness integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// The `devdocs` binary the subprocess pass needs.
///
/// `assert_cmd` resolves the binary the same way the repo's other CLI tests
/// do (crates/devdocs/tests/cli.rs): from the test binary's own target
/// directory, so it works under nextest, under cargo-llvm-cov's dedicated
/// target dir, and on Windows (`devdocs.exe`). It fails with a clear
/// message when the binary is not built. `cargo test -p devdocs-eval`
/// builds the `devdocs` binary as a workspace dependency of the test
/// target; a bare `cargo build -p devdocs-eval` does not, so build it
/// first (`cargo build -p devdocs`) before a run that skips the build.
pub fn devdocs_bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin("devdocs")
}

/// Builds a fixture database and a three-question suite: one answered, one
/// unanswerable that abstains, and one whose only expected ref is absent.
pub fn fixture(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let db_path = dir.join("fixture.db");
    let store = devdocs_testkit::open_migrated(&db_path).unwrap();
    drop(store);
    let conn = Connection::open(&db_path).unwrap();
    let source = devdocs_testkit::seed_source(&conn, "test-lib", true).unwrap();
    devdocs_testkit::seed_chunk(
        &conn,
        source,
        0,
        r#"["proxy"]"#,
        Some("rewrite"),
        "middleware rewrite the incoming request path",
    )
    .unwrap();
    drop(conn);

    let suite_path = dir.join("fixture.jsonl");
    let lines = [
        r#"{"id":"answered-001","library":"test-lib","version":"1.0.0","task_type":"exact_api","question":"rewrite the request path","expect_refs":["docs://test-lib/v1.0.0/proxy#rewrite"],"expect_symbols":["rewrite"],"grader":"ref_match","notes":"the seeded chunk"}"#,
        r#"{"id":"unanswerable-001","library":"test-lib","version":"1.0.0","task_type":"unanswerable","question":"quantum flux capacitor wiring","expect_refs":[],"grader":"ref_match","notes":"nothing matches"}"#,
        r#"{"id":"missed-001","library":"test-lib","version":"1.0.0","task_type":"exact_api","question":"rewrite the request path","expect_refs":["docs://test-lib/v1.0.0/absent"],"grader":"ref_match","notes":"never seeded"}"#,
    ];
    std::fs::write(&suite_path, lines.join("\n") + "\n").unwrap();
    (db_path, suite_path, dir.join("runs"))
}
