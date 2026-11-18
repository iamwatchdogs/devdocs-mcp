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
/// level per OWASP — a bare verb ("run the tests") does not fire, and
/// neither does a verb paired with a code-shaped object ("run this
/// code"): the verb must pair with a remote or executable target noun.
/// The tune came from the golden corpus, where the bare verb shape
/// quarantined nine legitimate documentation chunks
/// (docs/implementation/07 §Injection filter).
static ACTION_REQUEST: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // "visit this link", "download this file", "run this script" —
        // the verb pairs with a remote or executable target noun; short
        // modifiers may sit between them ("run this Python script")
        r"(?i)\b(?:visit|download|run)\s+this\s+(?:(?:\w+\s+){0,2})?(?:link|url|page|site|file|script|command|package|repo(?:sitory)?|installer|binary|archive|payload|endpoint|asset|executable|tarball|snippet|bundle|setup|mirror|destination|zip|tool|image|artifact)\b",
        // "run this now", "visit this immediately" — the imperative
        // adverb keeps the bare-verb recall without the code-shaped
        // prose ("run this code", "run this test")
        r"(?i)\b(?:visit|download|run)\s+this\s+(?:now|immediately|first|right away)\b",
        // "curl this"
        r"(?i)\bcurl\s+this\b",
        // exfiltration: "send|upload|post (your|the) (api )?(key|token|credentials)"
        r"(?i)\b(?:send|upload|post)\s+(?:your|the)\s+(?:api\s+)?(?:key|token|credentials)\b",
    ]
    .into_iter()
    .map(compile)
    .collect()
});

/// The minimum interior length of an oversized HTML comment: more than
/// 2000 characters between one comment's opener and its own closer
/// (docs/implementation/07 §Injection filter).
const HIDDEN_COMMENT_MIN_INTERIOR: usize = 2000;

/// The byte spans of the oversized HTML comments in `text`, each covering
/// the whole comment from `<!--` through its own closer — the first `-->`
/// after the opener (docs/implementation/07 §Injection filter). The empty
/// comment forms `<!-->` and `<!--->` are complete per `CommonMark`
/// 0.31.2 and span nothing.
/// Segmentation splits a standalone block over the token target, so a
/// comment over the 2k threshold almost never fits one chunk and the
/// per-chunk oversized-comment rule rarely fires; the pipeline scans the
/// page once with this function, pre-segmentation, and flags every chunk
/// whose byte range intersects a span. The oversized verdict counts each
/// comment's interior separately — opener to its own closer, the closer
/// excluded — so two short comments never pool into one detection, and it
/// counts characters, not bytes. Fenced content is exempt: `<!--` and
/// `-->` markers inside a `fenced` byte range (from the caller's `scan`
/// of the same `text`) never pair. The spans are byte offsets into `text`
/// as given — callers passing fence-stripped out-of-code text get
/// out-of-code-relative offsets, and since fence stripping removes the
/// fence syntax they pass no fenced ranges.
#[must_use]
pub fn oversized_comment_spans(
    text: &str,
    fenced: &[std::ops::Range<usize>],
) -> Vec<std::ops::Range<usize>> {
    // Both marker sets in one forward pass, merged in offset order:
    // re-finding each marker over the remaining suffix per iteration is
    // quadratic on many unclosed `<!--`.
    let mut markers: Vec<(usize, bool)> = text
        .match_indices("<!--")
        .map(|(at, _)| (at, true))
        .chain(text.match_indices("-->").map(|(at, _)| (at, false)))
        .collect();
    markers.sort_unstable();

    let mut spans = Vec::new();
    let mut open_at: Option<usize> = None;
    let mut cursor = 0usize;
    // The marker's fenced-range index: markers and ranges are both
    // offset-ordered, so one index advances as the cursor passes each
    // range's end instead of re-checking every range per marker.
    let mut range_at = 0usize;
    for (at, opens) in markers {
        if at < cursor {
            continue;
        }
        while range_at < fenced.len() && fenced[range_at].end <= at {
            range_at += 1;
        }
        if range_at < fenced.len() && fenced[range_at].start <= at {
            continue;
        }
        // The empty form's closer shares the opener's last dash, so the
        // cursor check below would discard it and the text after would
        // hide to the page end as an unterminated comment.
        if opens {
            let empty_form = if text[at..].starts_with("<!--->") {
                Some(6)
            } else if text[at..].starts_with("<!-->") {
                Some(5)
            } else {
                None
            };
            if let Some(len) = empty_form {
                cursor = at + len;
                continue;
            }
        }
        cursor = at + if opens { 4 } else { 3 };
        match (open_at, opens) {
            (None, true) => open_at = Some(at),
            (Some(open), false) => {
                let body_start = open + 4;
                if text[body_start..at].chars().count() > HIDDEN_COMMENT_MIN_INTERIOR {
                    spans.push(open..at + 3);
                }
                open_at = None;
            }
            // A nested opener keeps the first one; a closer without an
            // opener is noise.
            _ => {}
        }
    }
    if let Some(open) = open_at {
        // An unterminated comment hides everything to the end of the
        // page; its tail still counts as oversized when long enough.
        let body_start = open + 4;
        if text[body_start..].chars().count() > HIDDEN_COMMENT_MIN_INTERIOR {
            spans.push(open..text.len());
        }
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
/// backreferences, so the shape resolves as a set intersection inside one
/// style context: every `color:` value and every `background(-color):`
/// value of a single declaration context are collected, and a shared
/// value fires.
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

/// One declaration context: a quoted `style` attribute value
/// (`style="color: #fff; …"`) or a brace-delimited selector block
/// (`.cls { color: #fff; … }`). Values never pair across contexts.
static STYLE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?is)style\s*=\s*(?:"([^"]*)"|'([^']*)')"#));

