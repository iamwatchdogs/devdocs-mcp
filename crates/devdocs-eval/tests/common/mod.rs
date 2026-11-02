//! Shared fixtures for the harness integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// The `devdocs` binary the subprocess pass needs.
pub fn devdocs_bin() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        for profile in ["debug", "release"] {
            let candidate = dir.join("target").join(profile).join("devdocs");
            if candidate.is_file() {
                return candidate;
            }
        }
        assert!(
            dir.pop(),
            "the devdocs binary is not built; run `cargo build -p devdocs` \
             (or `just test`, which builds the whole workspace)"
        );
    }
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
