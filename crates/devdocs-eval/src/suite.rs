//! The golden-set suite: the question schema, validation, and JSONL loading
//! (docs/implementation/14 §Golden-set format).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::EvalError;

/// The eight-category task-type union (docs/implementation/14
/// §Golden-set format; research: benchmark-protocol.md Step 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Known library and version, exact API lookup.
    ExactApi,
    /// Known library, unknown page, natural-language how-to.
    HowTo,
    /// Unknown or niche library discovery.
    Discovery,
    /// Version migration comparison.
    Migration,
    /// Multi-library comparison or cross-library composition.
    MultiLibrary,
    /// Agent coding task with a deterministic verifier.
    AgentTask,
    /// Abstention correctness: the answer does not exist.
    Unanswerable,
    /// Poisoned-doc case: unsafe-content handling.
    Poisoned,
}

impl TaskType {
    /// The stable JSON spelling of the category.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExactApi => "exact_api",
            Self::HowTo => "how_to",
            Self::Discovery => "discovery",
            Self::Migration => "migration",
            Self::MultiLibrary => "multi_library",
            Self::AgentTask => "agent_task",
            Self::Unanswerable => "unanswerable",
            Self::Poisoned => "poisoned",
        }
    }
}

/// The grader that decides a question's outcome (docs/implementation/14
/// §Golden-set format).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grader {
    /// Pass when at least one expected `docs://` ref appears in the results.
    RefMatch,
    /// Pass when every pinned symbol appears in the results.
    SymbolMatch,
    /// Not machine-gradable; a run refuses suites that carry it.
    Manual,
}

impl Grader {
    /// The stable JSON spelling of the grader.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RefMatch => "ref_match",
            Self::SymbolMatch => "symbol_match",
            Self::Manual => "manual",
        }
    }
}

/// One golden question: one line of `eval/golden/<suite>.jsonl`
/// (docs/implementation/14 §Golden-set format).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// Stable id, unique inside the suite.
    pub id: String,
    /// Normalized library name, for example `next.js`.
    pub library: String,
    /// Exact pinned version, for example `16.2.9`. Ranges are invalid.
    pub version: String,
    /// The task category.
    pub task_type: TaskType,
    /// The question text, as an agent would ask it.
    pub question: String,
    /// Acceptable `docs://` refs. Present, and empty, for `unanswerable`
    /// questions: a hit is a failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_refs: Option<Vec<String>>,
    /// Symbols the results must contain for symbol-recall scoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_symbols: Option<Vec<String>>,
    /// The grader for the question.
    pub grader: Grader,
    /// Curation note: what counts as correct, and why.
    pub notes: String,
}

impl Question {
    /// Validates the question against the golden-set contract.
    ///
    /// # Errors
    /// One line naming the first broken rule.
    pub fn validate(&self) -> Result<(), String> {
        ensure_non_empty("id", &self.id)?;
        ensure_non_empty("library", &self.library)?;
        ensure_pinned_version(&self.version)?;
        ensure_non_empty("question", &self.question)?;
        ensure_non_empty("notes", &self.notes)?;

        if self.expect_refs.is_none() && self.expect_symbols.is_none() {
            return Err("one of expect_refs or expect_symbols is required".to_owned());
        }
        let refs = self.expect_refs.as_deref().unwrap_or_default();
        let symbols = self.expect_symbols.as_deref().unwrap_or_default();

        if self.task_type == TaskType::Unanswerable {
            if !refs.is_empty() || !symbols.is_empty() {
                return Err(
                    "unanswerable questions expect an empty result set; expect_refs and \
                     expect_symbols must be empty"
                        .to_owned(),
                );
            }
        } else {
            if refs.is_empty() && symbols.is_empty() {
                return Err(
                    "expect_refs or expect_symbols must be non-empty for a graded question"
                        .to_owned(),
                );
            }
            match self.grader {
                Grader::RefMatch if refs.is_empty() => {
                    return Err("grader ref_match requires non-empty expect_refs".to_owned());
                }
                Grader::SymbolMatch if symbols.is_empty() => {
                    return Err("grader symbol_match requires non-empty expect_symbols".to_owned());
                }
                _ => {}
            }
            for r in refs {
                validate_ref(r, &self.library, &self.version, self.task_type)?;
            }
        }
        Ok(())
    }
}

