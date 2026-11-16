// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::suite::{Grader, Question, TaskType, hex_hash};

mod load;
mod meta;
mod validate;

/// A valid question in the shape of the docs/implementation/14 example.
fn valid_question() -> Question {
    Question {
        id: "nextjs-proxy-rewrite-001".to_owned(),
        library: "next.js".to_owned(),
        version: "16.2.9".to_owned(),
        task_type: TaskType::ExactApi,
        question: "How do I rewrite an incoming request path in middleware?".to_owned(),
        expect_refs: Some(vec!["docs://next.js/v16.2.9/proxy#rewrite".to_owned()]),
        expect_symbols: Some(vec!["middleware".to_owned()]),
        grader: Grader::RefMatch,
        notes: "accepts the proxy section".to_owned(),
    }
}

fn with_refs(mut q: Question, refs: Vec<String>) -> Question {
    q.expect_refs = Some(refs);
    q
}

#[test]
fn task_types_and_graders_print_stable_names() {
    for (task, name) in [
        (TaskType::ExactApi, "exact_api"),
        (TaskType::HowTo, "how_to"),
        (TaskType::Discovery, "discovery"),
        (TaskType::Migration, "migration"),
        (TaskType::MultiLibrary, "multi_library"),
        (TaskType::AgentTask, "agent_task"),
        (TaskType::Unanswerable, "unanswerable"),
        (TaskType::Poisoned, "poisoned"),
    ] {
        assert_eq!(task.as_str(), name);
    }
    for (grader, name) in [
        (Grader::RefMatch, "ref_match"),
        (Grader::SymbolMatch, "symbol_match"),
        (Grader::Manual, "manual"),
    ] {
        assert_eq!(grader.as_str(), name);
    }
}

#[test]
fn suite_hash_is_the_file_sha256() {
    assert_eq!(
        hex_hash(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
