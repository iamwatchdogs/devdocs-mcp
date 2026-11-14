//! End-to-end `devdocs eval run` tests through the real binary: the
//! harness exit semantics (0/1/2/3, docs/implementation/14 §Harness
//! flow) at the CLI edge, the flag passthrough, and the `--json` outcome
//! object. Exit 0 is unreachable until a real corpus exists — the
//! Retrieval-failures gate stays INCONCLUSIVE without the M4 baseline —
//! so every completed run here exits 2.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{Env, output};
use serde_json::Value;

/// Seeds the fixture database the suite runs against: one active source
/// with one chunk (section `proxy`, symbol `rewrite`) — the shape the
/// devdocs-eval integration fixtures seed.
fn seed_fixture_db(db: &Path) {
    let _store = devdocs_testkit::open_migrated(db).unwrap();
    let conn = rusqlite::Connection::open(db).unwrap();
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
    let chunk_id: i64 = conn
        .query_row("SELECT chunk_id FROM chunks LIMIT 1", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO symbols(symbol, chunk_id, kind) VALUES ('rewrite', ?1, 'function')",
        [chunk_id],
    )
    .unwrap();
}

/// Writes the three-question fixture suite into the env's golden dir:
/// one answered, one abstaining unanswerable, one whose only expected ref
/// is absent — so hit@5 fails and the run cannot pass.
fn write_suite(env: &Env, name: &str) {
    let golden = env.root.join("golden");
    std::fs::create_dir_all(&golden).unwrap();
    let lines = [
        r#"{"id":"answered-001","library":"test-lib","version":"1.0.0","task_type":"exact_api","question":"rewrite()","expect_refs":["docs://test-lib/v1.0.0/proxy#rewrite"],"expect_symbols":["rewrite"],"grader":"ref_match","notes":"the seeded chunk"}"#,
        r#"{"id":"unanswerable-001","library":"test-lib","version":"1.0.0","task_type":"unanswerable","question":"quantum flux capacitor wiring","expect_refs":[],"grader":"ref_match","notes":"nothing matches"}"#,
        r#"{"id":"missed-001","library":"test-lib","version":"1.0.0","task_type":"exact_api","question":"rewrite the request path","expect_refs":["docs://test-lib/v1.0.0/absent"],"grader":"ref_match","notes":"never seeded"}"#,
    ];
    std::fs::write(
        golden.join(format!("{name}.jsonl")),
        lines.join("\n") + "\n",
    )
    .unwrap();
}

/// The args for one `devdocs eval run`: the harness flags as docs/08
/// lists them; `--db` arrives on the global flag layer, `runs` overrides
/// the default artifacts dir.
fn eval_args(env: &Env, json: bool, suite: &str, db: Option<&Path>, runs: &Path) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if json {
        args.push(String::from("--json"));
    }
    args.extend([String::from("eval"), String::from("run"), suite.to_owned()]);
    if let Some(db) = db {
        args.push(String::from("--db"));
        args.push(db.display().to_string());
    }
    for (flag, value) in [
        ("--golden-dir", env.root.join("golden")),
        ("--runs-dir", runs.to_path_buf()),
        (
            "--devdocs-bin",
            PathBuf::from(env!("CARGO_BIN_EXE_devdocs")),
        ),
    ] {
        args.push(String::from(flag));
        args.push(value.display().to_string());
    }
    args
}

/// Runs one eval with the default artifacts dir.
fn run(env: &Env, json: bool, suite: &str, db: Option<&Path>) -> (i32, String, String) {
    run_with_runs(env, json, suite, db, &env.root.join("runs"))
}

/// Runs one eval with an explicit artifacts dir.
fn run_with_runs(
    env: &Env,
    json: bool,
    suite: &str,
    db: Option<&Path>,
    runs: &Path,
) -> (i32, String, String) {
    let args = eval_args(env, json, suite, db, runs);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    output(env.cmd(), &refs)
}

