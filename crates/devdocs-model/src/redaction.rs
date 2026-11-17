//! Secret redaction over indexed content.
//!
//! [`redact`] scans text for credential-shaped matches — token formats
//! taken from gitleaks baselines — and replaces every match with a
//! `[REDACTED:<kind>]` marker. The markers are stable: re-redacting
//! already-redacted output is a no-op, so the pass can run at more than
//! one pipeline stage.
//!
//! All patterns are RE2-compatible (no lookarounds, no backreferences) so
//! they compile with the `regex` crate. Overlapping detections resolve
//! presidio-style: the larger span wins on containment, a fixed kind
//! priority breaks exact ties, and one single-pass rewrite applies the
//! surviving spans so the offsets never shift mid-scan.

use std::sync::LazyLock;

use regex::Regex;

/// The kind of secret a redaction marker stands for. The `as_str`
/// spelling is the marker suffix and the stable storage/JSON spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RedactionKind {
    /// A GitHub personal access token (`ghp_…`, `gho_…`, `github_pat_…`).
    GithubToken,
    /// An `OpenAI` API key (`sk-…T3BlbkFJ…`, plain `sk-…` fallback).
    OpenaiKey,
    /// An AWS access key id (`AKIA…`) or a guarded secret companion.
    AwsKey,
    /// A PEM private-key block, `-----BEGIN … PRIVATE KEY-----` through
    /// its matching end line.
    PemBlock,
    /// An `Authorization: Bearer …` HTTP header line.
    BearerHeader,
}

impl RedactionKind {
    /// The storage spelling used as the `[REDACTED:<kind>]` marker
    /// suffix.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GithubToken => "github_token",
            Self::OpenaiKey => "openai_key",
            Self::AwsKey => "aws_key",
            Self::PemBlock => "pem_block",
            Self::BearerHeader => "bearer_header",
        }
    }
}

/// One detected secret span waiting for the overlap resolution.
struct Detection {
    kind: RedactionKind,
    start: usize,
    end: usize,
}

/// The `[REDACTED:` head every marker carries. The rewrite does not treat
/// it specially; no detection pattern matches inside a marker, which is
/// what makes [`redact`] idempotent.
const MARKER_PREFIX: &str = "[REDACTED:";

/// Compiles a fixed pattern of this module. Every pattern here is a
/// verified literal — the test suite exercises each one — so the compile
/// cannot fail.
#[allow(clippy::expect_used)]
fn compile(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("static redaction pattern must compile")
}

/// GitHub token formats: the classic `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`
/// 36-character access tokens plus the 82-character fine-grained
/// `github_pat_` tokens.
static GITHUB_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    compile("(?:ghp|gho|ghu|ghs|ghr)_[0-9a-zA-Z]{36}|github_pat_[A-Za-z0-9_]{82}")
});

/// `OpenAI` API keys. The project/service/admin shapes with the embedded
/// `T3BlbkFJ` marker come first so the leftmost-first alternation prefers
/// the full span over the plain `sk-` fallback on the same start.
static OPENAI_KEY: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        "sk-(?:proj|svcacct|admin)?-?[A-Za-z0-9_-]{20,}T3BlbkFJ[A-Za-z0-9_-]{20,}",
        "|sk-[A-Za-z0-9_-]{20,}"
    ))
});

/// AWS access key ids: the 20-character ids with the documented prefixes.
static AWS_ACCESS_KEY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\b(?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16}\b"));

/// The context a 40-character secret candidate must sit in to count as an
/// AWS secret: an explicit `aws` mention (`aws`, `aws_secret`, `AWS key`),
/// a `secret access key` phrase, or a detected access key id next to it.
static AWS_CONTEXT: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        "(?i)\\baws\\b|aws[_-]|secret[_-]?access[_-]?key",
        "|\\b(?:AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16}\\b"
    ))
});

/// PEM private-key blocks: the BEGIN line (with any algorithm words) lazy
/// through the END line. `[A-Za-z0-9_-]` under the case-insensitive flag
/// admits `OPENSSH`, `RSA`, `ENCRYPTED`, and the `PRIVATE KEY BLOCK`
/// spelling.
static PEM_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        "(?i)-----BEGIN[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----",
        "[\\s\\S-]{64,}?KEY(?: BLOCK)?-----"
    ))
});

