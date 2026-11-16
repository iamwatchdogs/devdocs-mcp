// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::suite::{Grader, TaskType};

use super::{valid_question, with_refs};

#[test]
fn the_docs_example_is_valid() {
    valid_question().validate().unwrap();
}

#[test]
fn one_of_the_two_expect_lists_is_required() {
    let mut q = valid_question();
    q.expect_refs = None;
    q.expect_symbols = None;
    assert_eq!(
        q.validate().unwrap_err(),
        "one of expect_refs or expect_symbols is required"
    );
}

#[test]
fn empty_fields_are_rejected() {
    for field in ["id", "library", "question", "notes"] {
        let mut q = valid_question();
        match field {
            "id" => q.id = " ".to_owned(),
            "library" => q.library = String::new(),
            "question" => q.question = String::new(),
            "notes" => q.notes = String::new(),
            _ => unreachable!(),
        }
        assert_eq!(
            q.validate().unwrap_err(),
            format!("{field} must be non-empty")
        );
    }
}

#[test]
fn a_graded_question_needs_a_nonempty_expectation() {
    let mut q = valid_question();
    q.expect_refs = Some(Vec::new());
    q.expect_symbols = Some(Vec::new());
    assert!(
        q.validate()
            .unwrap_err()
            .contains("must be non-empty for a graded question")
    );
}

#[test]
fn grader_and_expect_lists_must_agree() {
    let mut q = valid_question();
    q.expect_refs = None;
    q.grader = Grader::RefMatch;
    assert!(q.validate().unwrap_err().contains("ref_match requires"));

    let mut q = valid_question();
    q.expect_symbols = None;
    q.grader = Grader::SymbolMatch;
    assert!(q.validate().unwrap_err().contains("symbol_match requires"));
}

#[test]
fn unanswerable_questions_pin_nothing() {
    let mut q = with_refs(valid_question(), Vec::new());
    q.expect_symbols = None;
    q.task_type = TaskType::Unanswerable;
    q.grader = Grader::RefMatch;
    q.validate().unwrap();

    q.expect_refs = Some(vec!["docs://next.js/v16.2.9/proxy".to_owned()]);
    assert!(q.validate().unwrap_err().contains("unanswerable"));

    q.expect_refs = None;
    q.expect_symbols = None;
    assert_eq!(
        q.validate().unwrap_err(),
        "one of expect_refs or expect_symbols is required"
    );
}
