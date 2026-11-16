// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::error::EvalError;
use crate::suite::{Grader, Question, Suite};

use super::valid_question;

#[test]
fn unknown_task_types_fail_at_parse() {
    let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"fuzzy_lookup","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"grader":"ref_match","notes":"n"}"#;
    let err = serde_json::from_str::<Question>(line).unwrap_err();
    assert!(err.to_string().contains("unknown variant"), "{err}");
}

#[test]
fn an_omitted_grader_defaults_to_ref_match() {
    let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"notes":"n"}"#;
    let q = serde_json::from_str::<Question>(line).unwrap();
    assert_eq!(q.grader, Grader::RefMatch);
}

#[test]
fn unknown_fields_fail_at_parse() {
    let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"grader":"ref_match","notes":"n","grader_hint":"x"}"#;
    let err = serde_json::from_str::<Question>(line).unwrap_err();
    assert!(err.to_string().contains("unknown field"), "{err}");
}

#[test]
fn duplicate_ids_and_empty_files_are_suite_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup.jsonl");
    let line = serde_json::to_string(&valid_question()).unwrap();
    std::fs::write(&path, format!("{line}\n{line}\n")).unwrap();
    let err = Suite::load(&path, "dup").unwrap_err();
    assert!(err.to_string().contains("duplicate id"), "{err}");

    let empty = dir.path().join("empty.jsonl");
    std::fs::write(&empty, "\n\n").unwrap();
    assert!(
        Suite::load(&empty, "empty")
            .unwrap_err()
            .to_string()
            .contains("no questions")
    );
}

#[test]
fn unreadable_and_non_utf8_suites_fail_the_load() {
    let dir = tempfile::tempdir().unwrap();
    // A missing file is an I/O failure (exit 3), not invalid input.
    let err = Suite::load(&dir.path().join("absent.jsonl"), "absent").unwrap_err();
    assert!(matches!(err, EvalError::Io { action: "read", .. }), "{err}");

    // Bytes that are not UTF-8 are invalid input.
    let path = dir.path().join("binary.jsonl");
    std::fs::write(&path, [0xFF, 0xFE]).unwrap();
    let err = Suite::load(&path, "binary").unwrap_err();
    assert!(err.to_string().contains("not UTF-8"), "{err}");
}