/// Bearer authorization header lines: the header name, optional spacing,
/// the scheme, one or more separator spaces or tabs (RFC 6750 §2.1), and
/// an 8+-character credential run.
static BEARER_HEADER: LazyLock<Regex> =
    LazyLock::new(|| compile(r"(?i)authorization:[ \t]{0,5}bearer[ \t]+[\w=~@.+/-]{8,}"));

/// Whether `c` extends a base64-ish secret run (the
/// [`AWS_SECRET_CANDIDATE`] character set).
fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '/' || c == '+' || c == '='
}

/// The context window a secret candidate sits in: up to 80 characters on
/// either side of the run, char-boundary aligned. 80 reaches the
/// key=value line head (`aws_secret_access_key = …`) and a short label
/// line above it, while staying far below the point where unrelated
/// document text drifts in; the trailing half covers the configs that
/// spell the label after the value (`… = aws secret access key`).
fn context_window(text: &str, start: usize, end: usize) -> &str {
    let mut from = start.saturating_sub(80);
    while from < start && !text.is_char_boundary(from) {
        from += 1;
    }
    let mut to = (end + 80).min(text.len());
    while to > end && !text.is_char_boundary(to) {
        to -= 1;
    }
    &text[from..to]
}

/// The maximal runs of base64-ish secret characters in `text`, as
/// `(start, end)` byte offsets. Secret characters are all ASCII, so the
/// byte lengths are exact.
fn secret_runs(text: &str) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, c) in text.char_indices().chain([(text.len(), '\0')]) {
        match start {
            Some(s) if !is_secret_char(c) => {
                runs.push((s, idx));
                start = None;
            }
            None if is_secret_char(c) => start = Some(idx),
            _ => {}
        }
    }
    runs
}

/// Collects the AWS secret detections: a base64-ish run of exactly 40
/// characters — the exact-run rule keeps a longer blob from redacting its
/// tail half — that sits in AWS context (an `aws` mention, a `secret
/// access key` phrase, or a detected access key id within the
/// surrounding ±80 window).
fn detect_aws_secret(text: &str, detections: &mut Vec<Detection>) {
    for (start, end) in secret_runs(text) {
        if end - start == 40 && AWS_CONTEXT.is_match(context_window(text, start, end)) {
            detections.push(Detection {
                kind: RedactionKind::AwsKey,
                start,
                end,
            });
        }
    }
}

/// The tie-break rank for two detections that span exactly the same
/// bytes: the earlier rank wins. PEM blocks rank first because they
/// swallow the most context, then the token shapes, then header lines.
const fn rank(kind: RedactionKind) -> u8 {
    match kind {
        RedactionKind::PemBlock => 0,
        RedactionKind::OpenaiKey => 1,
        RedactionKind::GithubToken => 2,
        RedactionKind::AwsKey => 3,
        RedactionKind::BearerHeader => 4,
    }
}

/// Collects every detection of every kind over `text`, then resolves
/// overlaps: sort by start, longer span first, kind rank last; walk once
/// and keep a detection only when it starts at or after the kept span's
/// end. Same start keeps the longer span (containment), equal spans keep
/// the higher-ranked kind, and a later span inside a kept one drops.
fn resolve(text: &str) -> Vec<Detection> {
    let mut detections = Vec::new();
    for m in GITHUB_TOKEN.find_iter(text) {
        detections.push(Detection {
            kind: RedactionKind::GithubToken,
            start: m.start(),
            end: m.end(),
        });
    }
    for m in OPENAI_KEY.find_iter(text) {
        detections.push(Detection {
            kind: RedactionKind::OpenaiKey,
            start: m.start(),
            end: m.end(),
        });
    }
    for m in AWS_ACCESS_KEY.find_iter(text) {
        detections.push(Detection {
            kind: RedactionKind::AwsKey,
            start: m.start(),
            end: m.end(),
        });
    }
    detect_aws_secret(text, &mut detections);
    for m in PEM_BLOCK.find_iter(text) {
        detections.push(Detection {
            kind: RedactionKind::PemBlock,
            start: m.start(),
            end: m.end(),
        });
    }
    for m in BEARER_HEADER.find_iter(text) {
        detections.push(Detection {
            kind: RedactionKind::BearerHeader,
            start: m.start(),
            end: m.end(),
        });
    }

    detections.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(b.end.cmp(&a.end))
            .then(rank(a.kind).cmp(&rank(b.kind)))
    });

    let mut kept = Vec::new();
    let mut cursor = 0usize;
    for detection in detections {
        if detection.start >= cursor {
            cursor = detection.end;
            kept.push(detection);
        }
    }
    kept
}

