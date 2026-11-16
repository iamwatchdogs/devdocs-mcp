// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::error::EvalError;
use crate::suite::Suite;

use super::valid_question;

#[test]
fn a_broken_sidecar_fails_the_suite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta.jsonl");
    std::fs::write(
        &path,
        serde_json::to_string(&valid_question()).unwrap() + "\n",
    )
    .unwrap();

    // Sidecar JSON that does not parse is invalid input.
    std::fs::write(dir.path().join("meta.meta.json"), "{not json").unwrap();
    let err = Suite::load(&path, "meta").unwrap_err();
    assert!(err.to_string().contains("is invalid"), "{err}");

    // A sidecar that cannot be read is an I/O failure.
    let unread_path = dir.path().join("unread.jsonl");
    std::fs::write(
        &unread_path,
        serde_json::to_string(&valid_question()).unwrap() + "\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("unread.meta.json")).unwrap();
    let err = Suite::load(&unread_path, "unread").unwrap_err();
    assert!(matches!(err, EvalError::Io { action: "read", .. }), "{err}");
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
    assert_eq!(suite.meta.as_ref().unwrap().curation_bias, None);
    assert_eq!(suite.hash.len(), 64);
}

#[test]
fn the_meta_sidecar_accepts_the_curation_bias_disclosure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta.jsonl");
    std::fs::write(
        &path,
        serde_json::to_string(&valid_question()).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("meta.meta.json"),
        r#"{"curation_minutes":30,"curated_at":"2026-11-02","description":"d","curation_bias":"keyword-dense query forms are required by the FTS all-token AND rung"}"#,
    )
    .unwrap();
    let suite = Suite::load(&path, "meta").unwrap();
    assert_eq!(
        suite.meta.as_ref().unwrap().curation_bias.as_deref(),
        Some("keyword-dense query forms are required by the FTS all-token AND rung")
    );
}

#[test]
fn a_bad_curated_at_date_fails_the_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta.jsonl");
    std::fs::write(
        &path,
        serde_json::to_string(&valid_question()).unwrap() + "\n",
    )
    .unwrap();
    for bad in [
        "not-a-date",
        "2026-11",
        "2026-1-2",
        "2026-11-2",
        "+2026-11-02",
        "2026-+1-02",
        "2026-13-01",
        "2026-02-30",
        "2026-11-02T00:00:00Z",
    ] {
        std::fs::write(
            dir.path().join("meta.meta.json"),
            format!(r#"{{"curation_minutes":30,"curated_at":"{bad}","description":"d"}}"#),
        )
        .unwrap();
        let err = Suite::load(&path, "meta").unwrap_err();
        assert!(
            err.to_string().contains("not a YYYY-MM-DD date"),
            "{bad}: {err}"
        );
    }
}