/// The last stderr line: the error envelope. Logs share stderr
/// (.agents/instructions/observability), so the envelope print — the
/// CLI edge's last act — is the final line, not the whole stream.
fn envelope_line(stderr: &str) -> &str {
    stderr.trim().lines().last().unwrap_or("")
}

/// A completed run whose gates fail exits 2 with the `EVAL_GATE` envelope
/// on stderr and the outcome object on stdout; the INCONCLUSIVE
/// Retrieval-failures row is a block too (docs/implementation/14 §Harness
/// flow).
#[test]
fn failing_gates_exit_two_with_the_eval_gate_envelope() {
    let env = Env::new();
    let db = env.root.join("fixture.db");
    seed_fixture_db(&db);
    write_suite(&env, "fixture");
    let (code, stdout, stderr) = run(&env, true, "fixture", Some(&db));
    assert_eq!(code, 2, "stdout: {stdout} stderr: {stderr}");

    let outcome: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(outcome["all_gates_pass"], serde_json::json!(false));
    let gates = outcome["gates"].as_array().unwrap();
    assert_eq!(gates.len(), 6, "{gates:?}");

    // The error envelope shape (docs/05 §Error model): exactly error,
    // hint, detail — the gate names travel in the detail.
    let envelope: Value = serde_json::from_str(envelope_line(&stderr))
        .unwrap_or_else(|e| panic!("stderr envelope: {e}\nstderr: {stderr}"));
    assert_eq!(envelope["error"], serde_json::json!("EVAL_GATE"));
    assert!(envelope["hint"].as_str().unwrap().contains("run report"));
    let detail = envelope["detail"].as_str().unwrap();
    assert!(
        detail.contains("Retrieval failures (INCONCLUSIVE)"),
        "{detail}"
    );
    assert!(envelope.get("miss_reason").is_none());
    assert!(envelope.get("key_path").is_none());
}

/// A malformed suite file is harness invalid input: exit 1 with the
/// `USAGE` envelope, and nothing on stdout.
#[test]
fn a_malformed_suite_is_a_usage_error_exit_one() {
    let env = Env::new();
    let db = env.root.join("fixture.db");
    seed_fixture_db(&db);
    let golden = env.root.join("golden");
    std::fs::create_dir_all(&golden).unwrap();
    std::fs::write(
        golden.join("bad.jsonl"),
        r#"{"id":"x","library":"next.js","version":"^1.0","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v1.0/proxy"],"grader":"ref_match","notes":"n"}"#,
    )
    .unwrap();
    let (code, stdout, stderr) = run(&env, false, "bad", Some(&db));
    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stdout.is_empty(), "stdout stays clean on error: {stdout}");
    let envelope: Value = serde_json::from_str(envelope_line(&stderr)).unwrap();
    assert_eq!(envelope["error"], serde_json::json!("USAGE"));
    assert!(
        envelope["detail"]
            .as_str()
            .unwrap()
            .contains("not an exact pin"),
        "{envelope}"
    );
}

/// A suite file the harness cannot read is an operational failure:
/// exit 3 with the `UPSTREAM_ERROR` envelope.
#[test]
fn a_missing_suite_file_is_an_upstream_error_exit_three() {
    let env = Env::new();
    // No golden dir, no suite: the harness read fails before any work.
    let (code, stdout, stderr) = run(&env, false, "fixture", None);
    assert_eq!(code, 3, "stderr: {stderr}");
    assert!(stdout.is_empty(), "stdout stays clean on error: {stdout}");
    let envelope: Value = serde_json::from_str(envelope_line(&stderr)).unwrap();
    assert_eq!(envelope["error"], serde_json::json!("UPSTREAM_ERROR"));
    assert!(
        envelope["detail"].as_str().unwrap().contains("cannot read"),
        "{envelope}"
    );
}

