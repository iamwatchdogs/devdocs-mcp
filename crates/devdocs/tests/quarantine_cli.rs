//! The `db quarantine` operator surface end to end: the `--json` list and
//! release reports, the chunk serving again after a release, the exit-1
//! refusal envelopes, and the terminal re-escape on the human render
//! paths (docs/implementation/07 §Injection filter, docs/implementation/
//! 08 §CLI contract). Chunks seed directly through the testkit, bypassing
//! ingest, so the flags and the planted control bytes arrive verbatim.

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Env, output};
use serde_json::Value;

/// The raw ESC byte the escape tests plant in stored content.
const ESC: char = '\u{1b}';

/// The C1 control character the escape tests plant in stored content:
/// U+009B, the 8-bit CSI introducer terminals honor.
const CSI8: char = '\u{9b}';

/// A fresh environment with the database seeded at `db` and the `--db`
/// flag wired: the config layers stay empty, the flag points the store.
/// The chunk ids are the insert order — 1, 2, … on a fresh file.
fn seeded_env(
    seed: impl FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>,
) -> (Env, String) {
    let env = Env::new();
    // The store expects a private 0700 data directory; a drifted mode
    // logs a WARN line onto stderr ahead of the JSON envelope.
    let data = env.root.join("data");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir(&data).expect("data dir");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))
            .expect("data dir mode");
    }
    #[cfg(not(unix))]
    std::fs::create_dir(&data).expect("data dir");
    let db = data.join("docs.db");
    devdocs_testkit::open_migrated_and(&db, seed).expect("the seed lands");
    (env, db.to_string_lossy().into_owned())
}

