//! The metric math (docs/implementation/14 §Metrics): per-question grading
//! and the suite aggregates.
//!
//! Everything here is pure and checked against hand-computed fixtures. The
//! binary grader has no partial credit: partial counts as wrong (research:
//! benchmark-protocol.md Step 6).

use serde::Serialize;

use crate::refs::ParsedRef;
use crate::suite::{Grader, Question, TaskType};

/// One retrieval result, shaped for grading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetrievedRef {
    /// 1-based rank in the result list.
    pub rank: usize,
    /// The library the hit was served from.
    pub library: String,
    /// The exact version the hit was served from.
    pub version: String,
    /// The hit's page URL path relative to the source root, when the
    /// chunk carries a usable page URL (issue #88).
    pub page: Option<String>,
    /// The section-path segments of the hit.
    pub segments: Vec<String>,
    /// The hit's symbol, when the chunk documents one.
    pub symbol: Option<String>,
}

impl RetrievedRef {
    /// The canonical `docs://` spelling, for the run artifacts.
    #[must_use]
    pub fn reference(&self) -> String {
        let path = self.segments.join("/");
        let mut reference = format!("docs://{}/v{}/", self.library, self.version);
        if let Some(page) = &self.page {
            reference.push_str(page);
            reference.push('@');
        }
        reference.push_str(&path);
        if let Some(symbol) = &self.symbol {
            reference.push('#');
            reference.push_str(symbol);
        }
        reference
    }
}

/// Whether an expected ref matches a hit.
///
/// The libraries compare case-insensitively; the version must be exact. A
/// pinned page must equal the hit's page exactly — the page URL path
/// relative to the source root — so a section title another page shares
/// can no longer grade as a hit (issue #88); a ref without a page part
/// pins no page and matches on the breadcrumb alone. The expected path
/// matches when it equals the hit path or is a trailing run of its
/// segments, so a pinned `proxy` matches a hit served from `app/proxy`.
/// A pinned fragment requires the hit's symbol to equal it.
#[must_use]
pub fn ref_matches(expected: &str, hit: &RetrievedRef) -> bool {
    let Some(expected) = ParsedRef::parse(expected) else {
        return false;
    };
    if !expected.library.eq_ignore_ascii_case(&hit.library) || expected.version != hit.version {
        return false;
    }
    if let Some(page) = &expected.page {
        let pinned = page.join("/");
        if hit.page.as_deref() != Some(pinned.as_str()) {
            return false;
        }
    }
    let hit_tail = hit.segments.len().checked_sub(expected.segments.len());
    let Some(offset) = hit_tail else {
        return false;
    };
    if hit.segments[offset..] != expected.segments[..] {
        return false;
    }
    match &expected.fragment {
        Some(fragment) => hit.symbol.as_deref() == Some(fragment.as_str()),
        None => true,
    }
}

/// hit@5: at least one expected ref within the first five results
/// (docs/implementation/14 §Metrics).
#[must_use]
pub fn hit_at_5(expected_refs: &[String], hits: &[RetrievedRef]) -> bool {
    expected_refs
        .iter()
        .any(|e| hits.iter().take(5).any(|h| ref_matches(e, h)))
}

/// The binary ref grader: at least one expected ref anywhere in the result
/// list. hit@5 is the ranking-quality subset of this.
#[must_use]
pub fn ref_match(expected_refs: &[String], hits: &[RetrievedRef]) -> bool {
    expected_refs
        .iter()
        .any(|e| hits.iter().any(|h| ref_matches(e, h)))
}

/// numerator/denominator; 0.0 for an empty denominator.
#[allow(clippy::cast_precision_loss)] // suite sizes stay far below 2^53
#[must_use]
pub fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// The p95 of wall-time samples by nearest rank: the ceil(0.95 * n)-th
/// smallest sample. 0.0 for no samples.
#[must_use]
pub fn p95(samples: &[f64]) -> f64 {
    percentile_nearest_rank(samples, 0.95)
}

