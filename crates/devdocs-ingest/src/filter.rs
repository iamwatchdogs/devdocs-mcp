//! The injection filter v0: phrase rules over chunk text
//! (docs/implementation/07 §Injection filter).
//!
//! [`run`] scores one chunk's text and reports which rules fired. The
//! rules are cheap, auditable regexes over the text **outside** fenced
//! code blocks — fenced content is data a tutorial quotes, not a
//! directive to the agent — plus one annotation for fences whose info
//! string or first body line is a shell command that writes outside the
//! workspace. Quarantine rules store the chunk flagged and excluded from
//! serving; the review rule only lowers serving confidence. The filter
//! never edits text: quarantine, never silent edit.
//!
//! The rule vocabulary and the review-flag confidence factor are owned by
//! docs/implementation/07 §Injection filter; this module holds no
//! documentation copies of them.

use std::sync::LazyLock;

use regex::Regex;

/// What the injection filter decided about one chunk's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterReport {
    /// Quarantine rules that fired: the chunk stores flagged and excluded
    /// from serving until an operator releases it.
    pub quarantine_rules: Vec<&'static str>,
    /// Review rules that fired: the chunk stores flagged for review and
    /// serves with a confidence penalty.
    pub review_rules: Vec<&'static str>,
    /// A fenced block in the chunk carries a shell command that writes
    /// outside the workspace. An annotation, not a rule name: it never
    /// enters `rule_names`.
    pub code_caution: bool,
}

/// Quarantine rule: directives addressed to a model, outside code.
static MODEL_DIRECTIVE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // "ignore (all )?(previous|prior) instructions"
        r"(?i)\bignore\s+(?:all\s+)?(?:previous|prior)\s+instructions\b",
        // "disregard (your|the) rules"
        r"(?i)\bdisregard\s+(?:your|the)\s+rules\b",
        // "system prompt"
        r"(?i)\bsystem\s+prompt\b",
        // "SYSTEM: You are"
        r"(?i)\bsystem\s*:\s*you\s+are",
        // "instructions: 1." — numbered instruction lists addressed to a
        // model
        r"(?i)instructions?\s*:\s*\d+\.",
    ]
    .into_iter()
    .map(compile)
    .collect()
});

/// Quarantine rule: verb + target action requests outside code. Phrase
/// level per OWASP — a bare verb ("run the tests") does not fire.
static ACTION_REQUEST: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // "visit this", "download this", "run this"
        r"(?i)\b(?:visit|download|run)\s+this\b",
        // "curl this"
        r"(?i)\bcurl\s+this\b",
        // exfiltration: "send|upload|post (your|the) (api )?(key|token|credentials)"
        r"(?i)\b(?:send|upload|post)\s+(?:your|the)\s+(?:api\s+)?(?:key|token|credentials)\b",
    ]
    .into_iter()
    .map(compile)
    .collect()
});

/// Quarantine rule, shape 1: an HTML comment over 2k characters
/// (docs/implementation/07 §Injection filter).
static HIDDEN_COMMENT: LazyLock<Regex> = LazyLock::new(|| compile(r"(?s)<!--[\s\S]{2001,}?-->"));

/// The minimum interior length of an oversized HTML comment: more than
/// 2000 characters between the markers, the same threshold the
/// [`HIDDEN_COMMENT`] regex carries.
const HIDDEN_COMMENT_MIN_INTERIOR: usize = 2000;

/// The byte spans of the oversized HTML comments in `text`, each covering
/// the whole comment from `<!--` through `-->` (docs/implementation/07
/// §Injection filter). Segmentation splits a standalone block over the
/// token target, so a >2k comment almost never fits one chunk and the
/// per-chunk oversized-comment rule rarely fires; the pipeline scans the
/// page once with this function, pre-segmentation, and flags every chunk
/// whose byte range intersects a span. Nested or overlapping comments take
/// the outermost `-->`, one span each.
#[must_use]
pub fn oversized_comment_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = text[cursor..].find("<!--") {
        let open = cursor + start;
        let body_start = open + 4;
        let Some(close_at) = text[body_start..].find("-->") else {
            // An unterminated comment hides everything to the end of the
            // page; its tail still counts as oversized when long enough.
            if text.len() - body_start > HIDDEN_COMMENT_MIN_INTERIOR {
                spans.push(open..text.len());
            }
            break;
        };
        let end = body_start + close_at + 3;
        if end - body_start > HIDDEN_COMMENT_MIN_INTERIOR {
            spans.push(open..end);
        }
        cursor = end;
    }
    spans
}

/// Quarantine rule, shape 2: zero-width and invisible characters above a
/// density threshold. The set spans the zero-width and bidi-control
/// blocks (`U+200B`–`U+200F`), the word joiner (`U+2060`), the BOM
/// (`U+FEFF`), the soft hyphen (`U+00AD`), the Mongolian vowel separator
/// (`U+180E`), the invisible function-application characters
/// (`U+2061`–`U+2064`), and the Unicode tag block
/// (`U+E0000`–`U+E007F`, the invisible byte-payload channel).
static ZERO_WIDTH: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        "[\u{200b}-\u{200f}\u{2060}\u{feff}\u{ad}\u{180e}\u{2061}-\u{2064}",
        "\u{e0000}-\u{e007f}]"
    ))
});

