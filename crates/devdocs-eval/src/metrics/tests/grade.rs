// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::metrics::grade_question;
use crate::suite::{Grader, TaskType};

use super::{hit, question, refs};

#[test]
fn unanswerable_questions_pass_only_with_no_hits() {
    let q = question(Grader::RefMatch, TaskType::Unanswerable, Vec::new());
    let passed = grade_question(&q, &[], &[]).passed;
    assert!(passed, "an empty result set passes");

    let hits = vec![hit(1, "next.js", "16.2.9", "proxy", None)];
    let failed = grade_question(&q, &hits, &hits).passed;
    assert!(!failed, "a hit is a failure");
}

#[test]
fn ref_grader_needs_any_expected_ref() {
    let q = question(
        Grader::RefMatch,
        TaskType::ExactApi,
        refs(&[
            "docs://next.js/v16.2.9/proxy",
            "docs://next.js/v16.2.9/config",
        ]),
    );
    let hits = vec![hit(1, "next.js", "16.2.9", "config", None)];
    let score = grade_question(&q, &hits, &hits);
    assert!(score.passed);
    assert!(score.hit_at_5);

    let misses = vec![hit(1, "next.js", "16.2.9", "routing", None)];
    assert!(!grade_question(&q, &misses, &misses).passed);
}

#[test]
fn symbol_grader_needs_full_recall() {
    let mut q = question(Grader::SymbolMatch, TaskType::ExactApi, Vec::new());
    q.expect_symbols = Some(refs(&["middleware", "rewrite"]));

    let hits = vec![
        hit(1, "next.js", "16.2.9", "proxy", Some("middleware")),
        hit(2, "next.js", "16.2.9", "proxy", Some("other")),
    ];
    let score = grade_question(&q, &hits, &hits);
    assert!(!score.passed, "partial recall counts as wrong");
    assert_eq!(score.symbol_recall(), Some(0.5));

    let full = vec![
        hit(1, "next.js", "16.2.9", "proxy", Some("middleware")),
        hit(2, "next.js", "16.2.9", "proxy", Some("rewrite")),
    ];
    assert!(grade_question(&q, &full, &full).passed);
}

#[test]
fn a_manual_grader_never_passes() {
    let q = question(
        Grader::Manual,
        TaskType::ExactApi,
        refs(&["docs://next.js/v16.2.9/proxy"]),
    );
    let hits = vec![hit(1, "next.js", "16.2.9", "proxy", None)];
    assert!(
        !grade_question(&q, &hits, &hits).passed,
        "runs refuse manual-grader suites; the grader still never passes"
    );
}

#[test]
fn symbols_count_only_from_the_pinned_library_and_version() {
    let mut q = question(Grader::SymbolMatch, TaskType::ExactApi, Vec::new());
    q.expect_symbols = Some(refs(&["middleware"]));

    let foreign_version = vec![hit(1, "next.js", "16.1.0", "proxy", Some("middleware"))];
    let score = grade_question(&q, &foreign_version, &foreign_version);
    assert_eq!(score.symbols_found, 0, "another version does not count");

    let foreign_library = vec![hit(1, "zod", "16.2.9", "proxy", Some("middleware"))];
    let score = grade_question(&q, &foreign_library, &foreign_library);
    assert_eq!(score.symbols_found, 0, "another library does not count");

    let case_insensitive = vec![hit(1, "Next.js", "16.2.9", "proxy", Some("middleware"))];
    let score = grade_question(&q, &case_insensitive, &case_insensitive);
    assert_eq!(score.symbols_found, 1, "library case does not matter");
}

#[test]
fn version_correctness_checks_the_top_result() {
    let q = question(
        Grader::RefMatch,
        TaskType::ExactApi,
        refs(&["docs://next.js/v16.2.9/proxy"]),
    );
    let exact = vec![hit(1, "next.js", "16.2.9", "proxy", None)];
    assert_eq!(
        grade_question(&q, &exact, &exact).version_correct,
        Some(true)
    );

    let fallback = vec![hit(1, "next.js", "16.1.0", "proxy", None)];
    assert_eq!(
        grade_question(&q, &fallback, &fallback).version_correct,
        Some(false),
        "a wrong-version top hit stays version-incorrect"
    );

    let foreign = vec![hit(1, "zod", "16.2.9", "proxy", None)];
    assert_eq!(
        grade_question(&q, &foreign, &foreign).version_correct,
        Some(false),
        "another library with the same version string does not count"
    );

    // Nothing served: no served version to be wrong, so no verdict.
    assert_eq!(grade_question(&q, &[], &[]).version_correct, None);
}

#[test]
fn a_multi_library_top_hit_grades_against_the_matched_ref() {
    // The question pins next.js 16.2.9; the correct answer is the serde ref.
    let q = question(
        Grader::RefMatch,
        TaskType::MultiLibrary,
        refs(&["docs://serde/v1.0.229/derive"]),
    );
    let serde_hit = vec![hit(1, "serde", "1.0.229", "derive", None)];
    assert_eq!(
        grade_question(&q, &serde_hit, &serde_hit).version_correct,
        Some(true),
        "the matched expected ref's version decides, not the question pin"
    );

    // A top hit no expected ref matches falls back to the question pin.
    let own_pin = vec![hit(1, "next.js", "16.2.9", "routing", None)];
    assert_eq!(
        grade_question(&q, &own_pin, &own_pin).version_correct,
        Some(true)
    );

    // A top hit neither expected nor on the question pin is incorrect.
    let foreign = vec![hit(1, "zod", "4.6.7", "parse", None)];
    assert_eq!(
        grade_question(&q, &foreign, &foreign).version_correct,
        Some(false)
    );
}
