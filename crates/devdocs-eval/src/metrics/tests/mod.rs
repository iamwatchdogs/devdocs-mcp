// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::metrics::RetrievedRef;
use crate::suite::{Grader, Question, TaskType};

mod aggregate;
mod grade;
mod refs;

/// A hit with the given rank, library, version, path segments, symbol.
fn hit(
    rank: usize,
    library: &str,
    version: &str,
    path: &str,
    symbol: Option<&str>,
) -> RetrievedRef {
    RetrievedRef {
        rank,
        library: library.to_owned(),
        version: version.to_owned(),
        page: None,
        segments: path.split('/').map(str::to_owned).collect(),
        symbol: symbol.map(str::to_owned),
    }
}

/// The same hit served from a page: the page URL path relative to the
/// source root.
fn on_page(mut r: RetrievedRef, page: &str) -> RetrievedRef {
    r.page = Some(page.to_owned());
    r
}

fn ref_list(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// A ref-graded question pinning one ref.
fn question(grader: Grader, task_type: TaskType, refs: Vec<String>) -> Question {
    Question {
        id: "q-001".to_owned(),
        library: "next.js".to_owned(),
        version: "16.2.9".to_owned(),
        task_type,
        question: "q".to_owned(),
        expect_refs: (!refs.is_empty()).then_some(refs),
        expect_symbols: None,
        grader,
        notes: "n".to_owned(),
    }
}

#[test]
fn p95_is_the_nearest_rank_sample() {
    use crate::metrics::p95;

    let hundred: Vec<f64> = (1..=100).map(f64::from).collect();
    assert_eq!(p95(&hundred), 95.0);
    assert_eq!(p95(&[100.0, 200.0, 300.0, 400.0]), 400.0);
    assert_eq!(p95(&[7.0]), 7.0);
    assert_eq!(p95(&[]), 0.0);
    // Unsorted input is ranked, not assumed ordered.
    assert_eq!(p95(&[50.0, 10.0, 30.0, 20.0, 40.0, 5.0]), 50.0);
}

#[test]
fn ratio_of_zero_denominator_is_zero() {
    use crate::metrics::ratio;

    assert_eq!(ratio(0, 0), 0.0);
    assert_eq!(ratio(3, 4), 0.75);
}
