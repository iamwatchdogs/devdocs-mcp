// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{Baseline, PerQueryCost, ReportInput, SchemaRent, render_gate_table, render_report};
use crate::error::EvalError;
use crate::gates;
use crate::metrics::{Fraction, QuestionScore, SuiteMetrics};
use crate::report::{Host, load_baseline, write_artifact};
use crate::suite::{Grader, Question, TaskType};

/// One graded question fixture for the report rendering.
fn fixture_question() -> (Question, QuestionScore) {
    let question = Question {
        id: "nextjs-proxy-rewrite-001".to_owned(),
        library: "next.js".to_owned(),
        version: "16.2.9".to_owned(),
        task_type: TaskType::ExactApi,
        question: "How do I rewrite an incoming request path?".to_owned(),
        expect_refs: Some(vec!["docs://next.js/v16.2.9/proxy#rewrite".to_owned()]),
        expect_symbols: Some(vec!["rewrite".to_owned()]),
        grader: Grader::RefMatch,
        notes: "n".to_owned(),
    };
    let score = QuestionScore {
        passed: false,
        hit_at_5: false,
        symbols_found: 0,
        symbols_pinned: 1,
        version_correct: Some(false),
    };
    (question, score)
}

/// A Phase 0 baseline fixture for the report rendering.
fn fixture_baseline() -> Baseline {
    Baseline {
        recorded: "2026-11-02".to_owned(),
        workflow: "hosted docs MCP + web search".to_owned(),
        schema_rent: SchemaRent {
            band_tokens_per_tool: [550, 1400],
            source: "research: interfaces.md §1 (GEM §4)".to_owned(),
            notes: "raw multi-tool MCP dumps run higher".to_owned(),
        },
        per_query_cost: PerQueryCost {
            hosted_api_usd_per_1000_queries: 1.66,
            raw_mcp_usd_per_1000_queries: 3.66,
            assumptions: "frontier list pricing".to_owned(),
            source: "research: interfaces.md §1 (GLMP Ch.9)".to_owned(),
        },
    }
}

#[test]
fn report_snapshot() {
    let (question, score) = fixture_question();
    let metrics = SuiteMetrics::new(&[(question.clone(), score.clone())]);
    let gates = gates::evaluate(&metrics, Some(12.5));
    let scored = vec![(question, score)];
    let baseline = fixture_baseline();
    let suite_hash = "0".repeat(64);
    let db_hash = "1".repeat(64);
    let input = ReportInput {
        suite: "fixture",
        date: "2026-11-02T00:00:00.000000000Z",
        metrics,
        p95_ms: 12.5,
        gates: &gates,
        scored: &scored,
        curation_minutes: Some(90),
        baseline: Some(&baseline),
        suite_hash: &suite_hash,
        db_hash: &db_hash,
    };
    // Insta resolves the snapshot directory from this file's location;
    // pin it back to the crate's src/snapshots so the tracked snapshot
    // file keeps its path.
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../snapshots");
    settings.bind(|| insta::assert_snapshot!("render_report_fixture", render_report(&input)));
}

#[test]
fn the_host_records_cores_and_an_optional_cpu() {
    let host = Host::current();
    assert!(host.cores > 0, "{host:?}");
    let value = serde_json::to_value(&host).unwrap();
    assert!(value["cores"].as_u64().unwrap() > 0, "{value}");
    assert!(
        value["cpu"].is_null() || value["cpu"].is_string(),
        "{value}"
    );
}

#[test]
fn a_broken_baseline_record_is_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("phase0.json");
    std::fs::write(&path, "{nope").unwrap();
    let err = load_baseline(&path).unwrap_err();
    assert!(err.to_string().contains("is invalid"), "{err}");
    assert_eq!(err.exit_code(), 1);
}

#[test]
fn an_unreadable_baseline_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("phase0.json");
    std::fs::create_dir(&path).unwrap();
    let err = load_baseline(&path).unwrap_err();
    assert!(matches!(err, EvalError::Io { action: "read", .. }), "{err}");
}

#[test]
fn artifact_writes_surface_io_failures() {
    let dir = tempfile::tempdir().unwrap();

    // A parent that is a file fails the directory creation.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let err = write_artifact(&blocker.join("sub").join("report.md"), "x").unwrap_err();
    assert!(
        matches!(
            err,
            EvalError::Io {
                action: "create",
                ..
            }
        ),
        "{err}"
    );

    // A target that is a directory fails the write itself.
    let target = dir.path().join("report.md");
    std::fs::create_dir(&target).unwrap();
    let err = write_artifact(&target, "x").unwrap_err();
    assert!(
        matches!(
            err,
            EvalError::Io {
                action: "write",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn gate_table_renders_every_row() {
    let table = render_gate_table(&gates::evaluate(
        &SuiteMetrics {
            hit_at_5: Fraction { hits: 0, total: 1 },
            symbol_recall: Fraction { hits: 0, total: 0 },
            failure_rate: Fraction { hits: 1, total: 1 },
            version_correctness: Fraction { hits: 0, total: 1 },
        },
        Some(9.9),
    ));
    for gate in [
        "Symbol recall",
        "hit@5",
        "p95 latency",
        "Cost per 1,000 queries",
        "Retrieval failures",
        "Version correctness",
    ] {
        assert!(table.contains(gate), "{table}");
    }
    assert!(table.contains("INCONCLUSIVE"));
}