/// Rejects an empty field value.
fn ensure_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must be non-empty"))
    } else {
        Ok(())
    }
}

/// Rejects versions that are not an exact pin (docs/implementation/14:
/// "Version is exact. A question without a pinned version is invalid input
/// for the harness.").
fn ensure_pinned_version(version: &str) -> Result<(), String> {
    let bad = |why: &str| format!("version {version:?} is not an exact pin: {why}");
    match version.chars().next() {
        None => return Err(bad("empty")),
        Some('v' | 'V') => {
            return Err(bad(
                "drop the leading v; the pin is bare and refs carry the v",
            ));
        }
        Some('^' | '~' | '>' | '<' | '=' | '*') => {
            return Err(bad("a range selector pins nothing"));
        }
        _ => {}
    }
    if version.eq_ignore_ascii_case("latest") {
        return Err(bad("'latest' pins nothing"));
    }
    if version.chars().any(char::is_whitespace) {
        return Err(bad("whitespace"));
    }
    for component in version.split('.') {
        if component.is_empty() {
            return Err(bad("empty component"));
        }
        if component == "*" || component.eq_ignore_ascii_case("x") {
            return Err(bad("a wildcard component pins nothing"));
        }
    }
    Ok(())
}

/// A parsed `docs://` ref: `docs://<library>/v<version>/<path>[#<fragment>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRef {
    /// The library authority.
    pub library: String,
    /// The pinned version, without the leading `v`.
    pub version: String,
    /// The non-empty path segments.
    pub segments: Vec<String>,
    /// The fragment after `#`, when present.
    pub fragment: Option<String>,
}

impl ParsedRef {
    /// Parses the ref grammar; `None` for any broken shape.
    ///
    /// The path is one or more `/`-separated segments without `..`, `?`, or
    /// whitespace; a fragment must be non-empty without whitespace.
    #[must_use]
    pub fn parse(r: &str) -> Option<ParsedRef> {
        let rest = r.strip_prefix("docs://")?;
        let (library, tail) = rest.split_once('/')?;
        if library.is_empty() {
            return None;
        }
        let (path, fragment) = match tail.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (tail, None),
        };
        let (version_segment, segment_text) = path.split_once('/')?;
        let version = version_segment.strip_prefix('v')?;
        if version.is_empty() {
            return None;
        }
        let segments: Vec<String> = segment_text.split('/').map(str::to_owned).collect();
        if segments.is_empty()
            || segments.iter().any(|s| {
                s.is_empty() || s == ".." || s.contains('?') || s.contains(char::is_whitespace)
            })
        {
            return None;
        }
        if fragment.is_some_and(|f| f.is_empty() || f.contains(char::is_whitespace)) {
            return None;
        }
        Some(ParsedRef {
            library: library.to_owned(),
            version: version.to_owned(),
            segments,
            fragment: fragment.map(str::to_owned),
        })
    }
}

/// Validates one `docs://` ref against the question's pin.
///
/// `multi_library` questions may pin refs for other libraries; those refs
/// pin their own version.
fn validate_ref(r: &str, library: &str, version: &str, task_type: TaskType) -> Result<(), String> {
    let bad = |why: String| format!("malformed ref {r:?}: {why}");
    let parsed = ParsedRef::parse(r).ok_or_else(|| {
        bad("the shape is not docs://<library>/v<version>/<path>[#<fragment>]".to_owned())
    })?;
    if task_type != TaskType::MultiLibrary {
        if parsed.library != library {
            return Err(bad(format!(
                "library {:?} does not match the question library {library:?}",
                parsed.library
            )));
        }
        if parsed.version != version {
            return Err(bad(format!(
                "pins version {:?}, question pins {version:?}",
                parsed.version
            )));
        }
    }
    // A `multi_library` ref pins another library's version; every ref
    // version is still an exact pin.
    ensure_pinned_version(&parsed.version).map_err(bad)?;
    Ok(())
}