static SELECTOR_BLOCK: LazyLock<Regex> = LazyLock::new(|| compile(r"(?s)\{[^{}]*\}"));

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
pub(crate) struct Fence {
    info: String,
    first_body_line: Option<String>,
}

/// A fence the scanner is inside: the opening marker run plus the
/// container state frozen at the opener — a closer line's container
/// prefixes measure against it.
struct OpenFence {
    marker: char,
    run: usize,
    containers: Vec<Container>,
}

/// One container the scanner tracks while it walks the lines: a block
/// quote (a line continues it while the line carries a `>` prefix at its
/// margin) or a list item (a line continues it while the line indents to
/// its content indent). `indent` is the column where the container's
/// content starts — the marker's start column plus the marker width plus
/// the trailing indent columns (`CommonMark` §Block quotes, §List items).
#[derive(Clone, Copy)]
enum Container {
    Quote { indent: usize },
    List { indent: usize },
}

impl Container {
    fn indent(&self) -> usize {
        match self {
            Container::Quote { indent } | Container::List { indent } => *indent,
        }
    }
}

/// The content indent of the innermost open container; the root measures
/// at 0.
fn innermost(containers: &[Container]) -> usize {
    containers.last().map_or(0, Container::indent)
}

/// The column the first non-indent character of `line` sits at when the
/// line starts at column `col` (`CommonMark` §Tabs — a space is one
/// column, a tab advances to the next 4-column tab stop), plus the byte
/// length of the leading indent.
fn skip_indent(line: &str, col: usize) -> (usize, usize) {
    let mut columns = col;
    let mut bytes = 0usize;
    for c in line.chars() {
        match c {
            ' ' => {
                columns += 1;
                bytes += 1;
            }
            '\t' => {
                columns = (columns / 4 + 1) * 4;
                bytes += 1;
            }
            _ => break,
        }
    }
    (columns, bytes)
}

/// Consumes the leading indent of `line` up to column `to`, the line
/// starting at column `col`. Returns the rest and the column the rest
/// starts at; a tab that would overshoot `to` stays unconsumed.
fn take_indent(line: &str, to: usize, col: usize) -> (&str, usize) {
    let mut at = col;
    let mut bytes = 0usize;
    for c in line.chars() {
        let advance = match c {
            ' ' => 1,
            '\t' => (at / 4 + 1) * 4 - at,
            _ => break,
        };
        if at + advance > to {
            break;
        }
        at += advance;
        bytes += c.len_utf8();
    }
    (&line[bytes..], at)
}

