//! CLI-level tests through the real harness binary (the repo's `assert_cmd`
//! convention, as in crates/devdocs/tests/cli.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use assert_cmd::Command;

use common::devdocs_bin;

/// The harness command for one run of the fixture suite.
fn eval_run(suite: &str, dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_devdocs-eval"));
    cmd.arg("--json")
        .arg("run")
        .arg(suite)
        .arg("--db")
        .arg(dir.join("fixture.db"))
        .arg("--golden-dir")
        .arg(dir.join("golden"))
        .arg("--runs-dir")
        .arg(dir.join("runs"))
        .arg("--devdocs-bin")
        .arg(devdocs_bin());
    cmd
}

#[test]
fn the_cli_prints_the_gate_table_and_exits_two_on_gate_failure() {
    let dir = tempfile::tempdir().unwrap();
    common::fixture(dir.path());
    let suite = dir.path().join("fixture.jsonl");
    let output = eval_run(suite.to_str().unwrap(), dir.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["all_gates_pass"], serde_json::json!(false));
    assert_eq!(parsed["gates"].as_array().unwrap().len(), 6);
    assert_eq!(
        parsed["metrics"]["failure_rate"]["total"],
        serde_json::json!(3)
    );
}

#[test]
fn the_cli_exits_one_on_an_invalid_suite() {
    let dir = tempfile::tempdir().unwrap();
    common::fixture(dir.path());
    let suite = dir.path().join("bad.jsonl");
    std::fs::write(
        &suite,
        r#"{"id":"x","library":"next.js","version":"^1.0","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v1.0/proxy"],"grader":"ref_match","notes":"n"}"#,
    )
    .unwrap();
    let output = eval_run(suite.to_str().unwrap(), dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not an exact pin"), "{stderr}");
    assert!(output.stdout.is_empty(), "errors stay off stdout");
}
