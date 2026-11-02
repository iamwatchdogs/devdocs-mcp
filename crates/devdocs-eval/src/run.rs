//! The run loop (docs/implementation/14 §Harness flow): load and validate
//! the suite, run the retrieval pipeline, run the CLI-subprocess pass,
//! score, write the artifacts, and evaluate the gates.

use std::path::{Path, PathBuf};
use std::time::Instant;

use devdocs_store::Store;

use crate::error::EvalError;
use crate::gates::{self, Gate};
use crate::metrics::{self, QuestionScore, SuiteMetrics};
use crate::pipeline;
use crate::report::{
    self, HitRow, Host, Manifest, PassResult, QuestionResult, ReportInput, ResultsDocument,
};
use crate::subprocess::{self, SubprocessSample};
use crate::suite::{Grader, Question, Suite};

/// Everything a run needs (docs/implementation/14 §Harness flow).
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// The suite name under the golden dir, or a direct `.jsonl` path.
    pub suite: String,
    /// The fixture database; built from the store schema when absent.
    pub db: Option<PathBuf>,
    /// The directory that holds the golden suites; discovered from the
    /// working directory when absent.
    pub golden_dir: Option<PathBuf>,
    /// The directory that holds the run artifacts; next to the golden dir
    /// when absent.
    pub runs_dir: Option<PathBuf>,
    /// The `devdocs` binary for the subprocess pass; located when absent.
    pub devdocs_bin: Option<PathBuf>,
}

/// What a completed run reports.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// The directory holding the artifacts.
    pub run_dir: PathBuf,
    /// The suite-level metrics.
    pub metrics: SuiteMetrics,
    /// The evaluated gate rows.
    pub gates: Vec<Gate>,
    /// The subprocess p95 in milliseconds.
    pub p95_ms: f64,
    /// Whether every gate passed; `false` means the caller exits 2.
    pub all_gates_pass: bool,
}

/// Runs one suite end-to-end.
///
/// # Errors
/// [`EvalError::InvalidSuite`] or [`EvalError::SuiteParse`] for bad input
/// (exit 1); [`EvalError::Store`], [`EvalError::Subprocess`], and
/// [`EvalError::Io`] for operational failures (exit 3).
pub fn run(options: &RunOptions) -> Result<RunOutcome, EvalError> {
    let (golden_dir, runs_root, baseline_path) = resolve_dirs(options)?;
    let suite = load_suite(&golden_dir, &options.suite)?;
    refuse_manual_graders(&suite)?;

    let date = report::now_rfc3339();
    let run_dir = fresh_run_dir(&runs_root, &date, &suite.name)?;

    // The fixture database comes from the store schema, never a copy: the
    // open runs the same fschecks and pragmas, and the migration is
    // idempotent (docs/implementation/06).
    let db_path = options
        .db
        .clone()
        .unwrap_or_else(|| run_dir.join("fixture.db"));
    let store = Store::open(&db_path)?;
    store.migrate()?;

    let InProcess { mut rows, graded } = in_process_pass(&store, &suite)?;
    let p95_ms = subprocess_pass(options, &db_path, &suite, &mut rows)?;
    drop(store);

    let suite_metrics = SuiteMetrics::new(&graded);
    let gate_rows = gates::evaluate(&suite_metrics, Some(p95_ms));

    let db_hash = crate::suite::hex_hash(&read_db_bytes(&db_path)?);
    write_artifacts(
        &run_dir,
        &suite,
        &date,
        &rows,
        &graded,
        &suite_metrics,
        p95_ms,
        &gate_rows,
        &db_hash,
        &baseline_path,
    )?;

    let all_gates_pass = gates::all_pass(&gate_rows);
    Ok(RunOutcome {
        run_dir,
        metrics: suite_metrics,
        gates: gate_rows,
        p95_ms,
        all_gates_pass,
    })
}