/// The flags pass through to the harness: artifacts land in
/// `--runs-dir`, and `--db` is honored — the corpus in the artifacts
/// names the source seeded into that database file.
#[test]
fn the_flags_pass_through_to_the_harness() {
    let env = Env::new();
    let db = env.root.join("seeded").join("fixture.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    seed_fixture_db(&db);
    write_suite(&env, "fixture");
    let runs = env.root.join("out").join("runs");
    let (code, stdout, stderr) = run_with_runs(&env, true, "fixture", Some(&db), &runs);
    assert_eq!(code, 2, "stdout: {stdout} stderr: {stderr}");

    let outcome: Value = serde_json::from_str(stdout.trim()).unwrap();
    let run_dir = PathBuf::from(outcome["run_dir"].as_str().unwrap());
    assert!(
        run_dir.starts_with(&runs),
        "artifacts in --runs-dir: {run_dir:?}"
    );
    let results: Value =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("results.json")).unwrap())
            .unwrap();
    let corpus = results["corpus"].as_array().unwrap();
    assert_eq!(corpus.len(), 1, "the seeded database was used: {corpus:?}");
    assert_eq!(corpus[0]["library"], serde_json::json!("test-lib"));
}

/// Without `--json`, the gate table and the run verdict print to stdout,
/// and the exit stays 2 while a gate blocks.
#[test]
fn the_plain_run_prints_the_gate_table_and_exits_two() {
    let env = Env::new();
    let db = env.root.join("fixture.db");
    seed_fixture_db(&db);
    write_suite(&env, "fixture");
    let (code, stdout, stderr) = run(&env, false, "fixture", Some(&db));
    assert_eq!(code, 2, "stdout: {stdout} stderr: {stderr}");
    assert!(
        stdout.contains("| Gate | Measured | Threshold | Phase | Status |"),
        "{stdout}"
    );
    assert!(stdout.contains("hit@5"), "{stdout}");
    assert!(stdout.contains("gate failure"), "{stdout}");
    assert!(!stdout.contains("all gates PASS"), "{stdout}");
}

/// A stub `devdocs` whose every invocation records its argument vector,
/// one argument per line. The harness accepts exit 0 as a valid timed
/// call, so the subprocess pass runs to completion against the stub.
#[cfg(unix)]
fn argv_recorder(dir: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let args_file = dir.join(format!("{name}.args"));
    let bin = dir.join(name);
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 0\n",
            args_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    args_file
}

/// The global `--config` flag reaches the harness and the spawned
/// `devdocs search`: the subprocess argument vector carries `--config`
/// with the flag's path (docs/implementation/14 §Regression gate — the
/// spawned search must not grade against the operator's user config).
#[test]
#[cfg(unix)]
fn the_global_config_flag_reaches_the_subprocess_spawn() {
    let env = Env::new();
    let db = env.root.join("fixture.db");
    seed_fixture_db(&db);
    write_suite(&env, "fixture");
    let config = env.root.join("probe.toml");
    std::fs::write(
        &config,
        "[fetch]\nuser_agent = \"eval-config-passthrough-probe/0.1\"\n",
    )
    .unwrap();
    let args_file = argv_recorder(&env.root, "devdocs-stub");

    let mut args: Vec<String> = vec![
        String::from("eval"),
        String::from("run"),
        String::from("fixture"),
        String::from("--db"),
        db.display().to_string(),
        String::from("--config"),
        config.display().to_string(),
        String::from("--golden-dir"),
        env.root.join("golden").display().to_string(),
        String::from("--runs-dir"),
        env.root.join("runs").display().to_string(),
        String::from("--devdocs-bin"),
        env.root.join("devdocs-stub").display().to_string(),
    ];
    args.insert(0, String::from("--json"));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, stdout, stderr) = output(env.cmd(), &refs);
    let _ = (code, stdout, stderr);

    let spawn_args = std::fs::read_to_string(&args_file).unwrap();
    assert!(spawn_args.contains("--config\n"), "{spawn_args}");
    assert!(
        spawn_args.contains(config.to_str().unwrap()),
        "{spawn_args}"
    );
}