/// Walks the container prefixes of `line` against `containers`
/// (outermost first): a level the line cannot continue — a list item the
/// line dedents below, a block quote the line carries no `>` for — pops,
/// and the levels the line itself opens push. Returns the line's
/// remainder after the prefixes, the column the remainder starts at, and
/// whether every open level continued (`false` marks the line as the
/// container boundary `CommonMark` draws there). Blank lines never walk:
/// a list item spans its blank lines.
fn walk_containers<'a>(line: &'a str, containers: &mut Vec<Container>) -> (&'a str, usize, bool) {
    let mut rest = line;
    let mut col = 0usize;
    let mut continued = true;
    // Continue or drop the open levels, innermost first; the innermost
    // that continues consumes its prefix.
    while let Some(level) = containers.last() {
        match *level {
            Container::List { indent } => {
                let (columns, _) = skip_indent(rest, col);
                if columns < indent {
                    containers.pop();
                    continued = false;
                    continue;
                }
                let (at_rest, at_col) = take_indent(rest, indent, col);
                if at_col < indent {
                    // A tab overshoots the item's content indent; the
                    // column split is not representable, so the item
                    // ends.
                    containers.pop();
                    continued = false;
                    continue;
                }
                rest = at_rest;
                col = at_col;
                break;
            }
            Container::Quote { .. } => {
                let (columns, bytes) = skip_indent(rest, col);
                let after = &rest[bytes..];
                if columns - col > 3 || !after.starts_with('>') {
                    containers.pop();
                    continued = false;
                    continue;
                }
                let space = after[1..].starts_with(' ');
                rest = &after[1 + usize::from(space)..];
                col = columns + 1 + usize::from(space);
                break;
            }
        }
    }
    // Parse the levels the line itself opens, outermost in.
    loop {
        let (columns, bytes) = skip_indent(rest, col);
        let after = &rest[bytes..];
        if containers.is_empty() && columns > 3 {
            // A root-level line indented past three columns is an
            // indented code block, not a container (`CommonMark`
            // §Indented code blocks).
            break;
        }
        // A block quote: up to three indent columns before a run of `>`
        // markers.
        let quote_run = after.chars().take_while(|&c| c == '>').count();
        if quote_run > 0 && columns - col <= 3 {
            let space = after[quote_run..].starts_with(' ');
            col = columns + quote_run + usize::from(space);
            containers.push(Container::Quote { indent: col });
            rest = &after[quote_run + usize::from(space)..];
            continue;
        }
        // A list item: a bullet or a 1-9-digit ordered marker followed
        // by one to four columns of whitespace. The content indent is
        // the marker's start column plus the marker width plus the
        // trailing columns; past four the rest is an indented code
        // block, not item content, so no level opens (`CommonMark`
        // §List items).
        let line_bytes = after.as_bytes();
        let bullet = matches!(line_bytes.first(), Some(b'-' | b'+' | b'*'));
        let (marker_cols, marker_bytes) = if bullet {
            (1usize, 1usize)
        } else {
            let digits = line_bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if (1..10).contains(&digits) && matches!(line_bytes.get(digits), Some(b'.' | b')')) {
                (digits + 1, digits + 1)
            } else {
                break;
            }
        };
        let mut trail_cols = 0usize;
        let mut trail_bytes = 0usize;
        for c in after[marker_bytes..].chars() {
            let at = columns + marker_cols + trail_cols;
            let advance = match c {
                ' ' => 1,
                '\t' => (at / 4 + 1) * 4 - at,
                _ => break,
            };
            trail_cols += advance;
            trail_bytes += c.len_utf8();
        }
        if trail_cols == 0 || trail_cols > 4 {
            break;
        }
        col = columns + marker_cols + trail_cols;
        containers.push(Container::List { indent: col });
        rest = &after[marker_bytes + trail_bytes..];
    }
    (rest, col, continued)
}

/// The `CommonMark` fence verdict for one line's remainder after its
/// container prefixes: the marker character and its run length when the
/// remainder, measured from column `col` against the innermost open
/// container's content indent (the root measures at 0), indents at most
/// three relative columns, and starts with three or more backticks or
/// tildes. A backtick-fence info string carrying a backtick does not
/// open — the `CommonMark` rule this v0 scanner keeps. The container
/// tracking is estimation, not a parser; its limits live in
/// docs/implementation/07 §Injection filter (the fence scanner).
fn fence_on(line: &str, col: usize, container: usize) -> Option<(char, usize)> {
    let (columns, bytes) = skip_indent(line, col);
    if columns.saturating_sub(container) > 3 {
        return None;
    }
    let stripped = &line[bytes..];
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

/// Whether `rest` — the line's remainder after its container prefixes,
/// starting at column `col` — closes a fence opened with `(marker,
/// run)`: the indent measures relative to the fence's innermost
/// container (the root at 0) and stays within three columns, and the
/// rest is marker characters of the opening kind only, at least the
/// opening run. The container-relative measure replaces the absolute
/// column rule [`fence_on`] applies — a container margin can make a
/// wider indent the `CommonMark`-valid closer, while a root-level
/// four-space-indented marker line is an indented code block and never
/// closes.
fn fence_closes(open: (char, usize), rest: &str, col: usize, container: usize) -> bool {
    let (columns, bytes) = skip_indent(rest, col);
    if columns.saturating_sub(container) > 3 {
        return false;
    }
    marker_run(open, &rest[bytes..])
}

/// The blind-strip closer check, kept for lines that cannot continue the
/// fence's containers: `CommonMark` ends the containers there, and
/// accepting the closer keeps the fence from sheltering past the
/// boundary. Over-accepting a closer only widens the rule-checked set,
/// never the sheltered one (docs/implementation/07 §Injection filter).
fn fence_closes_blind(open: (char, usize), line: &str) -> bool {
    marker_run(open, line.trim_start_matches([' ', '\t']).trim_end())
}

/// Whether `content` is a marker-only run of the opening kind, at least
/// the opening run.
fn marker_run(open: (char, usize), content: &str) -> bool {
    !content.is_empty() && content.chars().all(|c| c == open.0) && content.chars().count() >= open.1
}

/// The maximal backtick runs of one line, as `(start, end)` byte ranges.
fn backtick_runs(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            runs.push((start, i));
        } else {
            i += 1;
        }
    }
    runs
}