/// Curation metadata: one `<suite>.meta.json` sidecar next to the JSONL.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteMeta {
    /// Curation effort in minutes; the research protocol records it as a
    /// measurement (docs/implementation/14 §Pilot).
    pub curation_minutes: u64,
    /// Curation date, RFC 3339.
    pub curated_at: String,
    /// One-line description of the suite.
    pub description: String,
}

/// A parsed and validated golden suite.
#[derive(Debug, Clone)]
pub struct Suite {
    /// Suite name, for example `pilot`.
    pub name: String,
    /// The JSONL file the suite loaded from.
    pub path: PathBuf,
    /// SHA-256 hex of the suite file bytes; the manifest records it.
    pub hash: String,
    /// The questions, in file order.
    pub questions: Vec<Question>,
    /// Curation metadata, when the sidecar file exists.
    pub meta: Option<SuiteMeta>,
}

impl Suite {
    /// Loads and validates a suite file, plus its `<stem>.meta.json`
    /// sidecar when one sits next to it.
    ///
    /// # Errors
    /// [`EvalError::SuiteParse`] for a bad line; [`EvalError::InvalidSuite`]
    /// for a broken cross-question rule or a bad sidecar;
    /// [`EvalError::Io`] when the file is unreadable.
    pub fn load(path: &Path, name: &str) -> Result<Suite, EvalError> {
        let bytes = std::fs::read(path).map_err(|source| EvalError::Io {
            action: "read",
            path: path.to_path_buf(),
            source,
        })?;
        let text = std::str::from_utf8(&bytes).map_err(|e| EvalError::InvalidSuite {
            suite: name.to_owned(),
            reason: format!("not UTF-8: {e}"),
        })?;

        let mut questions = Vec::new();
        let mut seen = HashSet::new();
        for (idx, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let line_no = idx + 1;
            let fail = |reason: String| EvalError::SuiteParse {
                suite: name.to_owned(),
                line: line_no,
                reason,
            };
            let question: Question = serde_json::from_str(line).map_err(|e| fail(e.to_string()))?;
            question.validate().map_err(fail)?;
            if !seen.insert(question.id.clone()) {
                return Err(fail(format!("duplicate id {:?}", question.id)));
            }
            questions.push(question);
        }
        if questions.is_empty() {
            return Err(EvalError::InvalidSuite {
                suite: name.to_owned(),
                reason: "the suite has no questions".to_owned(),
            });
        }

        let stem = path
            .file_stem()
            .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned());
        let meta_path = path.with_file_name(format!("{stem}.meta.json"));
        let meta =
            match std::fs::read(&meta_path) {
                Ok(meta_bytes) => Some(serde_json::from_slice(&meta_bytes).map_err(|e| {
                    EvalError::InvalidSuite {
                        suite: name.to_owned(),
                        reason: format!("{} is invalid: {e}", meta_path.display()),
                    }
                })?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(source) => {
                    return Err(EvalError::Io {
                        action: "read",
                        path: meta_path,
                        source,
                    });
                }
            };

        Ok(Suite {
            name: name.to_owned(),
            path: path.to_path_buf(),
            hash: hex_hash(&bytes),
            questions,
            meta,
        })
    }
}

