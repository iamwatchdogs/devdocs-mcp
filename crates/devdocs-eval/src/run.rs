//! The run loop (docs/implementation/14 §Harness flow): load and validate
//! the suite, run the retrieval pipeline, run the CLI-subprocess pass,
//! score, write the artifacts, and evaluate the gates.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use devdocs_model::CorpusEntry;
use devdocs_retrieve::Plan;
use devdocs_store::Store;
use sha2::{Digest, Sha256};

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
    /// The operator config file the run measures with. The harness
    /// snapshots it into the run dir at run start, and the scoring pass,
    /// the warmup, and every spawned `devdocs search` load that snapshot,
    /// so a mid-run edit of the original cannot change what the run
    /// measures. Together with the child's isolated `XDG_CONFIG_HOME` (an
    /// empty per-run scratch dir) this keeps the run hermetic against the
    /// operator's user config (docs/implementation/14 §Regression gate).
    /// When absent, the harness writes an empty `child-defaults.toml`
    /// into the run dir and hands that to the child: the explicit file
    /// skips the project `devdocs.toml` discovery the inherited cwd would
    /// otherwise trigger.
    pub config: Option<PathBuf>,
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
/// [`EvalError::InvalidSuite`], [`EvalError::SuiteParse`], or
/// [`EvalError::InvalidConfig`] for bad input (exit 1);
/// [`EvalError::Store`], [`EvalError::Subprocess`], and
/// [`EvalError::Io`] for operational failures (exit 3).
pub fn run(options: &RunOptions) -> Result<RunOutcome, EvalError> {
    if let Some(config) = options.config.as_deref() {
        validate_config(config)?;
    }
    let (golden_dir, runs_root, baseline_path) = resolve_dirs(options)?;
    let suite = load_suite(&golden_dir, &options.suite)?;
    refuse_manual_graders(&suite)?;

    let date = report::now_rfc3339();
    let run_dir = fresh_run_dir(&runs_root, &date, &suite.name)?;

    let child_config = prepare_child_config(options.config.as_deref(), &run_dir)?;

    let db_path = options
        .db
        .clone()
        .unwrap_or_else(|| run_dir.join("fixture.db"));
    let store = open_fixture_store(&db_path)?;

    // The scoring pass must search with the same plan the timed
    // subprocess resolves from `--config`, so a gate row never mixes
    // quality from defaults with latency from the config.
    let plan = resolve_plan(&child_config)?;
    let mut passes = in_process_pass(&store, &suite, &plan)?;
    let p95_ms = subprocess_pass(
        options,
        &run_dir,
        &db_path,
        &child_config,
        &suite,
        &mut passes.rows,
    )?;
    score_and_finish(
        store,
        run_dir,
        &suite,
        &date,
        &baseline_path,
        &db_path,
        p95_ms,
        &passes,
    )
}

/// The operator config is snapshotted into the run dir before anything
/// measures it: the plan resolution, the warmup, and every timed child
/// read the snapshot, so a mid-run edit of the original file cannot
/// mix two configurations into one gate table (docs/implementation/14
/// §Harness flow). The snapshot passes the same parse the original
/// passed at run start: a copy raced with an edit is an input error,
/// not a subprocess failure.
fn prepare_child_config(
    options_config: Option<&Path>,
    run_dir: &Path,
) -> Result<PathBuf, EvalError> {
    let child_config = child_config_file(options_config, run_dir)?;
    if options_config.is_some() {
        validate_config(&child_config)?;
    }
    Ok(child_config)
}

/// The fixture database comes from the store schema, never a copy: the
/// open runs the same fschecks and pragmas, and the migration is
/// idempotent (docs/implementation/06).
fn open_fixture_store(db_path: &Path) -> Result<Store, EvalError> {
    let store = Store::open(db_path)?;
    store.migrate()?;
    Ok(store)
}

