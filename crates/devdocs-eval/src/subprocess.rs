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
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
/// `DEVDOCS_EVAL_DEVDOCS_BIN`, then a `target/{debug,release}/devdocs`
/// walk up from the working directory (honoring `CARGO_TARGET_DIR` and the
/// platform exe suffix), then `PATH`.
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
    let exe = format!("devdocs{}", std::env::consts::EXE_SUFFIX);
    from_env()
        .or_else(|| from_target_dir(&exe))
        .or_else(|| from_path(&exe))
        .ok_or_else(|| {
            EvalError::Subprocess(
                "cannot find the devdocs binary; pass --devdocs-bin, set \
                 DEVDOCS_EVAL_DEVDOCS_BIN, or run `cargo build -p devdocs` first"
                    .to_owned(),
            )
        })
}

/// The `DEVDOCS_EVAL_DEVDOCS_BIN` override.
fn from_env() -> Option<PathBuf> {
    let from_env = std::env::var("DEVDOCS_EVAL_DEVDOCS_BIN").ok()?;
    if from_env.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(from_env);
    candidate.is_file().then_some(candidate)
}

/// The workspace target directory: `CARGO_TARGET_DIR` when set, else a
/// `target/{debug,release}` walk up from the working directory.
fn from_target_dir(exe: &str) -> Option<PathBuf> {
    let profiles = ["debug", "release"];
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR")
        && !target_dir.is_empty()
    {
        for profile in profiles {
            let candidate = Path::new(&target_dir).join(profile).join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        for profile in profiles {
            let candidate = dir.join("target").join(profile).join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// A `devdocs` on `PATH`.
fn from_path(exe: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}

/// The wall-time deadline one `devdocs search` call gets before the run
/// treats it as stalled.
const CALL_DEADLINE: Duration = Duration::from_secs(30);

/// The `try_wait` poll interval while a call runs.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Times one `devdocs search` invocation end-to-end: process start, flag
/// parse, and dispatch (docs/implementation/08 §Latency).
///
/// The program and every argument travel as an argument vector — no shell,
/// no interpolation (see the module security note). The call gets a fixed
/// deadline: a stalled child is killed and reported as an error instead of
/// blocking the run forever. The output is not captured — the harness
/// reads only the timing and the exit code.
///
/// # Errors
/// [`EvalError::Subprocess`] when the process cannot be spawned, cannot be
/// waited on, or overshoots the deadline.
pub fn run_once(bin: &Path, db: &Path, question: &Question) -> Result<SubprocessSample, EvalError> {
    let start = Instant::now();
    let mut child = Command::new(bin)
        .arg("--db")
        .arg(db)
        .arg("--json")
        .arg("search")
        .arg(&question.question)
        .arg("--library")
        .arg(&question.library)
        .arg("--version")
        .arg(&question.version)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| EvalError::Subprocess(format!("cannot spawn {}: {e}", bin.display())))?;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                return Ok(SubprocessSample {
                    wall_ms,
                    exit_code: status.code().unwrap_or(-1),
                });
            }
            Ok(None) if start.elapsed() >= CALL_DEADLINE => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EvalError::Subprocess(format!(
                    "devdocs search exceeded the {}s deadline; the process was killed",
                    CALL_DEADLINE.as_secs()
                )));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(source) => {
                return Err(EvalError::Subprocess(format!(
                    "cannot wait for {}: {source}",
                    bin.display()
                )));
            }
        }
    }
}