/// Loads and validates the suite by name or direct path.
fn load_suite(golden_dir: &Path, requested: &str) -> Result<Suite, EvalError> {
    let (suite_path, suite_name) = if Path::new(requested)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        || requested.contains('/')
    {
        let path = PathBuf::from(requested);
        let name = path
            .file_stem()
            .map_or_else(|| "suite".to_owned(), |s| s.to_string_lossy().into_owned());
        (path, name)
    } else {
        (
            golden_dir.join(format!("{requested}.jsonl")),
            requested.to_owned(),
        )
    };
    Suite::load(&suite_path, &suite_name)
}

/// Refuses suites the harness cannot grade (docs/implementation/14: the
/// pilot uses binary ref/symbol graders).
fn refuse_manual_graders(suite: &Suite) -> Result<(), EvalError> {
    let manual = suite
        .questions
        .iter()
        .filter(|q| q.grader == Grader::Manual)
        .count();
    if manual > 0 {
        return Err(EvalError::InvalidSuite {
            suite: suite.name.clone(),
            reason: format!(
                "{manual} question(s) use the manual grader; the harness grades \
                 ref_match and symbol_match only"
            ),
        });
    }
    Ok(())
}

/// The in-process pass: one timed pipeline call per question, then the
/// per-question score (docs/implementation/14 §Harness flow step 2–3).
struct InProcess {
    /// The artifact rows, one per question.
    rows: Vec<QuestionResult>,
    /// The question/score pairs the metrics aggregate.
    graded: Vec<(Question, QuestionScore)>,
}

/// The in-process pass: one timed pipeline call per question, then the
/// per-question score (docs/implementation/14 §Harness flow step 2–3).
fn in_process_pass(store: &Store, suite: &Suite) -> Result<InProcess, EvalError> {
    let mut rows = Vec::with_capacity(suite.questions.len());
    let mut graded = Vec::with_capacity(suite.questions.len());
    for question in &suite.questions {
        let start = Instant::now();
        let hits = pipeline::retrieve(store, question)?;
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let verdict = metrics::grade_question(question, &hits);
        rows.push(QuestionResult {
            id: question.id.clone(),
            task_type: question.task_type.as_str(),
            grader: question.grader.as_str(),
            in_process: PassResult {
                wall_ms,
                hits: hits
                    .iter()
                    .map(|h| HitRow {
                        rank: h.rank,
                        reference: h.reference(),
                        symbol: h.symbol.clone(),
                        version: h.version.clone(),
                    })
                    .collect(),
            },
            subprocess: SubprocessSample {
                wall_ms: 0.0,
                exit_code: 0,
            },
            score: verdict.clone(),
        });
        graded.push((question.clone(), verdict));
    }
    Ok(InProcess { rows, graded })
}

/// The CLI-subprocess pass: one warmup excluded, then one timed
/// `devdocs search` per question (docs/implementation/14 §Harness flow).
///
/// # Errors
/// [`EvalError::Subprocess`] when the binary is missing or a spawn fails.
fn subprocess_pass(
    options: &RunOptions,
    db_path: &Path,
    suite: &Suite,
    rows: &mut [QuestionResult],
) -> Result<f64, EvalError> {
    let bin = subprocess::locate_devdocs(options.devdocs_bin.as_deref())?;
    // One warmup call; the cold start is excluded (docs/implementation/14).
    let _warmup = subprocess::run_once(&bin, db_path, &suite.questions[0])?;
    let mut samples = Vec::with_capacity(rows.len());
    for (row, question) in rows.iter_mut().zip(&suite.questions) {
        let sample = subprocess::run_once(&bin, db_path, question)?;
        samples.push(sample.wall_ms);
        row.subprocess = sample;
    }
    Ok(metrics::p95(&samples))
}