/// The nearest-rank percentile of the samples.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // rank stays in 1..=len, so the casts lose nothing
fn percentile_nearest_rank(samples: &[f64], q: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// The graded outcome of one question (docs/implementation/14 §Metrics).
#[derive(Debug, Clone, Serialize)]
pub struct QuestionScore {
    /// The binary grader outcome: partial counts as wrong.
    pub passed: bool,
    /// Any expected ref within the top five.
    pub hit_at_5: bool,
    /// Pinned symbols the results contain.
    pub symbols_found: usize,
    /// The symbols the question pins.
    pub symbols_pinned: usize,
    /// The top result serves the version the answer is pinned to. `None`
    /// when nothing was served — an abstention has no served version to be
    /// wrong — and for `unanswerable` questions, which never contribute
    /// (docs/implementation/14 §Metrics).
    pub version_correct: Option<bool>,
}

impl QuestionScore {
    /// Found / pinned symbols; `None` when the question pins none.
    #[must_use]
    pub fn symbol_recall(&self) -> Option<f64> {
        (self.symbols_pinned > 0).then(|| ratio(self.symbols_found, self.symbols_pinned))
    }
}

/// Grades one question against its retrieved results.
///
/// `unanswerable` questions pass only with an empty result set — a hit is a
/// failure (docs/implementation/14 §Golden-set format). `ref_match` scans
/// the full result list; `symbol_match` passes only at full symbol recall.
/// Symbol recall grades from `symbol_hits` — the symbol rung's hits, not
/// the packed page (docs/implementation/14 §Metrics) — while every other
/// grader grades from the served `hits`. A symbol counts only from a hit on
/// the question's own library and version pin — a `multi_library` question
/// grades its symbols against that primary pin, even though its refs may
/// target other libraries.
///
/// Version correctness grades the top hit against the version the answer
/// is pinned to: the question's own pin, except a `multi_library` top hit
/// that matches an expected ref, where that ref's version decides — a
/// correct answer may come from another library (docs/implementation/14
/// §Metrics). Nothing served leaves the metric: an abstention has no
/// served version to be wrong.
#[must_use]
pub fn grade_question(
    question: &Question,
    hits: &[RetrievedRef],
    symbol_hits: &[RetrievedRef],
) -> QuestionScore {
    let refs = question.expect_refs.as_deref().unwrap_or_default();
    let symbols = question.expect_symbols.as_deref().unwrap_or_default();
    // A symbol counts only when a rung hit from the question's own library
    // and pinned version documents it — the same library/version rule the
    // ref grader applies (docs/implementation/14 §Version correctness).
    let symbols_found = symbols
        .iter()
        .filter(|s| {
            symbol_hits.iter().any(|h| {
                h.symbol.as_deref() == Some(s.as_str())
                    && h.version == question.version
                    && h.library.eq_ignore_ascii_case(&question.library)
            })
        })
        .count();
    let passed = if question.task_type == TaskType::Unanswerable {
        hits.is_empty()
    } else {
        match question.grader {
            Grader::RefMatch => ref_match(refs, hits),
            Grader::SymbolMatch => !symbols.is_empty() && symbols_found == symbols.len(),
            Grader::Manual => false, // runs refuse manual-grader suites
        }
    };
    // The top hit grades against the version the answer is pinned to — the
    // question's own pin, except a `multi_library` top hit that matches an
    // expected ref: that ref's version decides, so a correct answer from
    // another library stays version-correct (docs/implementation/14
    // §Metrics). Nothing served leaves the metric: an abstention has no
    // served version to be wrong.
    let version_correct = if question.task_type == TaskType::Unanswerable {
        None
    } else {
        hits.first().map(|top| {
            let matched = question.task_type == TaskType::MultiLibrary
                && refs.iter().any(|e| ref_matches(e, top));
            matched
                || (top.version == question.version
                    && top.library.eq_ignore_ascii_case(&question.library))
        })
    };
    QuestionScore {
        passed,
        hit_at_5: hit_at_5(refs, hits),
        symbols_found,
        symbols_pinned: symbols.len(),
        version_correct,
    }
}

/// A count-over-count fraction with its denominator, so gates can tell a
/// measured 0.0 from "nothing qualified for the denominator".
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Fraction {
    /// The count that met the bar.
    pub hits: usize,
    /// The count the metric applies to.
    pub total: usize,
}

impl Fraction {
    /// The fraction; 0.0 when nothing qualifies for the denominator.
    #[must_use]
    pub fn value(self) -> f64 {
        ratio(self.hits, self.total)
    }

    /// The value with its raw counts, as the gate table and the report
    /// print it: `0.750 (3/4)`.
    #[must_use]
    pub fn display(self) -> String {
        format!("{:.3} ({}/{})", self.value(), self.hits, self.total)
    }
}

/// The suite-level aggregates (docs/implementation/14 §Metrics).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SuiteMetrics {
    /// Questions with non-empty `expect_refs` that had an expected ref in
    /// the top five.
    pub hit_at_5: Fraction,
    /// Pinned symbols present in the results.
    pub symbol_recall: Fraction,
    /// Questions with a wrong or partial result under the binary grader.
    pub failure_rate: Fraction,
    /// Questions where a result was served and the top result serves the
    /// version the answer is pinned to. Unanswerable abstentions leave the
    /// denominator.
    pub version_correctness: Fraction,
}

impl SuiteMetrics {
    /// Aggregates the per-question scores. hit@5 denominators count only
    /// questions with non-empty `expect_refs`; symbol recall counts pinned
    /// symbols; failure rate counts every question. Version correctness
    /// counts the questions where a result was served — unanswerable
    /// abstentions leave the denominator (docs/implementation/14 §Metrics).
    #[must_use]
    pub fn new(scored: &[(Question, QuestionScore)]) -> SuiteMetrics {
        let mut refs_total = 0;
        let mut refs_hits = 0;
        let mut symbols_pinned = 0;
        let mut symbols_found = 0;
        let mut failed = 0;
        let mut version_total = 0;
        let mut version_hits = 0;
        for (question, score) in scored {
            if score.symbols_pinned > 0 {
                symbols_pinned += score.symbols_pinned;
                symbols_found += score.symbols_found;
            }
            if !question
                .expect_refs
                .as_deref()
                .is_none_or(<[String]>::is_empty)
            {
                refs_total += 1;
                if score.hit_at_5 {
                    refs_hits += 1;
                }
            }
            if !score.passed {
                failed += 1;
            }
            // `None` (nothing served, or an unanswerable question) leaves
            // the denominator: no served version to be wrong.
            if let Some(correct) = score.version_correct {
                version_total += 1;
                version_hits += usize::from(correct);
            }
        }
        SuiteMetrics {
            hit_at_5: Fraction {
                hits: refs_hits,
                total: refs_total,
            },
            symbol_recall: Fraction {
                hits: symbols_found,
                total: symbols_pinned,
            },
            failure_rate: Fraction {
                hits: failed,
                total: scored.len(),
            },
            version_correctness: Fraction {
                hits: version_hits,
                total: version_total,
            },
        }
    }
}

#[cfg(test)]
mod tests;