/// The zero-width density threshold: above 5% of the out-of-code
/// characters (a count above one in twenty), the text hides content
/// between invisible joiners (docs/implementation/07 §Injection filter).
const ZERO_WIDTH_DENSITY_DENOM: usize = 20;

/// Quarantine rule, shape 3: text colored the same as its background —
/// the classic invisible-text CSS shape. The `regex` crate carries no
/// backreferences, so the shape resolves as a set intersection: every
/// `color:` value and every `background(-color):` value are collected,
/// and a shared value fires.
static COLOR_PROP: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r#"(?i)(?:^|[;{\s"'])color\s*:\s*"#,
        r"(#[0-9a-f]{3,8}|rgba?\(\s*\d+\s*,\s*\d+\s*,\s*\d+[^)]*\))"
    ))
});

static BACKGROUND_PROP: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?i)background(?:-color)?\s*:\s*",
        r"(#[0-9a-f]{3,8}|rgba?\(\s*\d+\s*,\s*\d+\s*,\s*\d+[^)]*\))"
    ))
});

/// Review rule: markdown links plus bare URLs against whitespace tokens,
/// above 0.5 the chunk is link-heavy and flagged for review
/// (docs/implementation/07 §Injection filter).
static MARKDOWN_LINK: LazyLock<Regex> = LazyLock::new(|| compile(r"\[[^\]]*\]\([^)]*\)"));

static BARE_URL: LazyLock<Regex> = LazyLock::new(|| compile(r"https?://[^\s)>\]]+"));

/// The link-ratio review threshold: above one link per two tokens the
/// chunk is link-heavy (docs/implementation/07 §Injection filter).
const LINK_RATIO_LIMIT_NUM: usize = 1;
/// The link-ratio review threshold's denominator.
const LINK_RATIO_LIMIT_DENOM: usize = 2;

/// The code-caution shapes: shell commands that write outside the
/// workspace. Matched against fence info strings and fence body first
/// lines, tuned toward recall (docs/implementation/07 §Injection filter).
static CODE_CAUTION: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // "curl … | sh" (bash, zsh, dash included)
        r"(?i)curl[^|\n]*\|\s*(?:sudo\s+)?(?:ba|z|da)?sh\b",
        // "rm -rf" / "rm -fr"
        r"(?i)\brm\s+-(?:rf|fr)\b",
        // "git push --force" / "git push -f"
        r"(?i)\bgit\s+push\b[^;\n]*(?:--force\b|-f\b)",
        // "sudo "
        r"(?i)\bsudo\s",
        // "chmod 777"
        r"(?i)\bchmod\s+777\b",
    ]
    .into_iter()
    .map(compile)
    .collect()
});

/// Compiles a fixed pattern of this module. Every pattern here is a
/// verified literal — the test suite exercises each one — so the compile
/// cannot fail.
#[allow(clippy::expect_used)]
fn compile(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("static filter pattern must compile")
}

/// One fenced code block the scanner found: its info string (the text
/// after the opening delimiter) and its first body line, the two inputs
/// the code-caution shapes match against. `first_body_line` stays `None`
/// for a fence that closes on the line right after its opener.
struct Fence {
    info: String,
    first_body_line: Option<String>,
}

/// The `CommonMark` fence verdict for one line: the marker character
/// and its run length when the line (after up to three leading
/// whitespace characters) starts with three or more backticks or tildes.
/// A backtick-fence info string carrying a backtick does not open — the
/// `CommonMark` rule this v0 scanner keeps.
fn fence_on(line: &str) -> Option<(char, usize)> {
    let mut stripped = line;
    for _ in 0..3 {
        stripped = stripped
            .strip_prefix(|c: char| c.is_whitespace())
            .unwrap_or(stripped);
    }
    let marker = stripped.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let run = stripped.chars().take_while(|&c| c == marker).count();
    if run < 3 {
        return None;
    }
    // A backtick fence's info string may not contain a backtick.
    if marker == '`' && stripped[run..].contains('`') {
        return None;
    }
    Some((marker, run))
}

/// Whether `line` closes a fence opened with `(marker, run)`: after up to
/// three leading whitespace characters, the line is only marker
/// characters of the same kind, at least the opening run
/// (docs/implementation/07 §Injection filter, the fence scanner).
fn fence_closes(open: (char, usize), line: &str) -> bool {
    let Some((marker, run)) = fence_on(line) else {
        return false;
    };
    marker == open.0 && run >= open.1 && line.trim().chars().all(|c| c == marker)
}