/// Writes `results.json`, `manifest.json`, and `report.md` into the run
/// directory (docs/implementation/14 §Run artifacts).
#[allow(clippy::too_many_arguments)]
fn write_artifacts(
    run_dir: &Path,
    suite: &Suite,
    date: &str,
    rows: &[QuestionResult],
    graded: &[(Question, QuestionScore)],
    suite_metrics: &SuiteMetrics,
    p95_ms: f64,
    gate_rows: &[Gate],
    db_hash: &str,
    baseline_path: &Path,
) -> Result<(), EvalError> {
    let results = ResultsDocument {
        suite: &suite.name,
        date,
        tool_version: env!("CARGO_PKG_VERSION"),
        db_hash,
        suite_hash: &suite.hash,
        questions: rows,
    };
    report::write_artifact(
        &run_dir.join("results.json"),
        &report::json_artifact("results.json", &results)?,
    )?;

    let baseline = report::load_baseline(baseline_path)?;
    let manifest = Manifest {
        tool: format!("devdocs-eval {}", env!("CARGO_PKG_VERSION")),
        suite: &suite.name,
        suite_hash: &suite.hash,
        db_hash,
        host: Host {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        },
        date,
        questions: suite.questions.len(),
        baseline_hash: baseline.as_ref().map(|(_, hash)| hash.clone()),
    };
    report::write_artifact(
        &run_dir.join("manifest.json"),
        &report::json_artifact("manifest.json", &manifest)?,
    )?;

    let input = ReportInput {
        suite: &suite.name,
        date,
        metrics: *suite_metrics,
        p95_ms,
        gates: gate_rows,
        scored: graded,
        curation_minutes: suite.meta.as_ref().map(|meta| meta.curation_minutes),
        baseline: baseline.as_ref().map(|(record, _)| record),
        suite_hash: &suite.hash,
        db_hash,
    };
    report::write_artifact(&run_dir.join("report.md"), &report::render_report(&input))
}

/// Resolves the golden dir, the runs dir, and the baseline path.
///
/// Explicit options win; otherwise the eval root is discovered by walking
/// up from the working directory until `eval/golden` appears.
fn resolve_dirs(options: &RunOptions) -> Result<(PathBuf, PathBuf, PathBuf), EvalError> {
    let root = options
        .golden_dir
        .as_ref()
        .and_then(|g| g.parent().map(Path::to_path_buf));
    let root = match root {
        Some(root) => root,
        None => discover_eval_root().ok_or_else(|| EvalError::InvalidSuite {
            suite: options.suite.clone(),
            reason: "cannot locate the eval/ directory from the working \
                     directory; pass --golden-dir"
                .to_owned(),
        })?,
    };
    let golden_dir = options
        .golden_dir
        .clone()
        .unwrap_or_else(|| root.join("eval").join("golden"));
    let runs_root = options
        .runs_dir
        .clone()
        .unwrap_or_else(|| root.join("eval").join("runs"));
    Ok((
        golden_dir,
        runs_root,
        root.join("eval").join("baseline").join("phase0.json"),
    ))
}

/// Walks up from the working directory to the repo root that holds
/// `eval/golden`.
fn discover_eval_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("eval").join("golden").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Creates `eval/runs/<date>-<suite>`, deduplicated with `-2`, `-3`, ….
fn fresh_run_dir(runs_root: &Path, date: &str, suite: &str) -> Result<PathBuf, EvalError> {
    let day = date.get(..10).unwrap_or("run");
    let mut run_dir = runs_root.join(format!("{day}-{suite}"));
    let mut suffix = 2;
    while run_dir.exists() {
        run_dir = runs_root.join(format!("{day}-{suite}-{suffix}"));
        suffix += 1;
    }
    std::fs::create_dir_all(&run_dir).map_err(|source| EvalError::Io {
        action: "create",
        path: run_dir.clone(),
        source,
    })?;
    Ok(run_dir)
}

/// Reads the database file bytes for the manifest hash.
fn read_db_bytes(db_path: &Path) -> Result<Vec<u8>, EvalError> {
    std::fs::read(db_path).map_err(|source| EvalError::Io {
        action: "read",
        path: db_path.to_path_buf(),
        source,
    })
}
