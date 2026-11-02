//! The CLI-subprocess pass (docs/implementation/14 §Harness flow): the
//! harness times `devdocs search` end-to-end. The p95 gate is judged on
//! these wall times, cold start excluded after one warmup call.
//!
//! # Security
//!
//! No shell is ever involved: every invocation is one
//! [`std::process::Command`] whose program and arguments travel as a
//! process argument vector (`shell=False` equivalent). No string is ever
//! concatenated into a command line, so no command-injection surface
//! exists; a question text with `; rm -rf /` is passed as one argument.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::Serialize;

use crate::error::EvalError;
use crate::suite::Question;

/// One timed subprocess call.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SubprocessSample {
    /// Wall time in milliseconds.
    pub wall_ms: f64,
    /// The process exit code; -1 when the process died to a signal.
    pub exit_code: i32,
}

/// Locates the `devdocs` binary: the explicit argument, then
/// `DEVDOCS_EVAL_DEVDOCS_BIN`, then a `target/{debug,release}/devdocs` walk
/// up from the working directory, then `PATH`.
///
/// # Errors
/// [`EvalError::Subprocess`] when nothing resolves to an executable file.
pub fn locate_devdocs(explicit: Option<&Path>) -> Result<PathBuf, EvalError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(EvalError::Subprocess(format!(
                "--devdocs-bin {} is not a file",
                path.display()
            )));
        }
        return Ok(path.to_path_buf());
    }
    if let Ok(from_env) = std::env::var("DEVDOCS_EVAL_DEVDOCS_BIN")
        && !from_env.is_empty()
    {
        let candidate = PathBuf::from(from_env);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let mut dir = std::env::current_dir()
        .map_err(|e| EvalError::Subprocess(format!("no working directory: {e}")))?;
    loop {
        for profile in ["debug", "release"] {
            let candidate = dir.join("target").join(profile).join("devdocs");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    Err(EvalError::Subprocess(
        "cannot find the devdocs binary; pass --devdocs-bin, set \
         DEVDOCS_EVAL_DEVDOCS_BIN, or run `cargo build -p devdocs` first"
            .to_owned(),
    ))
}

/// Times one `devdocs search` invocation end-to-end: process start, flag
/// parse, and dispatch (docs/implementation/08 §Latency).
///
/// The program and every argument travel as an argument vector — no shell,
/// no interpolation (see the module security note).
///
/// # Errors
/// [`EvalError::Subprocess`] when the process cannot be spawned.
pub fn run_once(bin: &Path, db: &Path, question: &Question) -> Result<SubprocessSample, EvalError> {
    let start = Instant::now();
    let output = Command::new(bin)
        .arg("--db")
        .arg(db)
        .arg("--json")
        .arg("search")
        .arg(&question.question)
        .arg("--library")
        .arg(&question.library)
        .arg("--version")
        .arg(&question.version)
        .output()
        .map_err(|e| EvalError::Subprocess(format!("cannot spawn {}: {e}", bin.display())))?;
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok(SubprocessSample {
        wall_ms,
        exit_code: output.status.code().unwrap_or(-1),
    })
}