/// Scans `text` once over its lines: returns the out-of-code text (the
/// complement of the fenced regions, lines joined with `\n`) and every
/// fence's info string plus first body line. An unterminated fence runs
/// to the end of the text.
fn scan(text: &str) -> (String, Vec<Fence>) {
    let mut out_of_code = String::with_capacity(text.len());
    let mut fences: Vec<Fence> = Vec::new();
    let mut open: Option<(char, usize)> = None;
    let mut current_fence: Option<Fence> = None;
    for line in text.split('\n') {
        match open {
            Some(fence) => {
                if let Some(fenced) = &mut current_fence
                    && fenced.first_body_line.is_none()
                    && !fence_closes(fence, line)
                {
                    // The body's first line: the line after the opening
                    // marker, when that line is not the closer itself.
                    fenced.first_body_line = Some(line.to_owned());
                }
                if fence_closes(fence, line) {
                    open = None;
                }
            }
            None => {
                if let Some(fence) = fence_on(line) {
                    open = Some(fence);
                    current_fence = Some(Fence {
                        info: line
                            .trim_start()
                            .chars()
                            .skip(fence.1)
                            .collect::<String>()
                            .trim()
                            .to_owned(),
                        first_body_line: None,
                    });
                } else {
                    if !out_of_code.is_empty() {
                        out_of_code.push('\n');
                    }
                    out_of_code.push_str(line);
                }
            }
        }
        if open.is_none()
            && let Some(fenced) = current_fence.take()
        {
            fences.push(fenced);
        }
    }
    // An unterminated fence holds its region to the end of the text; its
    // collected info and first line still count for the caution check.
    if let Some(fenced) = current_fence.take() {
        fences.push(fenced);
    }
    (out_of_code, fences)
}

/// The whitespace tokens of `text` (docs/implementation/07 §Injection
/// filter, the link-ratio denominator).
fn token_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Scores one chunk's text (docs/implementation/07 §Injection filter).
/// Quarantine rules and the review rule match only the text outside
/// fenced code blocks; the code-caution shapes match each fence's info
/// string and first body line.
#[must_use]
pub fn run(text: &str) -> FilterReport {
    let (out_of_code, fences) = scan(text);

    let mut quarantine_rules = Vec::new();
    if MODEL_DIRECTIVE.iter().any(|re| re.is_match(&out_of_code)) {
        quarantine_rules.push("model_directive");
    }
    if ACTION_REQUEST.iter().any(|re| re.is_match(&out_of_code)) {
        quarantine_rules.push("action_request");
    }
    if HIDDEN_COMMENT.is_match(&out_of_code)
        || is_zero_width_heavy(&out_of_code)
        || is_color_on_color(&out_of_code)
    {
        quarantine_rules.push("hidden_text");
    }

    let mut review_rules = Vec::new();
    if is_link_heavy(&out_of_code) {
        review_rules.push("link_ratio");
    }

    let code_caution = fences.iter().any(|fence| {
        CODE_CAUTION.iter().any(|re| {
            re.is_match(&fence.info)
                || fence
                    .first_body_line
                    .as_deref()
                    .is_some_and(|line| re.is_match(line))
        })
    });

    FilterReport {
        quarantine_rules,
        review_rules,
        code_caution,
    }
}

/// Whether the zero-width characters of `text` exceed the density
/// threshold over its total character count. Integer math: a count above
/// one in twenty fires.
fn is_zero_width_heavy(text: &str) -> bool {
    let total = text.chars().count();
    total > 0 && ZERO_WIDTH.find_iter(text).count() * ZERO_WIDTH_DENSITY_DENOM > total
}

/// Whether `text` colors any text the same as its background: a color
/// value that appears on both a `color:` property and a
/// `background(-color):` property. Values compare lower-cased.
fn is_color_on_color(text: &str) -> bool {
    let foregrounds: Vec<String> = COLOR_PROP
        .captures_iter(text)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect();
    if foregrounds.is_empty() {
        return false;
    }
    BACKGROUND_PROP
        .captures_iter(text)
        .filter_map(|c| c.get(1))
        .any(|m| foregrounds.contains(&m.as_str().to_ascii_lowercase()))
}

/// Whether the markdown links plus bare URLs of `text` exceed the
/// link-ratio limit over its whitespace tokens. A URL inside a markdown
/// link counts once — the markdown match masks its destination before the
/// bare-URL pass runs.
fn is_link_heavy(text: &str) -> bool {
    let tokens = token_count(text);
    if tokens == 0 {
        return false;
    }
    let mut links = 0usize;
    let mut masked = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for matched in MARKDOWN_LINK.find_iter(text) {
        masked.push_str(&text[cursor..matched.start()]);
        cursor = matched.end();
        links += 1;
    }
    masked.push_str(&text[cursor..]);
    links += BARE_URL.find_iter(&masked).count();
    links * LINK_RATIO_LIMIT_DENOM > tokens * LINK_RATIO_LIMIT_NUM
}

#[cfg(test)]
mod tests;