/// One active `acme` source (the ingest bootstrap floor, which serves
/// metadata) with the given `(flags, text)` chunks under `section_path`
/// (the column's JSON array form).
fn acme_source(
    conn: &rusqlite::Connection,
    section_path: &str,
    rows: &[(devdocs_testkit::ChunkFlags<'_>, &str)],
) {
    let source = devdocs_testkit::seed_source(conn, "acme", true).expect("source seeded");
    for (index, (flags, text)) in rows.iter().enumerate() {
        devdocs_testkit::seed_flagged_chunk(
            conn,
            source,
            i64::try_from(index).unwrap_or(0),
            section_path,
            None,
            &format!("# Guide\n\n{text}"),
            *flags,
        )
        .expect("chunk seeded");
    }
}

/// `db quarantine list --json` reports every flagged chunk with the
/// operator identity fields: chunk id, library, version, section path,
/// rule names, state, ordinal, and page URL — a clean chunk never lists.
#[test]
fn quarantine_list_reports_the_flagged_rows() {
    let (env, db) = seeded_env(|conn| {
        acme_source(
            conn,
            "[\"Guide\"]",
            &[
                (devdocs_testkit::ChunkFlags::default(), "clean chunk"),
                (
                    devdocs_testkit::ChunkFlags {
                        quarantined: true,
                        rule_names: Some("injection,prompt_leak"),
                        ..devdocs_testkit::ChunkFlags::default()
                    },
                    "held chunk",
                ),
                (
                    devdocs_testkit::ChunkFlags {
                        review_flagged: true,
                        ..devdocs_testkit::ChunkFlags::default()
                    },
                    "review chunk",
                ),
            ],
        );
        Ok(())
    });
    let (code, stdout, stderr) = output(
        env.cmd(),
        &["db", "quarantine", "list", "--json", "--db", &db],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let report: Value = serde_json::from_str(stdout.trim()).expect("stdout is JSON");
    let rows = report["results"].as_array().expect("results array");
    assert_eq!(rows.len(), 2, "report: {report}");
    assert_eq!(report["count"], 2);
    assert_eq!(rows[0]["chunk_id"], 2);
    assert_eq!(rows[0]["state"], "quarantined");
    assert_eq!(rows[0]["library"], "acme");
    assert_eq!(rows[0]["version"], "1.0.0");
    assert_eq!(
        rows[0]["section_path"],
        Value::Array(vec![Value::from("Guide")])
    );
    assert_eq!(rows[0]["rule_names"], "injection,prompt_leak");
    assert_eq!(rows[0]["ordinal"], 1);
    assert_eq!(rows[0]["page_url"], Value::Null);
    assert_eq!(rows[1]["chunk_id"], 3);
    assert_eq!(rows[1]["state"], "review");
    assert_eq!(rows[1]["rule_names"], Value::Null);
}

/// `db quarantine release <id> --json` reports the transition and the
/// chunk serves again: the search that missed with `quarantined_only`
/// before the release serves one result after it.
#[test]
fn quarantine_release_restores_serving_and_reports() {
    let (env, db) = seeded_env(|conn| {
        acme_source(
            conn,
            "[\"Guide\"]",
            &[(
                devdocs_testkit::ChunkFlags {
                    quarantined: true,
                    rule_names: Some("injection"),
                    ..devdocs_testkit::ChunkFlags::default()
                },
                "The routing layer maps requests.",
            )],
        );
        Ok(())
    });

    // Before the release the query misses with the quarantined reason.
    let (code, stdout, stderr) = output(env.cmd(), &["search", "routing", "--json", "--db", &db]);
    assert_eq!(code, 2, "stdout: {stdout} stderr: {stderr}");
    let error: Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|e| panic!("stderr: {stderr:?} parse: {e}"));
    assert_eq!(error["miss_reason"], "quarantined_only");

    // The release reports the transition with the documented fields.
    let (code, stdout, stderr) = output(
        env.cmd(),
        &[
            "db",
            "quarantine",
            "release",
            "1",
            "--note",
            "reviewed, safe",
            "--json",
            "--db",
            &db,
        ],
    );
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let report: Value = serde_json::from_str(stdout.trim()).expect("stdout is JSON");
    assert_eq!(report["chunk_id"], 1);
    assert_eq!(report["from"], "quarantined");
    assert_eq!(report["to"], "released");
    assert_eq!(report["operator"], "operator:cli");
    assert_eq!(report["note"], "reviewed, safe");

    // The chunk serves again.
    let (code, stdout, stderr) = output(env.cmd(), &["search", "routing", "--json", "--db", &db]);
    assert_eq!(code, 0, "stdout: {stdout} stderr: {stderr}");
    let envelope: Value = serde_json::from_str(stdout.trim()).expect("envelope");
    assert_eq!(envelope["results"].as_array().expect("results").len(), 1);
}

/// Releasing an unknown chunk id exits 1 with the documented USAGE
/// envelope on stderr (the same mapping the license refusals carry).
#[test]
fn releasing_an_unknown_chunk_exits_one() {
    let (env, db) = seeded_env(|conn| {
        acme_source(
            conn,
            "[\"Guide\"]",
            &[(devdocs_testkit::ChunkFlags::default(), "clean chunk")],
        );
        Ok(())
    });
    let (code, stdout, stderr) = output(
        env.cmd(),
        &["db", "quarantine", "release", "999", "--json", "--db", &db],
    );
    assert_eq!(code, 1, "stdout: {stdout} stderr: {stderr}");
    assert_eq!(stdout, "", "no report prints on a refusal");
    let error: Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|e| panic!("stderr: {stderr:?} parse: {e}"));
    assert_eq!(error["error"], "USAGE");
    assert!(
        error["detail"].as_str().unwrap_or_default().contains("999"),
        "stderr: {stderr}"
    );
}

/// Releasing a chunk that holds no flag exits 1: the chunk was never
/// quarantined, so there is nothing to release.
#[test]
fn releasing_an_unflagged_chunk_exits_one() {
    let (env, db) = seeded_env(|conn| {
        acme_source(
            conn,
            "[\"Guide\"]",
            &[(devdocs_testkit::ChunkFlags::default(), "clean chunk")],
        );
        Ok(())
    });
    let (code, _stdout, stderr) = output(
        env.cmd(),
        &["db", "quarantine", "release", "1", "--json", "--db", &db],
    );
    assert_eq!(code, 1, "stderr: {stderr}");
    let error: Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|e| panic!("stderr: {stderr:?} parse: {e}"));
    assert_eq!(error["error"], "USAGE");
    assert!(
        error["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("carries no quarantine or review flag"),
        "stderr: {stderr}"
    );
}

/// The human render paths re-escape the control sequences stored content
/// can plant: the quarantine list and the open output print the literal
/// `\u{1b}` text, never the raw ESC byte. The `--json` path escapes the
/// byte through `serde_json`, so the raw byte never reaches stdout there
/// either (docs/implementation/09 §Secrets and redaction).
#[test]
fn human_renders_escape_the_planted_control_sequences() {
    let text = format!("Run {ESC}[31mdanger{ESC}[0m now.");
    let (env, db) = seeded_env(|conn| {
        // The ESC rides in the section title (the list render prints it)
        // and in the body (the open page prints it). The column is a JSON
        // array, so the escape lands through serde_json's \u001b form.
        let section = serde_json::to_string(&vec![format!("Guide {ESC}[31mred")]).expect("json");
        acme_source(
            conn,
            &section,
            &[(
                devdocs_testkit::ChunkFlags {
                    quarantined: true,
                    rule_names: Some("injection"),
                    ..devdocs_testkit::ChunkFlags::default()
                },
                &text,
            )],
        );
        Ok(())
    });
    devdocs_testkit::approve_source_to(
        std::path::Path::new(&db),
        "acme",
        devdocs_model::LicenseStatus::ApprovedSnippet,
        "test:bootstrap",
    )
    .expect("acme reaches the snippet stage");

    // Human quarantine list: escaped section path, no raw ESC byte.
    let (code, stdout, stderr) = output(env.cmd(), &["db", "quarantine", "list", "--db", &db]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stdout.contains(ESC),
        "the raw ESC byte must not reach stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("\\u{1b}"),
        "the escape must render as literal text: {stdout:?}"
    );

    // The open path reads only served chunks, so the release runs first.
    let (code, _stdout, stderr) = output(
        env.cmd(),
        &["db", "quarantine", "release", "1", "--db", &db],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    // Human open output: the served page renders escaped too.
    let (code, stdout, stderr) = output(
        env.cmd(),
        &["open", "docs://acme/v1.0.0/guide-31mred", "--db", &db],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stdout.contains(ESC),
        "the raw ESC byte must not reach stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("\\u{1b}"),
        "the escape must render as literal text: {stdout:?}"
    );

    // The JSON path carries the byte only in serde_json's escaped form.
    let (code, stdout, _stderr) = output(
        env.cmd(),
        &[
            "open",
            "docs://acme/v1.0.0/guide-31mred",
            "--json",
            "--db",
            &db,
        ],
    );
    assert_eq!(code, 0);
    assert!(!stdout.contains(ESC), "stdout: {stdout:?}");
    assert!(stdout.contains("\\u001b"), "stdout: {stdout:?}");
}

/// The C1 control range (`U+0080`-`U+009F`) escapes on both render paths
/// too: `serde_json` does not escape C1, so the JSON path rewrites the
/// serialized line, and the human path renders the literal `\u{9b}`
/// spelling. A planted U+009B never reaches stdout raw, so it cannot act
/// as an 8-bit CSI introducer in a terminal
/// (docs/implementation/09 §Secrets and redaction).
#[test]
fn human_and_json_renders_escape_the_planted_c1_controls() {
    let (env, db) = seeded_env(|conn| {
        // The C1 char rides in the section title (the list render prints
        // it) and in the body (the open page prints it).
        let section = serde_json::to_string(&vec![format!("Guide {CSI8}31mred")]).expect("json");
        acme_source(
            conn,
            &section,
            &[(
                devdocs_testkit::ChunkFlags {
                    quarantined: true,
                    rule_names: Some("injection"),
                    ..devdocs_testkit::ChunkFlags::default()
                },
                &format!("Run {CSI8}31mdanger{CSI8}0m now."),
            )],
        );
        Ok(())
    });
    devdocs_testkit::approve_source_to(
        std::path::Path::new(&db),
        "acme",
        devdocs_model::LicenseStatus::ApprovedSnippet,
        "test:bootstrap",
    )
    .expect("acme reaches the snippet stage");

    // Human quarantine list: the raw C1 byte never reaches stdout.
    let (code, stdout, stderr) = output(env.cmd(), &["db", "quarantine", "list", "--db", &db]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stdout.contains(CSI8),
        "the raw C1 byte must not reach stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("\\u{9b}"),
        "the C1 char must render as literal text: {stdout:?}"
    );

    // The open path reads only served chunks, so the release runs first.
    let (code, _stdout, stderr) = output(
        env.cmd(),
        &["db", "quarantine", "release", "1", "--db", &db],
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    // Human open output: the served page renders escaped too.
    let (code, stdout, stderr) = output(
        env.cmd(),
        &["open", "docs://acme/v1.0.0/guide-31mred", "--db", &db],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stdout.contains(CSI8),
        "the raw C1 byte must not reach stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("\\u{9b}"),
        "the C1 char must render as literal text: {stdout:?}"
    );

    // The JSON path carries the C1 char only in serde_json's unicode
    // escape form.
    let (code, stdout, _stderr) = output(
        env.cmd(),
        &[
            "open",
            "docs://acme/v1.0.0/guide-31mred",
            "--json",
            "--db",
            &db,
        ],
    );
    assert_eq!(code, 0);
    assert!(!stdout.contains(CSI8), "stdout: {stdout:?}");
    assert!(stdout.contains("\\u009b"), "stdout: {stdout:?}");
}