/// SHA-256 of `bytes` as lowercase hex.
#[must_use]
pub fn hex_hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // write! on a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{Grader, Question, Suite, TaskType, ensure_pinned_version, hex_hash, validate_ref};

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
    fn versions_must_be_exact_pins() {
        for bad in [
            "^1.2.3", "~1.2", ">=1.0", "1.x", "1.2.*", "latest", "v16.2.9", "", "16.2.9 ", "16..9",
        ] {
            let err = ensure_pinned_version(bad).unwrap_err();
            assert!(err.contains("not an exact pin"), "{bad}: {err}");
        }
        for good in ["16.2.9", "4.6.7", "1.0.229", "4.0.0-beta.1", "16"] {
            ensure_pinned_version(good).unwrap();
        }
    }

    #[test]
    fn unknown_task_types_fail_at_parse() {
        let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"fuzzy_lookup","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"grader":"ref_match","notes":"n"}"#;
        let err = serde_json::from_str::<Question>(line).unwrap_err();
        assert!(err.to_string().contains("unknown variant"), "{err}");
    }

    #[test]
    fn a_missing_grader_fails_at_parse() {
        let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"notes":"n"}"#;
        let err = serde_json::from_str::<Question>(line).unwrap_err();
        assert!(err.to_string().contains("missing field `grader`"), "{err}");
    }

    #[test]
    fn unknown_fields_fail_at_parse() {
        let line = r#"{"id":"x","library":"next.js","version":"16.2.9","task_type":"exact_api","question":"q","expect_refs":["docs://next.js/v16.2.9/proxy"],"grader":"ref_match","notes":"n","grader_hint":"x"}"#;
        let err = serde_json::from_str::<Question>(line).unwrap_err();
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn malformed_refs_are_rejected() {
        let cases = [
            "https://next.js/v16.2.9/proxy",
            "next.js/v16.2.9/proxy",
            "docs://next.js/proxy",
            "docs://next.js/v16.2.9",
            "docs://next.js/v16.2.9/",
            "docs://next.js/v16.3.0/proxy",
            "docs://next.js/16.2.9/proxy",
            "docs://zod/v4.6.7/parse",
            "docs://next.js/v16.2.9/../etc",
            "docs://next.js/v16.2.9/a//b",
            "docs://next.js/v16.2.9/a b",
            "docs://next.js/v16.2.9/a?b",
            "docs://next.js/v16.2.9/proxy#",
            "docs://next.js/v16.2.9/proxy#a b",
        ];
        for r in cases {
            let err = validate_ref(r, "next.js", "16.2.9", TaskType::ExactApi).unwrap_err();
            assert!(err.contains("malformed ref"), "{r}: {err}");
        }
    }

    #[test]
    fn well_formed_refs_validate() {
        for (r, library, version) in [
            ("docs://next.js/v16.2.9/proxy", "next.js", "16.2.9"),
            ("docs://next.js/v16.2.9/proxy#rewrite", "next.js", "16.2.9"),
            (
                "docs://next.js/v16.2.9/app/api-reference#use-cache",
                "next.js",
                "16.2.9",
            ),
            ("docs://zod/v4.6.7/parse", "zod", "4.6.7"),
        ] {
            validate_ref(r, library, version, TaskType::ExactApi)
                .unwrap_or_else(|e| panic!("{r}: {e}"));
        }
    }

    #[test]
    fn multi_library_questions_accept_other_libraries() {
        validate_ref(
            "docs://serde/v1.0.229/derive",
            "next.js",
            "16.2.9",
            TaskType::MultiLibrary,
        )
        .unwrap();
        let err = validate_ref(
            "docs://serde/v1.0.229/derive",
            "next.js",
            "16.2.9",
            TaskType::ExactApi,
        )
        .unwrap_err();
        assert!(err.contains("does not match the question library"));
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
    fn load_reads_the_meta_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.jsonl");
        std::fs::write(
            &path,
            serde_json::to_string(&valid_question()).unwrap() + "\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("meta.meta.json"),
            r#"{"curation_minutes":30,"curated_at":"2026-11-02","description":"d"}"#,
        )
        .unwrap();
        let suite = Suite::load(&path, "meta").unwrap();
        assert_eq!(suite.questions.len(), 1);
        assert_eq!(suite.meta.as_ref().unwrap().curation_minutes, 30);
        assert_eq!(suite.hash.len(), 64);
    }

    #[test]
    fn suite_hash_is_the_file_sha256() {
        assert_eq!(
            hex_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    proptest::proptest! {
        #[test]
        fn well_formed_refs_always_validate(
            library in "[a-z][a-z0-9.]{2,15}",
            version in r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}",
            segment in "[a-z][a-z0-9-]{0,15}",
            fragment in "[a-z][a-z0-9-]{0,15}",
            depth in 1usize..4,
        ) {
            let path = std::iter::repeat_n(segment.as_str(), depth).collect::<Vec<_>>().join("/");
            let r = format!("docs://{library}/v{version}/{path}#{fragment}");
            super::validate_ref(&r, &library, &version, super::TaskType::ExactApi)
                .unwrap_or_else(|e| panic!("{r}: {e}"));
        }
    }
}
