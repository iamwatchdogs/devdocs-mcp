//! End-to-end harness tests through the library API: a seeded fixture
//! database, a tiny suite, and the real `devdocs` binary for the
//! subprocess pass.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use devdocs_eval::gates::GateStatus;
use devdocs_eval::run::{RunOptions, run};

/// Runs the fixture suite once and returns the outcome with its temp dir.
fn run_fixture() -> (tempfile::TempDir, devdocs_eval::run::RunOutcome) {
    let dir = tempfile::tempdir().unwrap();
    let (db, suite, runs) = common::fixture(dir.path());
    let outcome = run(&RunOptions {
        suite: suite.display().to_string(),
        db: Some(db),
        golden_dir: Some(dir.path().join("golden")),
        runs_dir: Some(runs),
        devdocs_bin: Some(common::devdocs_bin()),
    })
    .unwrap();
    (dir, outcome)
}

#[test]
fn a_run_scores_and_writes_parseable_artifacts() {
    let (_dir, outcome) = run_fixture();

    for artifact in ["results.json", "report.md", "manifest.json"] {
        assert!(outcome.run_dir.join(artifact).is_file(), "{artifact}");
    }

    // The answered question passes; the unanswerable abstains; the absent
    // ref fails (partial counts as wrong).
    let results: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outcome.run_dir.join("results.json")).unwrap(),
    )
    .unwrap();
    let rows = results["questions"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["score"]["passed"], serde_json::json!(true));
    assert_eq!(rows[0]["score"]["hit_at_5"], serde_json::json!(true));
    assert_eq!(rows[1]["score"]["passed"], serde_json::json!(true));
    assert_eq!(rows[2]["score"]["passed"], serde_json::json!(false));
    assert_ne!(
        rows[0]["in_process"]["hits"].as_array().unwrap().len(),
        0,
        "the answered question has hits"
    );

    // The manifest reconstructs the denominator.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(outcome.run_dir.join("manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["questions"], serde_json::json!(3));
    assert_eq!(manifest["suite"], serde_json::json!("fixture"));
    assert_eq!(manifest["db_hash"].as_str().unwrap().len(), 64);
}

#[test]
fn a_run_renders_the_report_and_judges_the_gates() {
    let (_dir, outcome) = run_fixture();

    // The report carries the gate table, the breakdown, and the stub note.
    let md = std::fs::read_to_string(outcome.run_dir.join("report.md")).unwrap();
    assert!(md.contains("## Gate table"));
    assert!(md.contains("exact_api"));
    assert!(
        md.contains("not recorded"),
        "no sidecar: minutes unrecorded"
    );
    assert!(md.contains("Phase 0 stub"));

    // Deterministic gates: hit@5 is 1/2 (the absent ref misses), symbol
    // recall is 1/1, and version correctness is 2/3 (the abstention has no
    // top hit). The absent ref keeps the table honest: not all PASS.
    let status = |name: &str| {
        outcome
            .gates
            .iter()
            .find(|g| g.gate == name)
            .unwrap()
            .status
    };
    assert_eq!(status("hit@5"), GateStatus::Fail);
    assert_eq!(status("Symbol recall"), GateStatus::Pass);
    assert_eq!(status("Version correctness"), GateStatus::Fail);
    assert_eq!(status("Cost per 1,000 queries"), GateStatus::Pass);
    assert!(!outcome.all_gates_pass);
}

#[test]
fn an_invalid_suite_is_input_error_exit_one() {
    let dir = tempfile::tempdir().unwrap();
    let suite_path = dir.path().join("bad.jsonl");
    std::fs::write(
        &suite_path,
        r#"{"id":"x","library":"next.js","version":"^1.0","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v1.0/proxy"],"grader":"ref_match","notes":"n"}"#,
    )
    .unwrap();
    let (db, _suite, runs) = common::fixture(dir.path());
    let outcome = run(&RunOptions {
        suite: suite_path.display().to_string(),
        db: Some(db),
        golden_dir: Some(dir.path().join("golden")),
        runs_dir: Some(runs),
        devdocs_bin: Some(common::devdocs_bin()),
    });
    let err = outcome.unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("not an exact pin"), "{err}");
}

#[test]
fn a_manual_grader_suite_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let suite_path = dir.path().join("manual.jsonl");
    std::fs::write(
        &suite_path,
        r#"{"id":"m","library":"next.js","version":"16.2.9","task_type":"agent_task","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"grader":"manual","notes":"n"}"#,
    )
    .unwrap();
    let (db, _suite, runs) = common::fixture(dir.path());
    let outcome = run(&RunOptions {
        suite: suite_path.display().to_string(),
        db: Some(db),
        golden_dir: Some(dir.path().join("golden")),
        runs_dir: Some(runs),
        devdocs_bin: Some(common::devdocs_bin()),
    });
    let err = outcome.unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("manual grader"), "{err}");
}
