// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::metrics::{Fraction, QuestionScore, SuiteMetrics, grade_question};
use crate::suite::{Grader, TaskType};

use super::{hit, question, ref_list};

#[test]
fn symbol_recall_counts_found_over_pinned() {
    let score = QuestionScore {
        passed: true,
        hit_at_5: true,
        symbols_found: 2,
        symbols_pinned: 3,
        version_correct: Some(true),
    };
    assert_eq!(score.symbol_recall(), Some(2.0 / 3.0));

    let none = QuestionScore {
        passed: true,
        hit_at_5: true,
        symbols_found: 0,
        symbols_pinned: 0,
        version_correct: Some(true),
    };
    assert_eq!(none.symbol_recall(), None);
}

#[test]
fn an_abstention_leaves_the_version_correctness_denominator() {
    let q = question(
        Grader::RefMatch,
        TaskType::ExactApi,
        ref_list(&["docs://next.js/v16.2.9/proxy"]),
    );
    let unans = question(Grader::RefMatch, TaskType::Unanswerable, Vec::new());

    // A correct abstention: nothing served, the question passes, and it
    // carries no version verdict.
    let unans_score = grade_question(&unans, &[], &[]);
    assert!(unans_score.passed);
    assert_eq!(unans_score.version_correct, None);

    let hits = vec![hit(1, "next.js", "16.2.9", "proxy", None)];
    let scored = vec![
        (q.clone(), grade_question(&q, &hits, &hits)),
        (unans.clone(), unans_score),
    ];
    let m = SuiteMetrics::new(&scored);
    // The abstention is out: the denominator counts the graded question
    // only, and its correct top hit fills the numerator.
    assert_eq!(
        (m.version_correctness.hits, m.version_correctness.total),
        (1, 1)
    );
}

#[test]
fn an_all_unanswerable_suite_has_an_empty_version_denominator() {
    let unans = question(Grader::RefMatch, TaskType::Unanswerable, Vec::new());
    let scored = vec![(unans.clone(), grade_question(&unans, &[], &[]))];
    let m = SuiteMetrics::new(&scored);
    assert_eq!(
        (m.version_correctness.hits, m.version_correctness.total),
        (0, 0)
    );
}

#[test]
fn suite_aggregates_use_the_documented_denominators() {
    let q_ref = question(
        Grader::RefMatch,
        TaskType::ExactApi,
        ref_list(&["docs://next.js/v16.2.9/proxy"]),
    );
    let q_sym = {
        let mut q = question(Grader::SymbolMatch, TaskType::ExactApi, Vec::new());
        q.expect_symbols = Some(ref_list(&["a", "b"]));
        q
    };
    let q_unans = question(Grader::RefMatch, TaskType::Unanswerable, Vec::new());

    let hit_proxy = vec![hit(1, "next.js", "16.2.9", "proxy", Some("a"))];
    let sym_score = grade_question(&q_sym, &hit_proxy, &hit_proxy); // partial recall → fail
    let unans_score = grade_question(&q_unans, &hit_proxy.clone(), &hit_proxy.clone()); // hit on unanswerable → fail
    let scored = vec![
        (
            q_ref.clone(),
            grade_question(&q_ref, &hit_proxy, &hit_proxy),
        ), // pass, hit@5, 1/2 symbols, version ok
        (q_sym, sym_score),
        (q_unans, unans_score),
    ];
    let m = SuiteMetrics::new(&scored);
    // hit@5: only q_ref pins refs, and its expected ref is in the top five.
    assert_eq!((m.hit_at_5.hits, m.hit_at_5.total), (1, 1));
    // symbols: q_sym pins a,b; only a found (via the shared hit).
    assert_eq!((m.symbol_recall.hits, m.symbol_recall.total), (1, 2));
    // failures: q_sym and the unanswerable hit fail.
    assert_eq!((m.failure_rate.hits, m.failure_rate.total), (2, 3));
    // the served top hits are all on-pin (2 questions) and the
    // unanswerable abstention-failure leaves the denominator.
    assert_eq!(
        (m.version_correctness.hits, m.version_correctness.total),
        (2, 2)
    );
    assert_eq!(m.failure_rate.value(), 2.0 / 3.0);
    let f = Fraction { hits: 1, total: 4 };
    assert_eq!(f.value(), 0.25);
}