/// The inline-code spans of one line as `(start, end)` byte ranges over
/// the span content: equal-length backtick runs pair left-to-right
/// (the `CommonMark` rule), and a pair's span covers the bytes between
/// the two runs. An unpaired run spans nothing.
fn code_span_ranges(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Vec<(usize, usize)> = Vec::new();
    for (start, end) in backtick_runs(line) {
        let len = end - start;
        if let Some(at) = open.iter().rposition(|&(open_len, _)| open_len == len) {
            let (_, open_start) = open.swap_remove(at);
            spans.push((open_start + len, start));
        } else {
            open.push((len, start));
        }
    }
    spans
}

/// Scans `text` once over its lines: returns the out-of-code text (the
/// complement of the fenced regions, lines joined with `\n`), every
/// fence's info string plus first body line, and the fenced regions' byte
/// ranges — each from the fence-opener line's start through its closer
/// line's end (or the text end for an unterminated fence). An
/// unterminated fence runs to the end of the text. Fence boundaries are
/// container-aware: the scanner tracks the list items and block quotes it
/// walks through and measures fence indents relative to the innermost
/// open container, so a fence at a list margin or inside a block quote
/// opens and closes at its container's content indent
/// (docs/implementation/07 §Injection filter, the fence scanner).
/// Outside fences, each `<!--` opens an HTML comment and each `-->`
/// closes it, acting in offset order on the line; a `<!--` inside an
/// inline code span is quoted text and never opens. A comment's lines
/// never open or close fences; comments inside a real fence stay fenced.
pub(crate) fn scan(text: &str) -> (String, Vec<Fence>, Vec<std::ops::Range<usize>>) {
    let (out_of_code, fences, fenced_ranges, _) = scan_with_state(text, false);
    (out_of_code, fences, fenced_ranges)
}