/// Replaces every secret-shaped match in `text` with a
/// `[REDACTED:<kind>]` marker. Idempotent: the markers match no pattern,
/// so re-redacting redacted output returns it unchanged. Overlapping
/// detections collapse into one marker for the larger span.
#[must_use]
pub fn redact(text: &str) -> String {
    let kept = resolve(text);
    if kept.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for detection in &kept {
        out.push_str(&text[cursor..detection.start]);
        out.push_str(MARKER_PREFIX);
        out.push_str(detection.kind.as_str());
        out.push(']');
        cursor = detection.end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    // Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// 36 alphanumeric characters after `ghp_`.
    const GHP: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    /// 82 identifier characters after `github_pat_`.
    const PAT: &str = "github_pat_0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRST";
    /// The canonical AWS example secret: exactly 40 base64-ish characters.
    const AWS_SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

    #[test]
    fn kind_spellings_are_stable() {
        assert_eq!(RedactionKind::GithubToken.as_str(), "github_token");
        assert_eq!(RedactionKind::OpenaiKey.as_str(), "openai_key");
        assert_eq!(RedactionKind::AwsKey.as_str(), "aws_key");
        assert_eq!(RedactionKind::PemBlock.as_str(), "pem_block");
        assert_eq!(RedactionKind::BearerHeader.as_str(), "bearer_header");
    }

    #[test]
    fn redacts_github_tokens() {
        let text = format!("token {GHP} and {PAT} end");
        let out = redact(&text);
        assert!(out.contains("[REDACTED:github_token]"));
        assert!(!out.contains("ghp_"));
        assert!(!out.contains("github_pat_"));
        // Exactly two markers, both of the same kind.
        assert_eq!(out.matches("[REDACTED:github_token]").count(), 2);
    }

    #[test]
    fn keeps_a_short_github_prefix() {
        // 35 characters after `ghp_` — one short of the token shape.
        let text = "run ghp_0123456789abcdefghijklmnopqrstuvwxy first";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redacts_openai_keys() {
        let long = concat!(
            "sk-proj-abcdEFGHijklMNOPqrst",
            "T3Bl",
            "bkFJ",
            "uvwxyzABCDEFGHIJKLMNOP"
        );
        let plain = "sk-plain0123456789abcdefghijklmn";
        let text = format!("keys {long} {plain}");
        let out = redact(&text);
        assert_eq!(out.matches("[REDACTED:openai_key]").count(), 2);
        assert!(!out.contains("sk-"));
    }

    #[test]
    fn keeps_short_sk_prefixes_in_prose() {
        let text = "our sk-learn pipeline and sk-12345 setup";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn prefers_the_long_openai_span_over_the_fallback() {
        // The plain `sk-` fallback also matches inside the long key shape;
        // the resolution must keep one marker for the full span.
        let long = concat!(
            "sk-proj-abcdEFGHijklMNOPqrst",
            "T3Bl",
            "bkFJ",
            "uvwxyzABCDEFGHIJKLMNOP"
        );
        let text = format!("key {long}");
        let out = redact(&text);
        assert_eq!(out, "key [REDACTED:openai_key]");
    }

    #[test]
    fn redacts_aws_access_key_id_and_guarded_secret() {
        let text = format!(
            "aws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = {AWS_SECRET}"
        );
        let out = redact(&text);
        assert!(out.contains("[REDACTED:aws_key]"));
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!out.contains(AWS_SECRET));
        assert_eq!(out.matches("[REDACTED:aws_key]").count(), 2);
    }

    #[test]
    fn redacts_an_aws_secret_with_trailing_context() {
        // The label spelled after the value — the symmetric window's
        // reason: the backward-only window missed this shape.
        let text = format!("{AWS_SECRET} = the AWS secret access key");
        let out = redact(&text);
        assert!(
            out.contains("[REDACTED:aws_key]"),
            "the trailing-context secret must redact: {out}"
        );
        assert!(!out.contains(AWS_SECRET));
    }

    #[test]
    fn redacts_an_aws_secret_with_leading_context() {
        // The classic config line: the label before the value.
        let text = format!("aws_secret_access_key = {AWS_SECRET}");
        let out = redact(&text);
        assert_eq!(out, "aws_secret_access_key = [REDACTED:aws_key]");
    }

    #[test]
    fn keeps_a_40_char_string_without_aws_context() {
        // A bare SHA-1-shaped hex run with no AWS context nearby.
        let text = "digest 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2a0";
        assert!(text.len() >= 40);
        assert_eq!(redact(text), text);
    }

    #[test]
    fn keeps_a_shorter_aws_secret_run() {
        // 39 base64-ish characters next to AWS context — one short.
        let text = "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKE";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redacts_pem_private_key_blocks() {
        // The body must exceed the 64-character minimum the pattern needs
        // between the BEGIN and END lines.
        let body = "MIIEvQ".repeat(16);
        let text = format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----");
        let out = redact(&text);
        assert_eq!(out, "[REDACTED:pem_block]");
    }

    #[test]
    fn keeps_non_private_pem_blocks() {
        let text = "-----BEGIN CERTIFICATE-----\nMIIDdzCCAl+gAwIBAgIE\n-----END CERTIFICATE-----";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redacts_bearer_headers() {
        let text = "authorization: Bearer abc123def456ghi789";
        let out = redact(text);
        assert_eq!(out, "[REDACTED:bearer_header]");
    }

    #[test]
    fn redacts_a_bearer_header_with_repeated_spaces() {
        // RFC 6750 §2.1 allows one or more spaces between the scheme and
        // the credentials; a single-space separator let the credential
        // survive with two.
        let text = "authorization: Bearer   abc123def456";
        let out = redact(text);
        assert_eq!(out, "[REDACTED:bearer_header]");
    }

    #[test]
    fn keeps_bearer_without_header_context() {
        let text = "the word Bearer alone, and Bearer abc123def456 in prose";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn keeps_a_basic_auth_header() {
        let text = "Authorization: Basic dXNlcjpwYXNzd29yZA==";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn plain_text_passes_through() {
        let text = "The routing layer maps requests to handlers. Nothing secret here.";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redaction_is_idempotent() {
        let text = format!(
            "header: authorization: Bearer abc123def456\nkey {GHP}\nsk-plain0123456789abcdefghijklmn\n-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\naws_secret_access_key = {AWS_SECRET}",
            "MIIEvQ".repeat(16)
        );
        let once = redact(&text);
        assert_ne!(once, text);
        assert_eq!(redact(&once), once);
    }

    #[test]
    fn the_bearer_span_swallows_an_embedded_token() {
        // The bearer credential run contains a full GitHub token; the
        // bearer detection starts earlier and spans wider, so the token
        // detection drops and one marker covers the header.
        let text = format!("authorization: Bearer {GHP}");
        let out = redact(&text);
        assert_eq!(out, "[REDACTED:bearer_header]");
        assert!(!out.contains("github_token"));
    }

    #[test]
    fn an_empty_input_stays_empty() {
        assert_eq!(redact(""), "");
    }

    #[test]
    fn redacts_a_token_with_multibyte_context_around_it() {
        // The ±80-byte context window walks to char boundaries; multibyte
        // padding forces both walks, so the AWS-context scan on the window
        // must not panic or split a character.
        // `context_window` runs only for a 40-byte secret run, so the
        // fixture uses the AWS secret; the `_` in a GitHub token would
        // split the run. 30 `あ` before the label put the window's low
        // edge mid-character, 29 after the secret put its high edge
        // mid-character, so both boundary walks run.
        let pre = "あ".repeat(30);
        let post = "あ".repeat(29);
        let text = format!("{pre}aws_secret_access_key = {AWS_SECRET}{post}");
        let out = redact(&text);
        assert!(out.contains("[REDACTED:aws_key]"));
        assert!(!out.contains(AWS_SECRET));
        assert!(out.starts_with(&pre), "the leading context survives");
        assert!(out.ends_with(&post), "the trailing context survives");
    }
}