/// Loads and validates the suite by name or direct path.
fn load_suite(golden_dir: &Path, requested: &str) -> Result<Suite, EvalError> {
    let requested_path = Path::new(requested);
    let (suite_path, suite_name) = if requested_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        || requested_path.is_absolute()
        || requested.contains(['/', '\\'])
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

/// Refuses a `--config` path that does not name a readable, well-formed
/// config file, at run start: the spawned `devdocs search` would otherwise
/// reject malformed contents with exit 1, whose stderr the harness
/// discards — the run would surface only as a generic subprocess failure
/// after the pipeline pass and the warmup already spent work.
fn validate_config(config: &Path) -> Result<(), EvalError> {
    if std::fs::File::open(config).is_err() {
        return Err(EvalError::InvalidConfig {
            reason: format!(
                "config file {} does not exist or is not readable",
                config.display()
            ),
        });
    }
    // The same full parse the child runs (`config validate` parity:
    // devdocs-model `Config::validate_file`), so a malformed file is an
    // input error (exit 1) instead of a subprocess failure (exit 3).
    devdocs_model::Config::validate_file(config).map_err(|error| EvalError::InvalidConfig {
        reason: format!("config file {} is invalid: {error}", config.display()),
    })
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

/// Accepts a subprocess call that exited 0 or 2 and refuses the rest. An
/// exit 2 is the documented miss — a valid query that served nothing
/// (docs/implementation/08 §Error codes) — and its wall time is a real
/// timed search. Any other nonzero exit says the call died before doing
/// real work, so the run fails (exit 3) instead of letting a dead binary
/// pass the p95 gate.
fn check_exit(sample: SubprocessSample, label: &str) -> Result<(), EvalError> {
    if sample.exit_code == 0 || sample.exit_code == 2 {
        return Ok(());
    }
    Err(EvalError::Subprocess(format!(
        "devdocs search exited {} for {label}",
        sample.exit_code
    )))
}

/// The in-process pass: one timed pipeline call per question, then the
/// per-question score (docs/implementation/14 §Harness flow step 2–3).
struct InProcess {
    /// The artifact rows, one per question.
    rows: Vec<QuestionResult>,
    /// The question/score pairs the metrics aggregate.
    graded: Vec<(Question, QuestionScore)>,
}

/// The search plan the in-process pass scores with: the knobs of the
/// child config file — the run-dir snapshot when an operator `--config`
/// is given, the empty defaults file otherwise. That is the same file
/// every spawned child resolves, and the child's isolated env leaves only
/// the file layer, so the harness loads the file layer alone
/// (devdocs-model `Config::load_file_layer`). The empty defaults file
/// parses to the documented defaults.
///
/// # Errors
/// [`EvalError::InvalidConfig`] when the file fails to load — normally
/// unreachable: `validate_config` ran the same parse on the operator's
/// original and the snapshot at run start.
fn resolve_plan(config: &Path) -> Result<Plan, EvalError> {
    let resolved = devdocs_model::Config::load_file_layer(config).map_err(|error| {
        EvalError::InvalidConfig {
            reason: format!("config file {} is invalid: {error}", config.display()),
        }
    })?;
    Ok(Plan {
        limit: resolved.search.limit_default,
        max_tokens: resolved.search.max_tokens_default,
        rrf_k: resolved.search.rrf_k,
        cursor: None,
    })
}

/// The in-process pass: one timed pipeline call per question, then the
/// per-question score (docs/implementation/14 §Harness flow step 2–3).
fn in_process_pass(store: &Store, suite: &Suite, plan: &Plan) -> Result<InProcess, EvalError> {
    let mut rows = Vec::with_capacity(suite.questions.len());
    let mut graded = Vec::with_capacity(suite.questions.len());
    for question in &suite.questions {
        let start = Instant::now();
        let retrieval = pipeline::retrieve(store, plan, question)?;
        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        let verdict = metrics::grade_question(question, &retrieval.refs, &retrieval.symbol_refs);
        rows.push(QuestionResult {
            id: question.id.clone(),
            task_type: question.task_type.as_str(),
            grader: question.grader.as_str(),
            in_process: PassResult {
                wall_ms,
                hits: retrieval
                    .refs
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

/// The config file every spawned child loads: a snapshot of the operator
/// `--config` copied into the run dir when given, else an empty defaults
/// file the harness writes there. The snapshot freezes the configuration
/// the run measures — the plan resolution, the warmup, and every timed
/// child read one immutable file, and the run dir records exactly what
/// was measured. The empty file parses to an empty layer — the child
/// runs on the documented defaults — and an explicit file path skips the
/// project `devdocs.toml` discovery that `Config::load`'s none-given arm
/// would otherwise run against the inherited cwd, where a stray file
/// would silently poison the timed p95.
///
/// The files are the harness's own artifacts; `validate_config` guards
/// the operator's original at run start and the snapshot copy once
/// written, never the empty defaults file.
///
/// # Errors
/// [`EvalError::Io`] when the file cannot be written.
fn child_config_file(options_config: Option<&Path>, run_dir: &Path) -> Result<PathBuf, EvalError> {
    let Some(config) = options_config else {
        let path = run_dir.join("child-defaults.toml");
        std::fs::write(&path, "").map_err(|source| EvalError::Io {
            action: "create",
            path: path.clone(),
            source,
        })?;
        return Ok(path);
    };
    let path = run_dir.join("child-config.toml");
    std::fs::copy(config, &path).map_err(|source| EvalError::Io {
        action: "copy",
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// The CLI-subprocess pass: one warmup excluded, then one timed
/// `devdocs search` per question (docs/implementation/14 §Harness flow).
///
/// # Errors
/// [`EvalError::Subprocess`] when the binary is missing or a spawn fails.
fn subprocess_pass(
    options: &RunOptions,
    run_dir: &Path,
    db_path: &Path,
    config: &Path,
    suite: &Suite,
    rows: &mut [QuestionResult],
) -> Result<f64, EvalError> {
    // The child's user-config layer lives in an empty scratch dir: its
    // XDG_CONFIG_HOME names it, so the spawned search never reads the
    // operator's user config, however broken (docs/implementation/14
    // §Regression gate).
    let config_home = run_dir.join("child-config-home");
    std::fs::create_dir(&config_home).map_err(|source| EvalError::Io {
        action: "create",
        path: config_home.clone(),
        source,
    })?;
    let bin = subprocess::locate_devdocs(options.devdocs_bin.as_deref())?;
    // One warmup call; the cold start is excluded (docs/implementation/14).
    let warmup = subprocess::run_once(&bin, db_path, config, &config_home, &suite.questions[0])?;
    check_exit(warmup, "warmup")?;
    let mut samples = Vec::with_capacity(rows.len());
    for (row, question) in rows.iter_mut().zip(&suite.questions) {
        let sample = subprocess::run_once(&bin, db_path, config, &config_home, question)?;
        check_exit(sample, &question.id)?;
        samples.push(sample.wall_ms);
        row.subprocess = sample;
    }
    Ok(metrics::p95(&samples))
}

/// Finishes the run: reads the corpus, scores and writes the artifacts,
/// and assembles the outcome the caller exits on
/// (docs/implementation/14 §Harness flow).
#[allow(clippy::too_many_arguments)] // the phase reads what run() resolved
fn score_and_finish(
    store: Store,
    run_dir: PathBuf,
    suite: &Suite,
    date: &str,
    baseline_path: &Path,
    db_path: &Path,
    p95_ms: f64,
    passes: &InProcess,
) -> Result<RunOutcome, EvalError> {
    // Read the corpus last, so the artifacts name the sources the hashed
    // database file holds (the read-path sweep can mutate license state).
    let corpus = store.list_corpus()?;
    drop(store);

    let (metrics, gate_rows) = score_and_write(
        &run_dir,
        suite,
        date,
        baseline_path,
        &corpus,
        db_path,
        p95_ms,
        passes,
    )?;
    let all_gates_pass = gates::all_pass(&gate_rows);
    Ok(RunOutcome {
        run_dir,
        metrics,
        gates: gate_rows,
        p95_ms,
        all_gates_pass,
    })
}

/// Scores the run, hashes the database, and writes the artifacts into
/// the run dir; returns the metrics and the evaluated gate rows.
#[allow(clippy::too_many_arguments)] // the phase reads what run() resolved
fn score_and_write(
    run_dir: &Path,
    suite: &Suite,
    date: &str,
    baseline_path: &Path,
    corpus: &[CorpusEntry],
    db_path: &Path,
    p95_ms: f64,
    passes: &InProcess,
) -> Result<(SuiteMetrics, Vec<Gate>), EvalError> {
    let metrics = SuiteMetrics::new(&passes.graded);
    let gate_rows = gates::evaluate(&metrics, Some(p95_ms));

    let db_hash = hash_file(db_path)?;
    let bundle = RunData {
        suite,
        date,
        metrics,
        p95_ms,
        gates: &gate_rows,
        db_hash: &db_hash,
        baseline_path,
        corpus,
    };
    write_artifacts(run_dir, &bundle, passes)?;
    Ok((metrics, gate_rows))
}

/// Everything the artifacts need, assembled once by the run.
struct RunData<'a> {
    /// The validated suite.
    suite: &'a Suite,
    /// The run date, RFC 3339 UTC.
    date: &'a str,
    /// The suite-level metrics.
    metrics: SuiteMetrics,
    /// The subprocess p95 in milliseconds.
    p95_ms: f64,
    /// The evaluated gate rows.
    gates: &'a [Gate],
    /// SHA-256 hex of the database file bytes.
    db_hash: &'a str,
    /// The baseline record the report and manifest carry.
    baseline_path: &'a Path,
    /// The indexed sources the run executed against.
    corpus: &'a [CorpusEntry],
}

/// Writes `results.json`, `manifest.json`, and `report.md` into the run
/// directory (docs/implementation/14 §Run artifacts).
fn write_artifacts(
    run_dir: &Path,
    data: &RunData<'_>,
    passes: &InProcess,
) -> Result<(), EvalError> {
    let baseline = report::load_baseline(data.baseline_path)?;
    write_results(run_dir, data, &passes.rows)?;
    write_manifest(
        run_dir,
        data,
        baseline.as_ref().map(|(_, hash)| hash.clone()),
    )?;
    write_report(
        run_dir,
        data,
        &passes.graded,
        baseline.as_ref().map(|(b, _)| b),
    )
}

/// Writes the per-question `results.json` rows.
fn write_results(
    run_dir: &Path,
    data: &RunData<'_>,
    rows: &[QuestionResult],
) -> Result<(), EvalError> {
    let results = ResultsDocument {
        suite: &data.suite.name,
        date: data.date,
        tool_version: env!("CARGO_PKG_VERSION"),
        db_hash: data.db_hash,
        suite_hash: &data.suite.hash,
        corpus: data.corpus,
        questions: rows,
    };
    report::write_artifact(
        &run_dir.join("results.json"),
        &report::json_artifact("results.json", &results)?,
    )
}

/// Writes the `manifest.json` that reconstructs the denominator.
fn write_manifest(
    run_dir: &Path,
    data: &RunData<'_>,
    baseline_hash: Option<String>,
) -> Result<(), EvalError> {
    let manifest = Manifest {
        tool: format!("devdocs-eval {}", env!("CARGO_PKG_VERSION")),
        suite: &data.suite.name,
        suite_hash: &data.suite.hash,
        db_hash: data.db_hash,
        host: Host::current(),
        corpus: data.corpus,
        date: data.date,
        questions: data.suite.questions.len(),
        baseline_hash,
    };
    report::write_artifact(
        &run_dir.join("manifest.json"),
        &report::json_artifact("manifest.json", &manifest)?,
    )
}

/// Writes the rendered `report.md`.
fn write_report(
    run_dir: &Path,
    data: &RunData<'_>,
    graded: &[(Question, QuestionScore)],
    baseline: Option<&report::Baseline>,
) -> Result<(), EvalError> {
    let input = ReportInput {
        suite: &data.suite.name,
        date: data.date,
        metrics: data.metrics,
        p95_ms: data.p95_ms,
        gates: data.gates,
        scored: graded,
        curation_minutes: data.suite.meta.as_ref().map(|meta| meta.curation_minutes),
        baseline,
        suite_hash: &data.suite.hash,
        db_hash: data.db_hash,
    };
    report::write_artifact(&run_dir.join("report.md"), &report::render_report(&input))
}

/// Resolves the golden dir, the runs dir, and the baseline path.
///
/// With an explicit `--golden-dir`, its parent is the eval directory and
/// the default runs and baseline paths derive from it. Otherwise the eval
/// directory is discovered by walking up from the working directory until
/// an `eval/golden` pair appears.
fn resolve_dirs(options: &RunOptions) -> Result<(PathBuf, PathBuf, PathBuf), EvalError> {
    let eval_root = match options
        .golden_dir
        .as_ref()
        .and_then(|g| g.parent().map(Path::to_path_buf))
    {
        Some(eval_dir) => eval_dir.clone(),
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
        .unwrap_or_else(|| eval_root.join("golden"));
    let runs_root = options
        .runs_dir
        .clone()
        .unwrap_or_else(|| eval_root.join("runs"));
    Ok((
        golden_dir,
        runs_root,
        eval_root.join("baseline").join("phase0.json"),
    ))
}

/// Walks up from the working directory to the `eval` directory that holds
/// `golden`.
fn discover_eval_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("eval").join("golden").is_dir() {
            return Some(dir.join("eval"));
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Creates `eval/runs/<date>-<suite>`, deduplicated with `-2`, `-3`, ….
///
/// Creation is atomic per candidate: `create_dir` fails with
/// `AlreadyExists` when another process took the directory first, so two
/// concurrent runs of one suite never share a run directory.
fn fresh_run_dir(runs_root: &Path, date: &str, suite: &str) -> Result<PathBuf, EvalError> {
    std::fs::create_dir_all(runs_root).map_err(|source| EvalError::Io {
        action: "create",
        path: runs_root.to_path_buf(),
        source,
    })?;
    let day = date.get(..10).unwrap_or("run");
    let mut run_dir = runs_root.join(format!("{day}-{suite}"));
    let mut suffix = 2;
    loop {
        match std::fs::create_dir(&run_dir) {
            Ok(()) => return Ok(run_dir),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                run_dir = runs_root.join(format!("{day}-{suite}-{suffix}"));
                suffix += 1;
            }
            Err(source) => {
                return Err(EvalError::Io {
                    action: "create",
                    path: run_dir,
                    source,
                });
            }
        }
    }
}

/// SHA-256 of the file at `path` as lowercase hex, read in buffered
/// chunks so the database never sits in memory whole (the manifest hash).
fn hash_file(db_path: &Path) -> Result<String, EvalError> {
    let file = std::fs::File::open(db_path).map_err(|source| EvalError::Io {
        action: "open",
        path: db_path.to_path_buf(),
        source,
    })?;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|source| EvalError::Io {
            action: "read",
            path: db_path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(crate::suite::hex_digest(&hasher.finalize()))
}

#[cfg(test)]
mod tests;