/// [`scan`] with the comment state seeded `comment_open`: a chunk cut out
/// of the page mid-comment starts inside an open comment, and the seed
/// lets its own scan carry that state in
/// (docs/implementation/07 §Injection filter). Also returns the comment
/// state the walk ends on, which [`comment_open_at`] reads back.
pub(crate) fn scan_with_state(
    text: &str,
    mut comment_open: bool,
) -> (String, Vec<Fence>, Vec<std::ops::Range<usize>>, bool) {
    let mut out_of_code = String::with_capacity(text.len());
    let mut fences: Vec<Fence> = Vec::new();
    let mut fenced_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut containers: Vec<Container> = Vec::new();
    let mut open: Option<OpenFence> = None;
    let mut current_fence: Option<Fence> = None;
    let mut fence_start = 0usize;
    let mut pos = 0usize;
    for line in text.split('\n') {
        let line_start = pos;
        pos += line.len() + 1;
        if let Some(fence) = open.as_mut() {
            // Blank lines are always body: a blank never closes a fence
            // and never ends a list item.
            let closes = if line.trim().is_empty() {
                false
            } else {
                // The fence's container state froze at its opener; the
                // line's prefixes measure against a copy of it.
                let mut frozen = fence.containers.clone();
                let (rest, col, continued) = walk_containers(line, &mut frozen);
                if continued {
                    fence_closes((fence.marker, fence.run), rest, col, innermost(&frozen))
                } else {
                    // The line cannot continue the fence's containers —
                    // `CommonMark` ends them here. The blind strip
                    // accepts the closer so the fence never shelters
                    // past the boundary: over-accepting a closer only
                    // widens the rule-checked set.
                    fence_closes_blind((fence.marker, fence.run), line)
                }
            };
            if let Some(fenced) = &mut current_fence
                && fenced.first_body_line.is_none()
                && !closes
            {
                // The body's first line: the line after the opening
                // marker, when that line is not the closer itself.
                fenced.first_body_line = Some(line.to_owned());
            }
            if closes {
                open = None;
                fenced_ranges.push(fence_start..line_start + line.len());
            }
        } else {
            let mut is_fence = false;
            if !comment_open && !line.trim().is_empty() {
                let (rest, col, _) = walk_containers(line, &mut containers);
                if let Some((marker, run)) = fence_on(rest, col, innermost(&containers)) {
                    is_fence = true;
                    open = Some(OpenFence {
                        marker,
                        run,
                        containers: containers.clone(),
                    });
                    fence_start = line_start;
                    current_fence = Some(Fence {
                        info: rest
                            .trim_start()
                            .chars()
                            .skip(run)
                            .collect::<String>()
                            .trim()
                            .to_owned(),
                        first_body_line: None,
                    });
                }
            }
            if !is_fence {
                // The comment's lines never reach `fence_on`: a ```
                // line inside a comment is comment text. A `<!--` inside
                // an inline code span is quoted text, not an opener.
                let spans = code_span_ranges(line);
                let mut markers: Vec<(usize, bool)> = line
                    .match_indices("<!--")
                    .map(|(at, _)| (at, true))
                    .chain(line.match_indices("-->").map(|(at, _)| (at, false)))
                    .collect();
                markers.sort_unstable();
                for (at, opens) in markers {
                    if opens {
                        if !spans.iter().any(|&(s, e)| s <= at && at + 4 <= e) {
                            comment_open = true;
                        }
                    } else {
                        comment_open = false;
                    }
                }
                if !out_of_code.is_empty() {
                    out_of_code.push('\n');
                }
                out_of_code.push_str(line);
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
        fenced_ranges.push(fence_start..text.len());
    }
    (out_of_code, fences, fenced_ranges, comment_open)
}

/// Whether the page byte offset `at` sits inside an open HTML comment:
/// the comment state the scanner holds when its cursor reaches `at`.
/// The pipeline seeds each chunk's filter run from this so a chunk cut
/// out of the page mid-comment carries the comment state across the
/// boundary (docs/implementation/07 §Injection filter).
#[must_use]
pub(crate) fn comment_open_at(text: &str, at: usize) -> bool {
    scan_with_state(&text[..at], false).3
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
    run_with_state(text, false)
}

/// [`run`] with the comment state seeded `comment_open`: the pipeline
/// passes the page's comment state at the chunk's cut point, so a chunk
/// that starts inside an open HTML comment treats its comment text as
/// comment text — a fence-like line inside it never opens a fence, and
/// the directives after it stay exposed to the rules. Seeding only
/// widens the checked set (docs/implementation/07 §Injection filter).
#[must_use]
pub fn run_with_state(text: &str, comment_open: bool) -> FilterReport {
    let (out_of_code, fences, _, _) = scan_with_state(text, comment_open);

    let mut quarantine_rules = Vec::new();
    if MODEL_DIRECTIVE.iter().any(|re| re.is_match(&out_of_code)) {
        quarantine_rules.push("model_directive");
    }
    if ACTION_REQUEST.iter().any(|re| re.is_match(&out_of_code)) {
        quarantine_rules.push("action_request");
    }
    // The stripped text carries no fence syntax, so there are no fenced
    // ranges to exempt — scan's ranges are in the original text's
    // coordinates, not `out_of_code`'s.
    if !oversized_comment_spans(&out_of_code, &[]).is_empty()
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
/// `background(-color):` property within one declaration context — a
/// quoted `style` attribute or a selector block. Values compare
/// lower-cased. Values never pair across contexts: two separate visible
/// elements (`color:#000; background:#fff` plus `color:#fff;
/// background:#000`) share nothing within either element.
fn is_color_on_color(text: &str) -> bool {
    let mut contexts = Vec::new();
    for caps in STYLE_ATTR.captures_iter(text) {
        if let Some(m) = caps.get(1).or_else(|| caps.get(2)) {
            contexts.push(m.as_str());
        }
    }
    contexts.extend(SELECTOR_BLOCK.find_iter(text).map(|m| m.as_str()));
    contexts.into_iter().any(|context| {
        let foregrounds: Vec<String> = COLOR_PROP
            .captures_iter(context)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_ascii_lowercase())
            .collect();
        !foregrounds.is_empty()
            && BACKGROUND_PROP
                .captures_iter(context)
                .filter_map(|c| c.get(1))
                .any(|m| foregrounds.contains(&m.as_str().to_ascii_lowercase()))
    })
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
