//! Shared fixtures for the harness integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::Connection;

/// The `devdocs` binary the subprocess pass needs.
///
/// The helper locates the binary beside the test binary, so it handles the
/// `deps/` directory, the `--release` profile, and the platform
/// `EXE_SUFFIX`. `cargo test -p devdocs-eval` does not build `devdocs` as a
/// side effect, so on a clean target the helper builds it once with
/// `cargo build --locked -p devdocs` and caches the path.
pub fn devdocs_bin() -> PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let (bin, release) = binary_beside_the_test();
        if !bin.is_file() {
            build_devdocs(release);
        }
        assert!(
            bin.is_file(),
            "`cargo build --locked -p devdocs` did not produce {}",
            bin.display()
        );
        bin
    })
    .clone()
}

/// The `devdocs` path that matches the test binary's target profile.
fn binary_beside_the_test() -> (PathBuf, bool) {
    let exe = std::env::current_exe().expect("the test binary has a path");
    let mut profile_dir = exe
        .parent()
        .expect("the test binary has a parent")
        .to_path_buf();
    if profile_dir.file_name().and_then(std::ffi::OsStr::to_str) == Some("deps") {
        profile_dir = profile_dir
            .parent()
            .expect("the deps directory has a parent")
            .to_path_buf();
    }
    let release = profile_dir.file_name().and_then(std::ffi::OsStr::to_str) == Some("release");
    let bin = profile_dir.join(format!("devdocs{}", std::env::consts::EXE_SUFFIX));
    (bin, release)
}

/// Builds `devdocs` into the active target directory.
fn build_devdocs(release: bool) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = std::process::Command::new(cargo);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--locked", "-p", "devdocs"]);
    if release {
        command.arg("--release");
    }
    let status = command.status().expect("cargo runs the devdocs build");
    assert!(
        status.success(),
        "`cargo build --locked -p devdocs` failed: {status}"
    );
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
