//! The redaction pass's tests: one positive and one negative fixture per
//! pattern kind, plus the overlap-resolution, fixed-point, and sweep
//! cases (docs/implementation/09).

// Tests use unwrap/expect for setup and asserts; a panic fails the test (tracked in #87).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// 36 alphanumeric characters after `ghp_`.
const GHP: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
/// 82 identifier characters after `github_pat_`.
const PAT: &str =
    "github_pat_0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRST";
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
    let text =
        format!("aws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = {AWS_SECRET}");
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
fn redacts_an_aws_secret_in_a_no_space_assignment() {
    // `is_secret_char` admits `=`: without the assignment split,
    // `KEY=` bridges into the value and the 40-byte exact-run check
    // rejects the 44-byte run, leaving the secret raw.
    let text = format!("AWS_SECRET_ACCESS_KEY={AWS_SECRET}");
    let out = redact(&text);
    assert!(
        out.contains("[REDACTED:aws_key]"),
        "the no-space assignment must redact: {out}"
    );
    assert!(!out.contains(AWS_SECRET));
}

#[test]
fn redacts_an_aws_secret_after_a_space_padded_assignment() {
    // The assignment sign padded on the left only (`KEY =value`): a
    // run starting AT the `=` counts its own byte, so the value's run
    // came out 41 bytes and the exact-40 check rejected it, leaving
    // the secret raw.
    let text = format!("AWS_SECRET_ACCESS_KEY ={AWS_SECRET}");
    let out = redact(&text);
    assert!(
        out.contains("[REDACTED:aws_key]"),
        "the space-padded assignment must redact: {out}"
    );
    assert!(!out.contains(AWS_SECRET));
}

#[test]
fn redacts_an_aws_secret_value_starting_with_a_base64_sign() {
    // A value starting `/` or `+` used to bridge at the `=`: the
    // split fired only for alphanumeric values, so the fused 43-byte
    // run failed the exact-40 check and the secret survived raw. A
    // `/` or `+` after a padding `=` is not valid base64, so the
    // split cannot break a legit padded run.
    for head in ['/', '+'] {
        let text = format!("AWS_SECRET_ACCESS_KEY={head}{}", "A".repeat(39));
        let out = redact(&text);
        assert!(
            out.contains("[REDACTED:aws_key]"),
            "the {head}-headed value must redact: {out}"
        );
        assert!(!out.contains("AAAA"), "no raw value survives: {out}");
    }
}

#[test]
fn keeps_a_longer_blob_with_base64_padding() {
    // Padding `=` only sits at a run's end: a 42-character blob with
    // two trailing pad characters stays rejected by the exact-40
    // rule even in AWS context.
    let text = format!("aws_secret_access_key = {}==", "a".repeat(40));
    assert_eq!(redact(&text), text);
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
fn an_overlapping_detection_extends_the_kept_span() {
    // The OpenAI plain fallback keeps `sk-` + 20 A's; the AWS secret
    // run (20 A's + `/` + 19 B's = 40 bytes, ` aws` context) starts
    // inside it and extends past it. Dropping the AWS detection would
    // leave the B tail raw; the kept span widens instead.
    let text = format!("sk-{}{}{} aws", "A".repeat(20), "/", "B".repeat(19));
    let out = redact(&text);
    assert_eq!(out, "[REDACTED:openai_key] aws");
    assert!(!out.contains('B'), "no raw B tail survives: {out}");
}

#[test]
fn redaction_reaches_a_fixed_point_when_a_marker_moves_context() {
    // Pass 1 redacts only the GitHub token: the `aws` label sits
    // outside the 40-character A-run's context window. The marker
    // replacement shifts `aws` into the window, so a single pass
    // would leave the A's raw and break the idempotency contract.
    let text = format!("{} {} {} aws", "A".repeat(40), ".".repeat(50), GHP);
    let once = redact(&text);
    assert_eq!(redact(&once), once, "redact must reach a fixed point");
    assert!(!once.contains(GHP));
    assert!(
        once.contains("[REDACTED:aws_key]"),
        "the marker shift must surface the A run: {once}"
    );
}

#[test]
fn redaction_sweeps_chained_aws_candidates_when_the_pass_cap_hits() {
    // Ten candidates, each one pass apart: every marker carries
    // `aws_key` into the next candidate's window, so the
    // context-gated passes converge one candidate at a time and the
    // 8-pass cap leaves some behind. The fail-closed sweep takes the
    // rest without the context requirement, and the fixed point
    // holds.
    let mut text = String::new();
    for _ in 0..10 {
        text.push_str(&"A".repeat(40));
        text.push_str(&".".repeat(60));
    }
    text.push_str(" aws");
    let once = redact(&text);
    assert_eq!(redact(&once), once, "redact must reach a fixed point");
    assert!(
        !once.contains(&"A".repeat(40)),
        "no raw candidate survives the sweep: {once}"
    );
    assert_eq!(once.matches("[REDACTED:aws_key]").count(), 10);
}

#[test]
fn keeps_an_unrelated_run_when_the_gated_passes_converge_at_the_cap() {
    // Eight chained candidates settle exactly at the 8-pass bound:
    // the eighth pass removes the final candidate, and one
    // re-checked gated pass confirms convergence — the fail-closed
    // sweep must not run, or it would redact the unrelated
    // context-free exact-40 run too.
    let digest = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b";
    let mut text = String::new();
    for _ in 0..8 {
        text.push_str(&"A".repeat(40));
        text.push_str(&".".repeat(60));
    }
    text.push_str(" aws");
    text.push_str(&".".repeat(100));
    text.push_str(digest);
    let once = redact(&text);
    assert_eq!(redact(&once), once, "redact must reach a fixed point");
    assert_eq!(once.matches("[REDACTED:aws_key]").count(), 8);
    assert!(
        once.contains(digest),
        "the unrelated run must survive a converged redaction: {once}"
    );
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
